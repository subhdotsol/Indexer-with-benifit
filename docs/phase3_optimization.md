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

### Configuration Constants

```rust
const QUEUE_CAPACITY: usize = 1000;  // Buffer for bursts
const BATCH_SIZE: usize = 50;         // Events per batch insert
const FLUSH_INTERVAL_MS: u64 = 500;   // Max wait before flush
```

### Key Components

| Component | Description |
|-----------|-------------|
| `mpsc::channel` | Bounded queue connecting parser to persistence task |
| `try_send()` | Non-blocking send, drops events if queue full |
| Background task | Receives events, batches them, flushes to DB |
| `save_events_batch()` | Uses single transaction for all events |

### Files Modified

| File | Changes |
|------|---------|
| `application/ports/event_repository.rs` | Added `save_events_batch()` to trait |
| `adapters/outbound/postgres_repository.rs` | Implemented batch insert with transaction |
| `application/use_cases/ingest.rs` | Background queue + batch persistence |

---

## How It Works

1. **Pipeline starts**: Creates bounded mpsc channel (capacity 1000)
2. **Background task spawns**: Listens on channel, maintains event buffer
3. **Parsing loop runs**: Sends events to channel via `try_send()` (non-blocking)
4. **Background task flushes** when either:
   - Buffer reaches 50 events
   - 500ms have passed since last flush
5. **Batch insert**: All events in single PostgreSQL transaction

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
