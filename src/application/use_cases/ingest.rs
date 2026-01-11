use crate::{
    application::{EventRepository, TransactionParser, TransactionSource},
    domain::ChainEvent,
};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

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
        let (tx, mut rx) = mpsc::channel::<ChainEvent>(1000);

        let parsers_clone = self.parsers.clone();
        let repo_clone = self.repository.clone();

        let handle = tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                match event {
                    ChainEvent::Transaction(txn) => {
                        tracing::info!("Consumer received transaction: {}", txn.signature);
                        for parser in &parsers_clone {
                            if let Some(events) = parser.parse(&txn) {
                                for event in &events {
                                    tracing::info!(
                                        "Parser [{}] found event: {:?}",
                                        parser.name(),
                                        event
                                    );

                                    // Persist if repository is configured
                                    if let Some(ref repo) = repo_clone {
                                        if let Err(e) = repo.save_event(event).await {
                                            tracing::error!("Failed to persist event: {:?}", e);
                                        } else {
                                            tracing::info!("Persisted event to database");
                                        }
                                    }
                                }
                            }
                        }
                    }
                    ChainEvent::BlockMeta {
                        slot, block_hash, ..
                    } => {
                        tracing::info!(
                            "Consumer received block meta: slot={}, hash={}",
                            slot,
                            block_hash
                        );
                    }
                }
            }
            println!("Consumer finished");
        });

        // Producer loop
        let source_clone = self.source.clone();
        loop {
            let tx_next = source_clone.lock().await.next_event().await;
            match tx_next {
                Ok(Some(event)) => {
                    match &event {
                        ChainEvent::Transaction(txn) => {
                            tracing::info!("Producer sending transaction: {}", txn.signature)
                        }
                        ChainEvent::BlockMeta { slot, .. } => {
                            tracing::info!("Producer sending block meta for slot: {}", slot)
                        }
                    }
                    if tx.send(event).await.is_err() {
                        break;
                    }
                }
                Ok(None) => {
                    tracing::info!("Source reached end of stream.");
                    break;
                }
                Err(err) => {
                    eprintln!("Error reading source: {:?}", err);
                    break;
                }
            }
        }

        let _ = handle.await;
    }
}
