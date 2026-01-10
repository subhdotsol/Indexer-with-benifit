# Phase 1: Architecture & Codebase Explained

This document explains the "Clean Architecture" we built in Phase 1 and how the Rust files interact.

---

## 0. The Intuition: How Would a Beginner Approach This?

Imagine you're told: **"Build a Solana Indexer."** You know nothing. Here's the thought process:

### Step 1: What's the GOAL?
> "I need to **read** transactions from Solana and **do something** with them (store, alert, etc.)."

So, at its core, it's a **pipeline**: `Input → Process → Output`.

### Step 2: What's the SIMPLEST version?
Forget gRPC, forget databases. Start dumb:
```rust
fn main() {
    let fake_transaction = "sig_1";
    println!("Processing: {}", fake_transaction);
}
```
This is useless, but it **runs**. Now you have a starting point.

### Step 3: Make it loop
Real indexers process many transactions. Let's loop:
```rust
fn main() {
    for i in 1..=10 {
        let sig = format!("sig_{}", i);
        println!("Processing: {}", sig);
    }
}
```
Now we process 10 fake items. Progress!

### Step 4: Define what a "Transaction" IS
We're printing strings. But a real transaction has `signature`, `slot`, `success`, etc. Let's be formal. This becomes **`src/domain/models.rs`**:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug,Clone,Serialize,Deserialize)]
pub struct SolanaTransaction{
    pub signature:String,
    pub success:bool,
    pub raw_bytes: Vec<u8>, // will parse it later
    // It is for ordering & finality is also expressed in terms of slot
    pub slot: u64,
}
```

### Step 5: Where does data COME FROM?
Right now, we hardcode it. But later, it'll come from:
- A file (for testing)
- gRPC (for production)

We don't want to rewrite everything when we switch. So we create a **contract** (Trait). This becomes **`src/application/ports/input.rs`**:

```rust
use async_trait::async_trait;
use crate::{application::AppResult, domain::SolanaTransaction};

#[async_trait]
pub trait TransactionSource: Send + Sync{
    async fn next_transaction(&mut self)->AppResult<Option<SolanaTransaction>>;
}
```
It says: "Anyone who wants to be a source MUST have a `next_transaction` function."

### Step 6: Implement the contract for FILES
This becomes **`src/adapters/inbound/file_source.rs`**:

```rust
use std::time::Duration;
use async_trait::async_trait;
use tokio::{time::sleep};
use crate::{application::{AppResult, TransactionSource}, domain::SolanaTransaction};

pub struct FileSourceAdaptor{
    // We will have BufferReader<File instead>
    current_count:u64,
    max_count:u64
}

impl FileSourceAdaptor{
    pub fn new(max_count:u64)->Self{
        Self { 
            current_count :0,
            max_count
        }
    }
}

#[async_trait]
impl TransactionSource for FileSourceAdaptor{
    async fn next_transaction(&mut self)->AppResult<Option<SolanaTransaction>>{
        if self.current_count >= self.max_count {
            return Ok(None);
        }

        // Simulating DISK latency
        sleep(Duration::from_micros(10)).await;

        self.current_count += 1;

        Ok(Some(
            SolanaTransaction{
                success:true,
                slot: 1000 + self.current_count,
                raw_bytes: vec![],
                signature: format!("sig_{}",self.current_count),
            }
        ))
    }
}
```

### Step 7: Build the Pipeline Logic
This becomes **`src/application/use_cases/ingest.rs`**:

```rust
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
```

### Step 8: Wire it up in `main.rs`
This is **`src/main.rs`**:

```rust
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
```

---

## 1. The Directory Structure

```
src/
├── main.rs                 # Entry point (The "Glue")
├── domain/                 # Core Data Types
│   └── models.rs           # SolanaTransaction
├── application/            # Business Logic
│   ├── ports/              # Interfaces (Traits)
│   │   └── input.rs        # TransactionSource trait
│   └── use_cases/          
│       └── ingest.rs       # IngestionPipeline
└── adapters/               
    └── inbound/            
        └── file_source.rs  # FileSourceAdaptor
```

---

## 2. How Imports Work (`mod` vs `use`)
*   **`mod folder_name;`**: "Include this module." Done once per module.
*   **`use crate::path::Item;`**: "Bring this Item into scope."
*   **`pub mod` / `pub use`**: Makes things available to other modules.

---

## Summary
1.  `main.rs` creates `FileSourceAdaptor`.
2.  `main.rs` gives it to `IngestionPipeline`.
3.  `IngestionPipeline` calls `next_transaction()`.
4.  `FileSourceAdaptor` returns simulated data.
5.  `domain` defines what that data looks like (`SolanaTransaction`).
