use std::sync::Arc;
use tokio::{sync::{Mutex, mpsc}};
use crate::{
    application::{TransactionSource, TransactionParser}, 
    domain::{SolanaTransaction, ChainEvent}
};

pub struct IngestionPipeline{
    source: Arc<Mutex<dyn TransactionSource>>,
    parsers: Vec<Arc<dyn TransactionParser>>,
}

impl IngestionPipeline {
    pub fn new(
        source: Arc<Mutex<dyn TransactionSource>>,
        parsers: Vec<Arc<dyn TransactionParser>>
    ) -> Self {
        Self { source, parsers }
    }

    pub async fn run(&self){
        let (tx, mut rx) = mpsc::channel::<ChainEvent>(1000);

        // Consumer loop
        let parsers_clone = self.parsers.clone();
        let handle = tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                match event {
                    ChainEvent::Transaction(txn) => {
                        tracing::info!("Consumer received transaction: {}", txn.signature);
                        // Run all parsers
                        for parser in &parsers_clone {
                            if let Some(events) = parser.parse(&txn) {
                                for event in events {
                                    tracing::info!("Parser [{}] found event: {:?}", parser.name(), event);
                                }
                            }
                        }
                    },
                    ChainEvent::BlockMeta { slot, block_hash, .. } => {
                        tracing::info!("Consumer received block meta: slot={}, hash={}", slot, block_hash);
                    }
                }
            }
            println!("Consumer finished");
        });

        // Producer loop
        let source_clone = self.source.clone();

        loop{
            let tx_next = source_clone.lock().await.next_event().await;

            match tx_next {
                Ok(Some(event))=>{
                    match &event {
                        ChainEvent::Transaction(txn) => tracing::info!("Producer sending transaction: {}", txn.signature),
                        ChainEvent::BlockMeta { slot, .. } => tracing::info!("Producer sending block meta for slot: {}", slot),
                    }
                    if let Err(_) = tx.send(event).await {
                        break;
                    }
                },
                Ok(None)=>{
                    tracing::info!("Source reached end of stream.");
                    break;
                },
                Err(err)=>{
                    eprintln!("Error reading source: {:?}", err);
                    break;
                }
            }
        }
        
        let _ = handle.await;
    }
}
