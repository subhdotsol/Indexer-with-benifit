# Solana Indexer

> **Production-Grade Data Ingestion for Solana**

A high-performance Solana blockchain indexer written in Rust. Ingests transactions from multiple sources, parses them into protocol-specific events, and persists them to PostgreSQL — with zero-loss recovery and real-time Telegram alerts.

[![Solana](https://img.shields.io/badge/Solana-9945FF?style=flat&logo=solana&logoColor=white)](https://solana.com/)
[![Rust](https://img.shields.io/badge/Rust-000000?style=flat&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](./LICENSE)

## Features

- **3 Ingestion Sources** — Yellowstone gRPC (live), RPC backfill (historical), file replay (debug)
- **4 Protocol Parsers** — Jupiter, Raydium AMM, Pump.fun, SPL Token
- **Zero-Loss Recovery** — slot cursor in `indexer_state` + gap backfill + Dead Letter Queue
- **Batch Persistence** — PostgreSQL via `sqlx` with `UNNEST` batch writes
- **Whale Alerts** — Telegram bot notifications for high-value swaps
- **Hexagonal Architecture** — swap any source, parser, or sink independently

## Architecture

<img width="914" height="1101" alt="Screenshot 2026-07-16 at 12 42 22 AM" src="https://github.com/user-attachments/assets/68541455-20fd-4d90-a626-05aa458ed942" />

## Quick Start

### Prerequisites
- **Rust** 1.75+
- **Docker & Docker Compose**
- **Solana RPC/gRPC endpoint** (e.g., Helius, QuickNode)

### Install

```bash
git clone https://github.com/subhdotsol/Indexer-with-benifit.git
cd Indexer-with-benifit
```

### Configure

```env
SOURCE_TYPE=grpc          # or 'file'
RUST_LOG=info

GRPC_URL=http://127.0.0.1:10000
GRPC_TOKEN=                        # optional, provider auth token
DATABASE_URL=postgres://postgres:postgres@localhost:5432/solana_indexer
RPC_URL=https://api.mainnet-beta.solana.com

# Optional — Telegram whale alerts
TELEGRAM_BOT_TOKEN=your_token
TELEGRAM_CHAT_ID=your_chat_id
```

### Run

```bash
# Start PostgreSQL
docker-compose up -d

# Run migrations
cargo sqlx migrate run

# Start indexer
cargo run --release
```

## Project Structure

```
.
├── Cargo.toml
├── docker-compose.yml
├── migrations/               # SQLx database migrations
└── src/
    ├── main.rs               # Entry point & wiring
    ├── lib.rs
    ├── domain/
    │   └── models.rs         # ChainEvent, TransactionEvent, SwapEvent
    ├── application/
    │   ├── ports/            # Traits: TransactionSource, TransactionParser, TransactionRepository
    │   └── use_cases/
    │       └── ingest.rs     # IngestionPipeline — batch loop & DLQ
    ├── adapters/
    │   ├── inbound/
    │   │   ├── grpc_source.rs
    │   │   ├── file_source.rs
    │   │   └── rpc_source.rs
    │   ├── outbound/
    │   │   ├── postgres_repository.rs
    │   │   └── telegram.rs
    │   └── parsers/
    │       ├── jupiter.rs
    │       ├── raydium_amm.rs
    │       ├── pump_fun.rs
    │       ├── spl_token.rs
    │       └── vixen_utils.rs
    └── infrastructure/
        └── buffer/           # MemoryBuffer (tokio mpsc)
```

## Roadmap

### Completed

- [x] Hexagonal architecture — pluggable sources, parsers, sinks
- [x] Yellowstone gRPC ingestion (raw, one layer below Vixen)
- [x] RPC backfill + file replay
- [x] Jupiter, Raydium AMM, Pump.fun, SPL Token parsers
- [x] PostgreSQL persistence with UNNEST batch writes
- [x] Slot cursor + DLQ for zero-loss recovery
- [x] Telegram whale alerts

## License

MIT — see [LICENSE](./LICENSE)
