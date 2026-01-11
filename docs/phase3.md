# Phase 3: PostgreSQL Persistence

In Phase 3, we added database persistence to store parsed events permanently.

---

## 1. Prerequisites

- PostgreSQL database (we used [Neon](https://neon.tech) - free serverless Postgres)
- SQLx CLI with TLS support

### Install SQLx CLI
```bash
cargo install sqlx-cli --features native-tls
```

---

## 2. Dependencies

Add to `Cargo.toml`:
```toml
sqlx = { version = "0.8.6", features = ["postgres", "runtime-tokio", "chrono", "uuid", "tls-native-tls"] }
uuid = { version = "1.19.0", features = ["v4"] }
```

> **Note**: `tls-native-tls` is required for cloud databases like Neon that enforce SSL.

---

## 3. Database Schema

Create `migrations/001_init.sql`:
```sql
-- Token Transfers
CREATE TABLE IF NOT EXISTS token_transfers (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    signature TEXT NOT NULL UNIQUE,
    slot BIGINT NOT NULL,
    from_address TEXT NOT NULL,
    to_address TEXT NOT NULL,
    amount BIGINT NOT NULL,
    mint TEXT,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Raydium Swaps
CREATE TABLE IF NOT EXISTS raydium_swaps (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    signature TEXT NOT NULL UNIQUE,
    slot BIGINT NOT NULL,
    amm_pool TEXT NOT NULL,
    signer TEXT NOT NULL,
    amount_in BIGINT NOT NULL,
    min_amount_out BIGINT NOT NULL,
    amount_received BIGINT NOT NULL,
    mint_source TEXT NOT NULL,
    mint_destination TEXT NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Jupiter Swaps
CREATE TABLE IF NOT EXISTS jupiter_swaps (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    signature TEXT NOT NULL UNIQUE,
    slot BIGINT NOT NULL,
    signer TEXT NOT NULL,
    amm_pool TEXT NOT NULL,
    mint_in TEXT NOT NULL,
    mint_out TEXT NOT NULL,
    amount_in BIGINT NOT NULL,
    amount_out BIGINT NOT NULL,
    slippage_bps SMALLINT NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- PumpFun Swaps
CREATE TABLE IF NOT EXISTS pumpfun_swaps (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    signature TEXT NOT NULL UNIQUE,
    slot BIGINT NOT NULL,
    signer TEXT NOT NULL,
    mint TEXT NOT NULL,
    is_buy BOOLEAN NOT NULL,
    sol_amount BIGINT NOT NULL,
    token_amount BIGINT NOT NULL,
    bonding_curve TEXT NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Indexes for common queries
CREATE INDEX idx_token_transfers_slot ON token_transfers(slot);
CREATE INDEX idx_raydium_swaps_slot ON raydium_swaps(slot);
CREATE INDEX idx_jupiter_swaps_slot ON jupiter_swaps(slot);
CREATE INDEX idx_pumpfun_swaps_slot ON pumpfun_swaps(slot);
```

### Run Migrations
```bash
# Set DATABASE_URL in .env first
sqlx migrate run
```

---

## 4. Repository Pattern

### 4.1 Define the Port (Trait)

`src/application/ports/event_repository.rs`:
```rust
use crate::application::AppResult;
use crate::domain::TransactionEvent;
use async_trait::async_trait;

#[async_trait]
pub trait EventRepository: Send + Sync {
    async fn save_event(&self, event: &TransactionEvent) -> AppResult<()>;
    async fn save_events(&self, events: &[TransactionEvent]) -> AppResult<()>;
}
```

Export in `src/application/ports/mod.rs`:
```rust
pub mod event_repository;
pub use event_repository::*;
```

### 4.2 Update AppError

`src/application/error.rs`:
```rust
#[derive(Debug)]
pub enum AppError {
    InvalidSource,
    ParseError(String),
    DatabaseError(String),
}

pub type AppResult<T> = Result<T, AppError>;
```

---

## 5. PostgreSQL Adapter

`src/adapters/outbound/postgres_repository.rs`:
```rust
use crate::{
    application::{AppError, AppResult, EventRepository},
    domain::{JupiterSwapEvent, PumpFunSwapEvent, RaydiumSwapEvent, TokenTransfer, TransactionEvent},
};
use async_trait::async_trait;
use sqlx::{postgres::PgPoolOptions, PgPool};

pub struct PostgresRepository {
    pool: PgPool,
}

impl PostgresRepository {
    pub async fn new(database_url: &str) -> Result<Self, sqlx::Error> {
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(database_url)
            .await?;
        Ok(Self { pool })
    }

    async fn save_token_transfer(&self, transfer: &TokenTransfer) -> AppResult<()> {
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
        .execute(&self.pool)
        .await
        .map_err(|e: sqlx::Error| AppError::DatabaseError(e.to_string()))?;
        Ok(())
    }

    // Similar methods for save_raydium_swap, save_jupiter_swap, save_pumpfun_swap...
}

#[async_trait]
impl EventRepository for PostgresRepository {
    async fn save_event(&self, event: &TransactionEvent) -> AppResult<()> {
        match event {
            TransactionEvent::TokenTransfer(t) => self.save_token_transfer(t).await,
            TransactionEvent::RaydiumSwap(s) => self.save_raydium_swap(s).await,
            TransactionEvent::JupiterSwap(s) => self.save_jupiter_swap(s).await,
            TransactionEvent::PumpFunSwap(s) => self.save_pumpfun_swap(s).await,
        }
    }

    async fn save_events(&self, events: &[TransactionEvent]) -> AppResult<()> {
        for event in events {
            self.save_event(event).await?;
        }
        Ok(())
    }
}
```

Export in `src/adapters/outbound/mod.rs`:
```rust
mod postgres_repository;
pub use postgres_repository::*;
```

---

## 6. Update IngestionPipeline

`src/application/use_cases/ingest.rs`:
```rust
pub struct IngestionPipeline {
    source: Arc<Mutex<dyn TransactionSource>>,
    parsers: Vec<Arc<dyn TransactionParser>>,
    repository: Option<Arc<dyn EventRepository>>,  // NEW
}

impl IngestionPipeline {
    pub fn new(source, parsers) -> Self {
        Self { source, parsers, repository: None }
    }

    pub fn with_repository(mut self, repo: Arc<dyn EventRepository>) -> Self {
        self.repository = Some(repo);
        self
    }

    pub async fn run(&self) {
        // In consumer loop, after parsing:
        if let Some(ref repo) = repo_clone {
            if let Err(e) = repo.save_event(event).await {
                tracing::error!("Failed to persist: {:?}", e);
            }
        }
    }
}
```

---

## 7. Wire in main.rs

```rust
use crate::adapters::PostgresRepository;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ... source and parsers setup ...

    // Connect to database
    let repository = if let Ok(database_url) = std::env::var("DATABASE_URL") {
        tracing::info!("Connecting to PostgreSQL...");
        match PostgresRepository::new(&database_url).await {
            Ok(repo) => Some(Arc::new(repo)),
            Err(e) => {
                tracing::warn!("Failed to connect: {}. Running without persistence.", e);
                None
            }
        }
    } else {
        None
    };

    // Build pipeline with optional repository
    let mut pipeline = IngestionPipeline::new(source, parsers);
    if let Some(repo) = repository {
        pipeline = pipeline.with_repository(repo);
    }

    pipeline.run().await;
    Ok(())
}
```

---

## 8. Environment Configuration

`.env`:
```env
DATABASE_URL=postgresql://user:pass@host/dbname?sslmode=require
```

---

## Summary

| Component | File | Purpose |
|-----------|------|---------|
| Schema | `migrations/001_init.sql` | Database tables |
| Port | `application/ports/event_repository.rs` | Repository trait |
| Adapter | `adapters/outbound/postgres_repository.rs` | PostgreSQL impl |
| Pipeline | `application/use_cases/ingest.rs` | Accepts repository |
| Entry | `main.rs` | Wires everything |

**Result**: Parsed events now persist to PostgreSQL automatically! 🎉
