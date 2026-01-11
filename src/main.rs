mod adapters;
mod application;
mod domain;

use crate::{
    adapters::{
        parsers::{JupiterParser, PumpFunParser, RaydiumAmmParser, SplTokenParser},
        FileSourceAdaptor, GrpcSourceAdaptor, PostgresRepository,
    },
    application::{IngestionPipeline, TransactionParser, TransactionSource},
};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, PartialEq)]
enum SourceType {
    File,
    Grpc,
}

impl SourceType {
    fn from_env() -> Result<Self, String> {
        let raw =
            std::env::var("SOURCE_TYPE").map_err(|_| "Source Type is not provided".to_string())?;

        match raw.to_lowercase().as_str() {
            "file" => Ok(SourceType::File),
            "grpc" => Ok(SourceType::Grpc),
            _ => Err(format!("Invalid SOURCE_TYPE: {}", raw)),
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv::dotenv().ok();

    tracing_subscriber::fmt::init();

    let source_type = SourceType::from_env().unwrap_or(SourceType::File);

    tracing::info!("Initializing Solana Indexer (Clean Arch)");

    // Dependency Injection - Source
    let source: Arc<Mutex<dyn TransactionSource>> = if source_type == SourceType::File {
        tracing::info!("Using simulated File Source");
        Arc::new(Mutex::new(FileSourceAdaptor::new(10)))
    } else {
        let endpoint = std::env::var("GRPC_ENDPOINT").expect("GRPC_ENDPOINT must be set");
        let token = std::env::var("X_TOKEN").ok();
        Arc::new(Mutex::new(
            GrpcSourceAdaptor::connect(endpoint, token)
                .await
                .expect("Failed to connect to gRPC"),
        ))
    };

    // Dependency Injection - Parsers
    let parsers: Vec<Arc<dyn TransactionParser>> = vec![
        Arc::new(SplTokenParser),
        Arc::new(RaydiumAmmParser::new()),
        Arc::new(JupiterParser::new()),
        Arc::new(PumpFunParser::new()),
    ];

    // Dependency Injection - Database Repository
    let repository = if let Ok(database_url) = std::env::var("DATABASE_URL") {
        tracing::info!("Connecting to PostgreSQL...");
        match PostgresRepository::new(&database_url).await {
            Ok(repo) => {
                tracing::info!("Connected to PostgreSQL successfully");
                Some(Arc::new(repo))
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to connect to PostgreSQL: {}. Running without persistence.",
                    e
                );
                None
            }
        }
    } else {
        tracing::info!("DATABASE_URL not set. Running without persistence.");
        None
    };

    // Ingestion Pipeline
    let mut pipeline = IngestionPipeline::new(source, parsers);

    if let Some(repo) = repository {
        pipeline = pipeline.with_repository(repo);
    }

    tracing::info!("Starting Ingestion Pipeline...");
    pipeline.run().await;

    Ok(())
}
