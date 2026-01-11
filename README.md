# Solana Indexer

> **High-Performance Data Ingestion for Solana**

A production-grade, high-performance Solana blockchain indexer written in Rust. Designed to ingest transactions from various sources (gRPC, RPC, File), parse them into protocol-specific events, and persist them for analytics and monitoring.

[![Solana](https://img.shields.io/badge/Solana-9945FF?style=flat&logo=solana&logoColor=white)](https://solana.com/)
[![Rust](https://img.shields.io/badge/Rust-000000?style=flat&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](./LICENSE)

## Features

- **Multi-Source Ingestion**
  - **gRPC (Yellowstone Geyser)**: High-throughput, low-latency streaming directly from validators.
  - **File Source**: Replay transactions from local files for testing and debugging.
  - **RPC Backfill**: Fetch historical blocks via standard RPC.

- **Protocol-Specific Parsing**
  - **Jupiter**: Aggregator swap parsing.
  - **Raydium**: AMM swap parsing.
  - **Pump.fun**: Bonding curve trade parsing.
  - **SPL Token**: Standard token transfer parsing.

- **Real-time Notifications**: Telegram bot alerts for high-value transactions (whale alerts).

- **Robust Architecture**
  - Built on `tokio` for efficient async I/O.
  - In-memory buffering for bursty traffic.
  - Dead Letter Queue (DLQ) for failed events.

- **Data Persistence**: PostgreSQL with `sqlx` for type-safe database interactions.

- **GraphQL API** *(Coming Soon)*: Query indexed data via a flexible API.

- **Web Dashboard** *(Coming Soon)*: Visualize indexed stats in real-time.

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                         SOURCES                              │
│   ┌─────────┐    ┌─────────┐    ┌─────────┐                 │
│   │  gRPC   │    │  File   │    │   RPC   │                 │
│   └────┬────┘    └────┬────┘    └────┬────┘                 │
│        │              │              │                       │
│        └──────────────┼──────────────┘                       │
│                       ▼                                      │
│              ┌────────────────┐                              │
│              │ Ingestion Pipe │                              │
│              └───────┬────────┘                              │
│                      ▼                                       │
│             ┌─────────────────┐                              │
│             │     Parsers     │                              │
│             │ (Jupiter, etc.) │                              │
│             └────────┬────────┘                              │
│                      ▼                                       │
│   ┌─────────────┐         ┌─────────────┐                    │
│   │  PostgreSQL │         │  Telegram   │                    │
│   └─────────────┘         └─────────────┘                    │
└─────────────────────────────────────────────────────────────┘
```

## Quick Start

### Prerequisites
- **Rust** 1.75+
- **Docker & Docker Compose**
- **Solana RPC/gRPC URL** (e.g., Helius, QuickNode)

### Installation

```bash
git clone https://github.com/subhdotsol/Indexer-with-benifit.git
cd Indexer-with-benifit
```

### Configuration
Create a `.env` file:
```env
SOURCE_TYPE=file   # or 'grpc'
RUST_LOG=info

# For gRPC mode
GRPC_URL=http://127.0.0.1:10000
DATABASE_URL=postgres://postgres:postgres@localhost:5432/solana_indexer

# Optional: Telegram Alerts
TELEGRAM_BOT_TOKEN=your_token
TELEGRAM_CHAT_ID=your_chat_id
```

### Run
```bash
cargo run --release
```

## Project Structure

```
.
├── Cargo.toml              # Dependencies & project config
├── migrations/             # SQLx database migrations
├── docs/                   # Phase documentation
└── src/
    ├── main.rs             # Entry point
    ├── lib.rs              # Library exports
    ├── domain/             # Core types & models
    │   └── models.rs       # TransactionEvent, Swap types, etc.
    ├── application/        # Business logic & pipeline
    │   ├── ports/          # Interface definitions (Traits)
    │   │   ├── transaction_source.rs
    │   │   ├── transaction_parser.rs
    │   │   └── event_repository.rs
    │   └── use_cases/      # Ingestion pipeline
    │       └── ingest.rs   # Background queue & batch processing
    ├── adapters/           # External interfaces
    │   ├── inbound/        # Data sources
    │   │   ├── grpc_source.rs   # Yellowstone gRPC
    │   │   └── file_source.rs   # File replay
    │   ├── outbound/       # Data sinks
    │   │   └── postgres_repository.rs
    │   └── parsers/        # Protocol parsers
    │       ├── jupiter.rs
    │       ├── raydium_amm.rs
    │       ├── pump_fun.rs
    │       └── spl_token.rs
    └── infrastructure/     # Cross-cutting concerns
```

## Roadmap

### ✅ Completed

- [x] **Phase 1: Foundation & Ingestion Pipeline**
  - Hexagonal architecture setup
  - Multi-source ingestion (gRPC, File, RPC)
  - Domain models and core types

- [x] **Phase 2: Protocol Parsers**
  - SPL Token transfer parsing
  - Raydium AMM swap parsing
  - Jupiter aggregator swap parsing
  - Pump.fun bonding curve trade parsing

- [x] **Phase 3: PostgreSQL Persistence**
  - Database schema with SQLx migrations
  - Repository pattern implementation
  - Background queue optimization (batch inserts)

---

### 🚧 In Progress / Planned

- [ ] **Phase 4: Query API & Dashboards**
  - HTTP API endpoints (axum/actix-web)
  - Endpoints: `/transfers`, `/swaps`, `/stats`
  - Pagination and filtering by slot, signer, mint

- [ ] **Phase 5: Real-time WebSocket Streaming**
  - Push new events to connected clients
  - Real-time dashboards support
  - Event subscription by type

- [ ] **Phase 6: Telegram Bot Notifications**
  - Whale alert notifications for high-value swaps
  - Configurable thresholds and filters
  - Dead Letter Queue (DLQ) for failed notifications

- [ ] **Phase 7: Metrics & Observability**
  - Prometheus metrics (events/sec, queue depth, DB latency)
  - OpenTelemetry tracing for distributed requests
  - Health check endpoints

- [ ] **Phase 8: Deployment & Infrastructure**
  - Dockerize the indexer
  - Kubernetes manifests / Docker Compose
  - CI/CD pipeline for automated builds/tests

- [ ] **Phase 9: Extended Parsing**
  - Additional DEX parsers (Orca, Meteora)
  - NFT marketplace events
  - Staking/governance events

## License

This project is licensed under the MIT License - see the [LICENSE](./LICENSE) file for details.
