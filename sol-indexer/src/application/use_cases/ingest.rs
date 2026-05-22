
use std::{sync::Arc, time::Duration};

use tokio::{sync::{mpsc}};

use crate::{application::{NotificationService, TransactionParser, TransactionRepository}, domain::{ChainEvent, SwapEvent, TransactionEvent}};

pub struct IngestionPipeline{
    // Dependency injection using Trait Object
    rx: mpsc::Receiver<ChainEvent>,
    repo: Arc<dyn TransactionRepository>,
    // This stores a list of parsers
    parsers:Vec<Box<dyn TransactionParser>>,
    notifier:Option<Arc<NotificationService>>,
}

impl IngestionPipeline {
    pub fn new(rx:mpsc::Receiver<ChainEvent>,repo:Arc<dyn TransactionRepository>,parsers:Vec<Box<dyn TransactionParser>>, notification_service: Option<Arc<NotificationService>>)->Self{
        Self { repo,rx,parsers,notifier:notification_service }
    }

    pub async fn run(&mut self){
        // ring buffer
        let repo_clone = self.repo.clone();
        // Spawing Consumer

        let mut batch:Vec<TransactionEvent> = Vec::with_capacity(100);
        let mut latest_slot = 0;
        let mut latest_block_hash = String::new();

        // timer to force flush if batch isn't full
        let flush_interval = tokio::time::interval(Duration::from_millis(1000));
        tokio::pin!(flush_interval);

        loop{
            tokio::select! {
                // Branch : 1
                Some(event) = self.rx.recv()=>{

                    match event {
                        ChainEvent::BlockMeta { slot, block_hash, parent_block_hash }=>{
                            latest_block_hash = block_hash;
                            latest_slot = slot;
                        },
                        ChainEvent::Transaction(txn)=>{

                            for parser in &self.parsers {
                                match parser.parse(txn.clone()){
                                    Ok(Some(data)) => {
                                        if let Some(notifier) = self.notifier.clone(){
                                            for event in &data {
                                                match event {
                                                    TransactionEvent::RaydiumSwap(swap) => {
                                                        let alert = SwapEvent::Raydium(swap.clone());
                                                        notifier.send_to_queue(&alert).await;
                                                    }
                                                    TransactionEvent::JupiterSwap(swap) => {
                                                        let alert = SwapEvent::Jupiter(swap.clone());
                                                        notifier.send_to_queue(&alert).await;
                                                    }
                                                    TransactionEvent::PumpFunTrade(trade) => {
                                                        let alert = SwapEvent::PumpFun(trade.clone());
                                                        notifier.send_to_queue(&alert).await;
                                                    }
                                                    _ => {}
                                                }
                                            }
                                        }

                                        batch.extend(data);
                                    },
                                    Ok(None) => {
                                        // No events parsed, continue
                                        continue;
                                    },
                                    Err(e) => {
                                        tracing::warn!("Parser {} error: {:?}", parser.name(), e);

                                        match self.repo.save_dlq(&txn, parser.name(), &e.to_string()).await {
                                            Ok(_) => tracing::info!("Saved to DLQ: {} (parser: {})", txn.signature, parser.name()),
                                            Err(db_err) => tracing::error!("Failed to save to DLQ (Double Fault): {}", db_err),
                                        }
                                    }
                                }
                            }

                            // If batch is full, write (IO bound)
                            if batch.len() >= 100 {
                                if let Err(e) = repo_clone.save_batch(&batch,latest_slot).await {
                                    tracing::error!("DB Error: {}", e);
                                }
                                batch.clear();
                            }
                        }

                    }
                }

                // Branch : 2
                _= flush_interval.tick() =>{
                    if !batch.is_empty() {
                        if let Err(e) = repo_clone.save_batch(&batch,latest_slot).await {
                            tracing::error!("DB Error: {}", e);
                        }
                        batch.clear();
                    }
                }
            }
        }

    }
}

