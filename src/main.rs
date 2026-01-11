//! Solana Indexer - Main Entry Point
//!
//! This binary connects to a Solana gRPC endpoint (e.g., Yellowstone/Triton),
//! streams real-time transactions, parses them for DeFi events, and optionally
//! persists them to PostgreSQL.
//!
//! Required environment variables:
//! - GRPC_ENDPOINT: The gRPC endpoint URL (e.g., https://grpc.triton.one)
//! - GRPC_TOKEN: Optional authentication token for the gRPC endpoint
//! - DATABASE_URL: Optional PostgreSQL connection string for persistence

use my_solana_indexer::{
    adapters::{
        GrpcSourceAdaptor, JupiterParser, PostgresRepository, PumpFunParser, RaydiumAmmParser,
        SplTokenParser,
    },
    application::{EventRepository, IngestionPipeline, TransactionParser},
};
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load environment variables
    dotenv::dotenv().ok();

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    tracing::info!("Starting Solana Indexer...");

    // Get gRPC configuration
    let grpc_endpoint = std::env::var("GRPC_ENDPOINT").expect("GRPC_ENDPOINT must be set");
    let grpc_token = std::env::var("GRPC_TOKEN").ok();

    // Connect to gRPC source
    let source = GrpcSourceAdaptor::connect(grpc_endpoint, grpc_token).await?;
    let source: Arc<Mutex<dyn my_solana_indexer::application::TransactionSource>> =
        Arc::new(Mutex::new(source));

    // Set up parsers
    let parsers: Vec<Arc<dyn TransactionParser>> = vec![
        Arc::new(SplTokenParser),
        Arc::new(RaydiumAmmParser),
        Arc::new(JupiterParser),
        Arc::new(PumpFunParser),
    ];

    // Connect to database if DATABASE_URL is set
    let repository: Option<Arc<dyn EventRepository>> =
        if let Ok(database_url) = std::env::var("DATABASE_URL") {
            tracing::info!("Connecting to PostgreSQL...");
            match PostgresRepository::new(&database_url).await {
                Ok(repo) => {
                    tracing::info!("Connected to PostgreSQL successfully!");
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
            tracing::warn!("DATABASE_URL not set. Running without persistence.");
            None
        };

    // Build the ingestion pipeline
    let mut pipeline = IngestionPipeline::new(source, parsers);

    if let Some(repo) = repository {
        pipeline = pipeline.with_repository(repo);
    }

    // Run the pipeline
    pipeline.run().await;

    Ok(())
}
