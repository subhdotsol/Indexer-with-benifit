mod domain;
mod application;
mod adapters;
mod infrastructure;

use std::sync::Arc;

use solana_client::rpc_client::RpcClient;
use tokio::sync::Mutex;

use crate::{
    adapters::{
        FileSourceAdaptor, GrpcSourceAdaptor,
        JupiterVixenParser, PostgresRepository, PumpFunParser,
        RaydiumAmmParser, SplTokenTransfer, TelegramNotifier,
        run_backfill_producer,
    },
    application::{
        EventBuffer, IngestionPipeline, NotificationService,
        TransactionParser, TransactionRepository, TransactionSource,
    },
    domain::{ChainEvent, IndexerState},
    infrastructure::MemoryBuffer,
};

#[derive(Debug, PartialEq)]
enum SourceMode {
    File,
    Grpc,
}

impl SourceMode {
    fn from_env() -> Result<Self, String> {
        match std::env::var("SOURCE_TYPE").as_deref() {
            Ok("file") => Ok(Self::File),
            Ok("grpc") => Ok(Self::Grpc),
            Ok(other) => Err(format!("Unknown SOURCE_TYPE: {}", other)),
            Err(_) => Err("SOURCE_TYPE not set".to_string()),
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    dotenv::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let source_mode = SourceMode::from_env()?;
    let db_url  = std::env::var("DATABASE_URL").expect("DATABASE_URL required");
    let rpc_url = std::env::var("RPC_URL").expect("RPC_URL required");

    // Optional Telegram alerts
    let notifier_service = match (
        std::env::var("TELEGRAM_BOT_TOKEN").ok(),
        std::env::var("TELEGRAM_CHAT_ID").ok(),
    ) {
        (Some(token), Some(chat_id)) => {
            tracing::info!("Telegram notifications enabled");
            let adapter = Arc::new(TelegramNotifier::new(token, chat_id));
            Some(Arc::new(NotificationService::new(adapter, 1_000_000_000)))
        }
        _ => {
            tracing::warn!("Telegram credentials not set — notifications disabled");
            None
        }
    };

    tracing::info!("Connecting to database...");
    let repo = Arc::new(
        PostgresRepository::new(&db_url)
            .await
            .expect("Failed to connect to PostgreSQL"),
    );

    let source: Arc<Mutex<dyn TransactionSource>> = if source_mode == SourceMode::File {
        Arc::new(Mutex::new(FileSourceAdaptor::new(50_000)))
    } else {
        let grpc_url   = std::env::var("GRPC_URL").unwrap_or_else(|_| "http://127.0.0.1:10000".to_string());
        let grpc_token = std::env::var("GRPC_TOKEN").ok();
        tracing::info!("Connecting to gRPC at {}", grpc_url);
        let adaptor = GrpcSourceAdaptor::connect(grpc_url, grpc_token)
            .await
            .expect("Failed to connect to gRPC endpoint");
        Arc::new(Mutex::new(adaptor))
    };

    let (buffer, rx) = MemoryBuffer::new(50_000);
    let buffer = Arc::new(buffer);

    let last_slot = repo.get_last_slot().await.unwrap_or(0);
    let network_slot = RpcClient::new(&rpc_url).get_slot().unwrap_or(0);
    tracing::info!("Resuming from slot {} (network tip: {})", last_slot, network_slot);

    let parsers: Vec<Box<dyn TransactionParser>> = vec![
        Box::new(SplTokenTransfer::new()),
        Box::new(RaydiumAmmParser::new()),
        Box::new(JupiterVixenParser::new()),
        Box::new(PumpFunParser::new()),
    ];

    // Producer: fetch events from source and push into the shared buffer
    let source_clone = source.clone();
    let buffer_clone = buffer.clone();
    tokio::spawn(async move {
        tracing::info!("Fetcher task started");
        loop {
            let event = source_clone.lock().await.next_event().await;
            match event {
                Ok(Some(ev)) => {
                    if buffer_clone.produce(ev).await.is_err() {
                        tracing::error!("Buffer closed — stopping fetcher");
                        break;
                    }
                }
                Ok(None) => {
                    tracing::info!("Source stream exhausted");
                    break;
                }
                Err(e) => {
                    tracing::error!("Source error: {:?}", e);
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            }
        }
    });

    // Consumer: parse events and persist in batches
    let mut pipeline = IngestionPipeline::new(rx, repo, parsers, notifier_service);
    tracing::info!("Ingestion pipeline running");
    pipeline.run().await;

    Ok(())
}
