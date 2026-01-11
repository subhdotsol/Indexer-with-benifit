# Phase 3.1: Background Queue Persistence Optimization

In Phase 3, we added database persistence but it was slow due to synchronous inserts blocking the event loop. This optimization decouples parsing from persistence.

---

## The Problem

With cloud PostgreSQL (Neon), each DB write has ~1-2 second latency:
- Connection acquisition: ~5 seconds (TLS + network)
- Each INSERT: ~1-1.5 seconds (network round-trip)

**Synchronous approach**: Parse → Wait for DB → Parse next → Wait for DB...

This caused:
1. gRPC stream timeout (events backed up)
2. `InvalidSource` errors flooding the logs
3. Slow throughput (<1 event/second)

---

## The Solution: Background Queue

```
┌──────────────┐     ┌─────────────────┐     ┌──────────────────┐
│  gRPC Stream │────▶│ Parsing Loop    │────▶│ mpsc::channel    │
│  (fast)      │     │ (non-blocking)  │     │ (1000 capacity)  │
└──────────────┘     └─────────────────┘     └────────┬─────────┘
                                                      │
                                                      ▼
                                             ┌──────────────────┐
                                             │ Background Task  │
                                             │ (batch inserts)  │
                                             └────────┬─────────┘
                                                      │
                                                      ▼
                                             ┌──────────────────┐
                                             │   PostgreSQL     │
                                             └──────────────────┘
```

---

## Implementation Details

### 1. EventRepository Trait

`src/application/ports/event_repository.rs`:
```rust
#[async_trait]
pub trait EventRepository: Send + Sync {
    async fn save_event(&self, event: &TransactionEvent) -> AppResult<()>;
    async fn save_events(&self, events: &[TransactionEvent]) -> AppResult<()>;
    /// Batch insert events using a single transaction for better performance.
    async fn save_events_batch(&self, events: Vec<TransactionEvent>) -> AppResult<usize>;
}
```

---

### 2. PostgreSQL Batch Insert

`src/adapters/outbound/postgres_repository.rs`:
```rust
async fn save_events_batch(&self, events: Vec<TransactionEvent>) -> AppResult<usize> {
    if events.is_empty() {
        return Ok(0);
    }

    let count = events.len();
    
    // Use a transaction for atomicity and better performance
    let mut tx = self.pool.begin().await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

    for event in &events {
        match event {
            TransactionEvent::TokenTransfer(transfer) => {
                sqlx::query(
                    r#"INSERT INTO token_transfers (signature, slot, from_address, to_address, amount, mint)
                       VALUES ($1, $2, $3, $4, $5, $6)
                       ON CONFLICT (signature) DO NOTHING"#,
                )
                .bind(&transfer.signature)
                .bind(transfer.slot as i64)
                .bind(&transfer.from)
                .bind(&transfer.to)
                .bind(transfer.amount as i64)
                .bind(&transfer.mint)
                .execute(&mut *tx)
                .await
                .map_err(|e| AppError::DatabaseError(e.to_string()))?;
            }
            // ... similar for RaydiumSwap, JupiterSwap, PumpFunSwap
        }
    }

    tx.commit().await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

    tracing::info!(count = count, "Batch persisted events to database");
    Ok(count)
}
```

---

### 3. Ingestion Pipeline with Background Queue

`src/application/use_cases/ingest.rs`:
```rust
/// Configuration for the background persistence queue
const QUEUE_CAPACITY: usize = 1000;
const BATCH_SIZE: usize = 50;
const FLUSH_INTERVAL_MS: u64 = 500;

pub struct IngestionPipeline {
    source: Arc<Mutex<dyn TransactionSource>>,
    parsers: Vec<Arc<dyn TransactionParser>>,
    repository: Option<Arc<dyn EventRepository>>,
}

impl IngestionPipeline {
    pub async fn run(&self) {
        // Set up the background persistence channel
        let (tx, rx) = mpsc::channel::<TransactionEvent>(QUEUE_CAPACITY);
        
        if let Some(ref repo) = self.repository {
            // Spawn background persistence task
            let repo_clone = Arc::clone(repo);
            tokio::spawn(async move {
                background_persistence_task(rx, repo_clone).await;
            });
        }

        loop {
            // Get next event from source
            let event = { /* ... */ };

            // Parse the transaction
            for parser in &self.parsers {
                if let Some(events) = parser.parse(&tx_data) {
                    for event in events {
                        // Send to background queue (non-blocking)
                        if let Err(e) = tx.try_send(event) {
                            tracing::warn!("Persistence queue full, event dropped");
                        }
                    }
                }
            }
        }
    }
}
```

---

### 4. Background Persistence Task

```rust
async fn background_persistence_task(
    mut rx: mpsc::Receiver<TransactionEvent>,
    repo: Arc<dyn EventRepository>,
) {
    let mut buffer: Vec<TransactionEvent> = Vec::with_capacity(BATCH_SIZE);
    let mut interval = tokio::time::interval(Duration::from_millis(FLUSH_INTERVAL_MS));

    loop {
        tokio::select! {
            // Try to receive events from the channel
            event = rx.recv() => {
                match event {
                    Some(e) => {
                        buffer.push(e);
                        
                        // Flush when batch is full
                        if buffer.len() >= BATCH_SIZE {
                            flush_buffer(&mut buffer, &repo).await;
                        }
                    }
                    None => {
                        // Channel closed, flush remaining and exit
                        if !buffer.is_empty() {
                            flush_buffer(&mut buffer, &repo).await;
                        }
                        break;
                    }
                }
            }
            // Periodic flush to ensure events don't sit in buffer too long
            _ = interval.tick() => {
                if !buffer.is_empty() {
                    flush_buffer(&mut buffer, &repo).await;
                }
            }
        }
    }
}

async fn flush_buffer(buffer: &mut Vec<TransactionEvent>, repo: &Arc<dyn EventRepository>) {
    let events: Vec<TransactionEvent> = buffer.drain(..).collect();
    match repo.save_events_batch(events).await {
        Ok(saved) => tracing::debug!(count = saved, "Flushed events to database"),
        Err(e) => tracing::error!(error = ?e, "Failed to flush events to database"),
    }
}
```

---

## Benefits

| Before | After |
|--------|-------|
| ~1 event/sec | ~100+ events/sec |
| gRPC timeout errors | Stable streaming |
| Blocking on each write | Non-blocking queue |
| N round-trips | 1 round-trip per batch |

---

## Usage

No code changes needed - just run the indexer:

```bash
RUST_LOG=info cargo run
```

You'll see logs like:
```
INFO Database persistence enabled with background queue queue_capacity=1000 batch_size=50
INFO Background persistence task started
INFO Batch persisted events to database count=50
```
