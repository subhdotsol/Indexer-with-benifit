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
We're printing strings. But a real transaction has `signature`, `slot`, `success`, etc. Let's be formal:
```rust
struct SolanaTransaction {
    signature: String,
    slot: u64,
}
```
This is our **`domain/models.rs`**. We just defined the "shape" of data.

### Step 5: Where does data COME FROM?
Right now, we hardcode it. But later, it'll come from:
- A file (for testing)
- gRPC (for production)

We don't want to rewrite everything when we switch. So we create a **contract**:
```rust
trait TransactionSource {
    fn next_transaction() -> Option<SolanaTransaction>;
}
```
This says: "I don't care HOW you get me data. Just give me `next_transaction()`."

This is our **`application/ports/input.rs`**.

### Step 6: Implement the contract for FILES
```rust
struct FileSourceAdaptor { /* ... */ }

impl TransactionSource for FileSourceAdaptor {
    fn next_transaction() -> Option<SolanaTransaction> {
        // ... read from file ...
    }
}
```
This is our **`adapters/inbound/file_source.rs`**.

### Step 7: Wire it up
In `main.rs`, we:
1. Create the source: `let source = FileSourceAdaptor::new();`
2. Create the pipeline: `let pipeline = IngestionPipeline::new(source);`
3. Run: `pipeline.run();`

The pipeline doesn't know it's using a file. It just knows it has a `TransactionSource`.

### The Aha! Moment
The magic is **separation**:
- **Domain**: Defines WHAT data looks like. Changes rarely.
- **Application**: Defines the LOGIC. "Get data, process it." Doesn't know specifics.
- **Adapters**: Defines HOW to get/store data. Easy to swap.

---

## 1. The Directory Structure (Clean Architecture)
We organized the code into three main layers. The rule is: **Dependencies point INWARDS.**
- **Inner Layer (`domain`)**: Knows nothing about the outside world.
- **Middle Layer (`application`)**: Orchestrates logic using Interfaces/Traits.
- **Outer Layer (`adapters`)**: Connects to the real world (Files, gRPC, Postgres).

```
src/
├── main.rs                 # Entry point (The "Glue")
├── domain/                 # Core Data Types (The "King")
│   └── models.rs           # e.g., SolanaTransaction
├── application/            # Business Logic (The "Brain")
│   ├── ports/              # Interfaces (Traits) - "I need X"
│   └── use_cases/          # Logic - "How to process flow"
└── adapters/               # Implementations (The "Hands")
    └── inbound/            # Sources of data (File, gRPC)
```

---

## 2. Walkthrough: `src/main.rs` (The Entry Point)
This file "bootstraps" the application. It connects the "Hands" (Adapters) to the "Brain" (Application).

```rust
// 1. Module Declarations: This tells Rust "Go find these folders/files"
mod domain;
mod application;
mod adapters;

use std::sync::Arc;
use tokio::sync::Mutex;
use crate::{adapters::FileSourceAdaptor, application::IngestionPipeline};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 2. Create the Adapter (The "Hand")
    let source = Arc::new(Mutex::new(FileSourceAdaptor::new(50_000)));

    // 3. Create the Application Pipeline (The "Brain")
    // Dependency Injection: giving the 'source' to the pipeline.
    let pipeline = IngestionPipeline::new(source);

    // 4. Run it
    pipeline.run().await;
    Ok(())
}
```

---

## 3. How Imports Work (`mod` vs `use`)
*   **`mod folder_name;`**: "Hey Rust, include this module." Done once per module.
*   **`use crate::path::Item;`**: "Bring this Item into scope for convenience."
*   **`pub mod` / `pub use`**: Makes things available to other modules.

---

## 4. Key Files in Phase 1

| File | Role |
|------|------|
| `domain/models.rs` | Defines `SolanaTransaction` struct. Pure data. |
| `application/ports/input.rs` | Defines `TransactionSource` trait (the contract). |
| `adapters/inbound/file_source.rs` | Implements `TransactionSource` for files. |
| `application/use_cases/ingest.rs` | The `IngestionPipeline` that runs the loop. |

---

## Summary
1.  `main.rs` creates `FileSourceAdaptor`.
2.  `main.rs` gives it to `IngestionPipeline`.
3.  `IngestionPipeline` calls `next_transaction()`.
4.  `FileSourceAdaptor` returns simulated data.
5.  `domain` defines what that data looks like (`SolanaTransaction`).
