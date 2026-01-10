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
src/
├── main.rs                 # Entry point
├── domain/                 # Core types & models
├── application/            # Business logic & pipeline
│   ├── ports/              # Interface definitions (Traits)
│   └── use_cases/          # Ingestion logic
└── adapters/               # External interfaces
    └── inbound/            # gRPC & File sources
```

## Roadmap

- [x] Phase 1: Foundation & Ingestion Pipeline
- [ ] Phase 2: Protocol Parsers (SPL, Raydium, Jupiter, Pump.fun)
- [ ] Phase 3: PostgreSQL Persistence
- [ ] Phase 4: Telegram Notifications & DLQ
- [ ] Phase 5: GraphQL API & Web Dashboard

## License

MIT
