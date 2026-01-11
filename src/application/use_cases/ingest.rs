use std::sync::Arc;
use tokio::{sync::{Mutex, mpsc}};
use crate::{
    application::{TransactionSource, TransactionParser}, 
    domain::{SolanaTransaction}
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
        let (tx, mut rx) = mpsc::channel::<SolanaTransaction>(1000);

        // Consumer loop
        let parsers_clone = self.parsers.clone();
        let handle = tokio::spawn(async move {
            while let Some(txn) = rx.recv().await {
                tracing::info!("Consumer received transaction: {}", txn.signature);
                // Run all parsers
                for parser in &parsers_clone {
                    if let Some(events) = parser.parse(&txn) {
                        for event in events {
                            tracing::info!("Parser [{}] found event: {:?}", parser.name(), event);
                        }
                    }
                }
            }
            println!("Consumer finished");
        });

        // Producer loop
        let source_clone = self.source.clone();

        loop{
            let tx_next = source_clone.lock().await.next_transaction().await;

            match tx_next {
                Ok(Some(txn))=>{
                    tracing::info!("Producer sending transaction: {}", txn.signature);
                    if let Err(_) = tx.send(txn).await {
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
