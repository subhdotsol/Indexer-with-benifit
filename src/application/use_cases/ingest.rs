use std::{sync::Arc, time::Duration};

use tokio::sync::mpsc;

use crate::{
    application::{NotificationService, TransactionParser, TransactionRepository},
    domain::{ChainEvent, SwapEvent, TransactionEvent},
};

pub struct IngestionPipeline {
    rx: mpsc::Receiver<ChainEvent>,
    repo: Arc<dyn TransactionRepository>,
    parsers: Vec<Box<dyn TransactionParser>>,
    notifier: Option<Arc<NotificationService>>,
}

impl IngestionPipeline {
    pub fn new(
        rx: mpsc::Receiver<ChainEvent>,
        repo: Arc<dyn TransactionRepository>,
        parsers: Vec<Box<dyn TransactionParser>>,
        notifier: Option<Arc<NotificationService>>,
    ) -> Self {
        Self { rx, repo, parsers, notifier }
    }

    pub async fn run(&mut self) {
        let repo_clone = self.repo.clone();

        let mut batch: Vec<TransactionEvent> = Vec::with_capacity(100);
        let mut latest_slot: u64 = 0;

        let flush_interval = tokio::time::interval(Duration::from_millis(1000));
        tokio::pin!(flush_interval);

        loop {
            tokio::select! {
                Some(event) = self.rx.recv() => {
                    match event {
                        ChainEvent::BlockMeta { slot, .. } => {
                            latest_slot = slot;
                        }
                        ChainEvent::Transaction(txn) => {
                            for parser in &self.parsers {
                                match parser.parse(txn.clone()) {
                                    Ok(Some(events)) => {
                                        if let Some(notifier) = self.notifier.clone() {
                                            for ev in &events {
                                                let alert = match ev {
                                                    TransactionEvent::RaydiumSwap(s) => Some(SwapEvent::Raydium(s.clone())),
                                                    TransactionEvent::JupiterSwap(s) => Some(SwapEvent::Jupiter(s.clone())),
                                                    TransactionEvent::PumpFunTrade(t) => Some(SwapEvent::PumpFun(t.clone())),
                                                    _ => None,
                                                };
                                                if let Some(alert) = alert {
                                                    notifier.send_to_queue(&alert).await;
                                                }
                                            }
                                        }
                                        batch.extend(events);
                                    }
                                    Ok(None) => continue,
                                    Err(e) => {
                                        tracing::warn!("Parser {} failed: {:?}", parser.name(), e);
                                        if let Err(db_err) = repo_clone.save_dlq(&txn, parser.name(), &e.to_string()).await {
                                            tracing::error!("DLQ write failed (double fault): {}", db_err);
                                        }
                                    }
                                }
                            }

                            if batch.len() >= 100 {
                                if let Err(e) = repo_clone.save_batch(&batch, latest_slot).await {
                                    tracing::error!("Batch DB write error: {}", e);
                                }
                                batch.clear();
                            }
                        }
                    }
                }

                _ = flush_interval.tick() => {
                    if !batch.is_empty() {
                        if let Err(e) = repo_clone.save_batch(&batch, latest_slot).await {
                            tracing::error!("Flush DB write error: {}", e);
                        }
                        batch.clear();
                    }
                }
            }
        }
    }
}
