mod domain;
mod application;
mod adapters;

use std::sync::Arc;
use tokio::sync::Mutex;
use crate::{adapters::FileSourceAdaptor, application::IngestionPipeline};

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

    dotenv::dotenv().ok();

    tracing_subscriber::fmt::init();

    // Default to File for now if env not set, for easier running without .env
    // But adhering to the reference which requires env
    let source_type = SourceType::from_env().unwrap_or(SourceType::File); 

    tracing::info!("Initializing Solana Indexer (Clean Arch)");

    // Dependency Injection
    let source = if source_type == SourceType::File {
        Arc::new(Mutex::new(FileSourceAdaptor::new(50_000)))
    }else {
        panic!("gRPC Source not implemented yet");
    };

    // Ingestion Pipeline, it doesn't know it is reading from what
    let pipeline = IngestionPipeline::new(source);

    tracing::info!("Starting Ingestion Pipeline...");
    pipeline.run().await;

    Ok(())

}
