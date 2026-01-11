//! Ingestion Pipeline
//!
//! This module orchestrates the main data flow of the Solana indexer:
//! 1. Reads chain events (transactions, block metadata) from a TransactionSource
//! 2. Parses transactions using configured parsers (SPL, Raydium, Jupiter, PumpFun)
//! 3. Optionally persists parsed events to the database via background queue
//!
//! The pipeline uses background queue persistence to decouple parsing from slow DB writes.
//! Events are sent to an mpsc channel and a background task handles batch inserts.

use crate::application::{EventRepository, TransactionParser, TransactionSource};
use crate::domain::{ChainEvent, TransactionEvent};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Mutex};

/// Configuration for the background persistence queue
const QUEUE_CAPACITY: usize = 1000;
const BATCH_SIZE: usize = 50;
const FLUSH_INTERVAL_MS: u64 = 500;

pub struct IngestionPipeline {
    source: Arc<Mutex<dyn TransactionSource>>,
    parsers: Vec<Arc<dyn TransactionParser>>,
    repository: Option<Arc<dyn EventRepository>>,
}

impl IngestionPipeline {
    pub fn new(
        source: Arc<Mutex<dyn TransactionSource>>,
        parsers: Vec<Arc<dyn TransactionParser>>,
    ) -> Self {
        Self {
            source,
            parsers,
            repository: None,
        }
    }

    pub fn with_repository(mut self, repo: Arc<dyn EventRepository>) -> Self {
        self.repository = Some(repo);
        self
    }

    pub async fn run(&self) {
        tracing::info!("Starting ingestion pipeline...");

        // Set up the background persistence channel if repository is configured
        let (tx, rx) = mpsc::channel::<TransactionEvent>(QUEUE_CAPACITY);

        if let Some(ref repo) = self.repository {
            tracing::info!(
                queue_capacity = QUEUE_CAPACITY,
                batch_size = BATCH_SIZE,
                flush_interval_ms = FLUSH_INTERVAL_MS,
                "Database persistence enabled with background queue"
            );

            // Spawn background persistence task
            let repo_clone = Arc::clone(repo);
            tokio::spawn(async move {
                background_persistence_task(rx, repo_clone).await;
            });
        } else {
            tracing::warn!("No repository configured - events will NOT be persisted");
            // Drop receiver so sender.send() will error gracefully
            drop(rx);
        }

        loop {
            // Get next event from source
            let event = {
                let mut source = self.source.lock().await;
                match source.next_event().await {
                    Ok(Some(event)) => event,
                    Ok(None) => {
                        tracing::info!("Source exhausted, stopping pipeline");
                        break;
                    }
                    Err(e) => {
                        tracing::error!("Error getting next event: {:?}", e);
                        // Add small delay to prevent tight loop on persistent errors
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        continue;
                    }
                }
            };

            // Only process transactions, log block metadata
            let tx_data = match event {
                ChainEvent::Transaction(tx_data) => tx_data,
                ChainEvent::BlockMeta {
                    slot, block_hash, ..
                } => {
                    tracing::debug!(slot = slot, block_hash = %block_hash, "Block metadata received");
                    continue;
                }
            };

            // Parse the transaction with all parsers
            for parser in &self.parsers {
                if let Some(events) = parser.parse(&tx_data) {
                    for event in events {
                        tracing::debug!(
                            parser = parser.name(),
                            signature = %tx_data.signature,
                            "Parsed event"
                        );

                        // Send to background queue (non-blocking)
                        if self.repository.is_some() {
                            if let Err(e) = tx.try_send(event) {
                                match e {
                                    mpsc::error::TrySendError::Full(_) => {
                                        tracing::warn!("Persistence queue full, event dropped");
                                    }
                                    mpsc::error::TrySendError::Closed(_) => {
                                        tracing::error!("Persistence queue closed");
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        tracing::info!("Ingestion pipeline stopped");
    }
}

/// Background task that receives events from the channel and persists them in batches
async fn background_persistence_task(
    mut rx: mpsc::Receiver<TransactionEvent>,
    repo: Arc<dyn EventRepository>,
) {
    tracing::info!("Background persistence task started");

    let mut buffer: Vec<TransactionEvent> = Vec::with_capacity(BATCH_SIZE);
    let mut interval = tokio::time::interval(Duration::from_millis(FLUSH_INTERVAL_MS));

    loop {
        tokio::select! {
            // Try to receive events from the channel
            event = rx.recv() => {
                match event {
                    Some(e) => {
                        buffer.push(e);

                        // Flush when batch is full
                        if buffer.len() >= BATCH_SIZE {
                            flush_buffer(&mut buffer, &repo).await;
                        }
                    }
                    None => {
                        // Channel closed, flush remaining and exit
                        tracing::info!("Persistence channel closed, flushing remaining events");
                        if !buffer.is_empty() {
                            flush_buffer(&mut buffer, &repo).await;
                        }
                        break;
                    }
                }
            }
            // Periodic flush to ensure events don't sit in buffer too long
            _ = interval.tick() => {
                if !buffer.is_empty() {
                    flush_buffer(&mut buffer, &repo).await;
                }
            }
        }
    }

    tracing::info!("Background persistence task stopped");
}

/// Flush the buffer to the database
async fn flush_buffer(buffer: &mut Vec<TransactionEvent>, repo: &Arc<dyn EventRepository>) {
    let events: Vec<TransactionEvent> = buffer.drain(..).collect();
    let count = events.len();

    match repo.save_events_batch(events).await {
        Ok(saved) => {
            tracing::debug!(count = saved, "Flushed events to database");
        }
        Err(e) => {
            tracing::error!(count = count, error = ?e, "Failed to flush events to database");
        }
    }
}
