use std::sync::Arc;
use tokio::{sync::{Mutex, mpsc}};
use crate::{application::TransactionSource, domain::SolanaTransaction};

pub struct IngestionPipeline{
    // Dependency injection using Trait Object
    source: Arc<Mutex<dyn TransactionSource>>,
}

impl IngestionPipeline {
    pub fn new(source:Arc<Mutex<dyn TransactionSource>>)->Self{
        Self { source }
    }

    pub async fn run(&self){
        // ring buffer
        let (tx,mut rx) = mpsc::channel::<SolanaTransaction>(1000);

        // Spawing Consumer 
        let handle = tokio::spawn(async move {
            while let Some(tx) = rx.recv().await {
                // parsing & DB write happen here
                println!("Processed transaction: {:?}", tx.signature);
            }
            println!("Consumer finished");
        });


        // Producer loop
        let source_clone = self.source.clone();

        loop{
            let tx_next = source_clone.lock().await.next_transaction().await;

            match tx_next {
                Ok(Some(txn))=>{
                    if let Err(_) = tx.send(txn).await {
                        break;
                    }
                },
                Ok(None)=>{
                    println!("Stream finished.");
                    break;
                },
                Err(err)=>{
                    eprintln!("Error reading source: {:?}", err);
                    break;
                }
            }
        }
    }
}
