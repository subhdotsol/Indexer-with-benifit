mod domain;
mod application;
mod adapters;
mod infrastructure;

use std::sync::{Arc};
use solana_client::{rpc_client::RpcClient};
use tokio::sync::Mutex;

use crate::{adapters::{FileSourceAdaptor, GrpcSourceAdaptor, JupiterVixenParser, PostgresRepository, PumpFunParser, RaydiumAmmParser, SplTokenTransfer, TelegramAdaptor, run_backfill_producer}, application::{EventBuffer, IngestionPipeline, NotificationService, TransactionParser, TransactionRepository, TransactionSource}, domain::{ChainEvent, IndexerState}, infrastructure::MemoryBuffer};

#[derive(Debug,PartialEq)]
enum SourceType{
    File,
    Grpc
}

impl SourceType{
    fn from_env()->Result<Self,String>{
        let raw = std::env::var("SOURCE_TYPE").map_err(|_| "Source Type is not provied".to_string())?;

        match raw.to_lowercase().as_str() {
            "file" => Ok(SourceType::File),
            "grpc" => Ok(SourceType::Grpc),
            _ => Err(format!("Invalid SOURCE_TYPE: {}", raw)),
        }
    }
}

#[tokio::main]
async fn main()->Result<(),Box<dyn std::error::Error>>{

    // Install crypto provider for TLS (required for rustls)
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install crypto provider");

    dotenv::dotenv().ok();

    tracing_subscriber::fmt::init();

    let source_type = SourceType::from_env()?;
    let db_url = std::env::var("DATABASE_URL").expect("Database URL is not provided");
    let rpc_url = std::env::var("RPC_URL").expect("RPC URL is not provided");
    let telegram_token = std::env::var("TELEGRAM_BOT_TOKEN").ok();
    let telegram_chat_id = std::env::var("TELEGRAM_CHAT_ID").ok();

    let notifier_service = if let (Some(token), Some(chat_id)) = (telegram_token, telegram_chat_id) {
        tracing::info!("Telegram Notifications Enabled");

        let adapter = Arc::new(TelegramAdaptor::new(token, chat_id));

        // Alert if Input Amount > 1 SOL
        // You should ideally check USD value, but raw amount is fine for now.
        Some(Arc::new(NotificationService::new(adapter, 1_000_000_000)))
    } else {
        tracing::warn!("Telegram credentials missing. Notifications disabled.");
        None
    };

    tracing::info!("Initializing Solana Indexer");

    let postgres_db = PostgresRepository::new(&db_url).await.expect("Failed to connect to the DB");
    let repo = Arc::new(postgres_db);

    // Dependency Injection
    let source : Arc<Mutex<dyn TransactionSource>> = if source_type == SourceType::File {
        Arc::new(Mutex::new(FileSourceAdaptor::new(50_000)))
    }else {
        // panic!("gRPC Source not implemented yet");
        let grpc_url = std::env::var("GRPC_URL").unwrap_or("http://127.0.0.1:10000".to_string());
        let grpc_token = std::env::var("GRPC_TOKEN").ok();
        tracing::info!("Connecting to gRPC Source at {}", grpc_url);
        let source_adaptor = GrpcSourceAdaptor::connect(grpc_url, grpc_token).await.expect("Failed to connect to gRPC endpoint.");

        Arc::new(Mutex::new(source_adaptor))
    };

    let (buffer,rx) = MemoryBuffer::new(50_000);
    let buffer = Arc::new(buffer);

    let last_slot = repo.get_last_slot().await.unwrap_or(0);
    let current_network_slot = RpcClient::new(&rpc_url).get_slot().unwrap();
    tracing::info!("Resuming from Slot: {}", last_slot);

    let parsers : Vec<Box<dyn TransactionParser>> = vec![
        Box::new(SplTokenTransfer::new()),
        Box::new(RaydiumAmmParser::new()),
        Box::new(JupiterVixenParser::new()),
        Box::new(PumpFunParser::new()),
    ];

    // Backfill strategy - diff < 2000 RPC
    // if it is greater than 2000, then I am assuming we are probably starting the indexer for the first time in the mainnet
    // if current_network_slot - last_slot < 2000 {
    //     tracing::info!("Huge Gap Detected, Starting Backfill Mode...");

    //     let (buffer, rx) = MemoryBuffer::new(50_000);
    //     let buffer = Arc::new(buffer);

    //     let rpc_url_clone = rpc_url.clone();
    //     tokio::spawn(async move {
    //         run_backfill_producer(
    //             rpc_url_clone,
    //             buffer,
    //             last_slot,
    //             current_network_slot
    //         ).await;
    //     });

    //     let backfill_parsers:  Vec<Box<dyn TransactionParser>>  = vec![
    //         Box::new(SplTokenTransfer::new()),
    //         Box::new(RaydiumAmmParser::new()),
    //         Box::new(JupiterVixenParser::new()),
    //         Box::new(PumpFunParser::new()),
    //     ];

    //     let mut pipeline = IngestionPipeline::new(rx, repo.clone(),backfill_parsers,notifier_service.clone());

    //     pipeline.run().await;
    //     tracing::info!("Backfill Complete");
    // }

    let source_clone = source.clone();
    let buffer_clone = buffer.clone();

    // Spawing Fetcher Task
    let _ = tokio::spawn(async move{
        tracing::info!("Starting Fetcher Task...");
        loop {
            // "Fetch" State
            let event = source_clone.lock().await.next_event().await;
            match event {
                Ok(Some(txn)) => {
                    // "Buffer" State
                    if let Err(_) = buffer_clone.produce(txn).await {
                        tracing::error!("Buffer closed, stopping fetcher");
                        break;
                    }
                },
                Ok(None) => {
                    tracing::info!("Source stream finished");
                    break;
                },
                Err(e) => {
                    tracing::error!("Error reading source: {:?}", e);
                    // Optional: Add a retry delay here
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            }
        }
    });

    // Ingestion Pipeline, it doesn't know it is reading from what
    let mut pipeline = IngestionPipeline::new(rx,repo,parsers,notifier_service);

    tracing::info!("Starting Ingestion Pipeline...");
    pipeline.run().await;

    Ok(())

}
