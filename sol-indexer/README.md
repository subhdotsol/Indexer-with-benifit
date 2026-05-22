# My Solana Indexer

> **High-Performance Data Ingestion for Solana**

A production-grade, high-performance Solana blockchain indexer written in Rust. This application is designed to ingest transactions from various sources (gRPC, RPC, File), parse them into protocol-specific events, and persist them into a PostgreSQL database for analytics and monitoring.

[![Solana](https://img.shields.io/badge/Solana-9945FF?style=flat&logo=solana&logoColor=white)](https://solana.com/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](./LICENSE)

##  Demo Video

▶ **End-to-end live indexing demo (gRPC → Parse → Postgres → Telegram alerts)**  
- Live Yellowstone gRPC ingestion  
- Jupiter / Raydium / Pump.fun parsing  
- DLQ handling for malformed events  
- Real-time Telegram whale alerts

https://github.com/user-attachments/assets/d96be5f9-8e6a-4bec-b6cb-fa1569f32b4e

## Key Features

- **Multi-Source Ingestion:**
  - **gRPC (Yellowstone Geyser):** High-throughput, low-latency streaming directly from Solana validators.
  - **File Source:** Replay transactions from local files for testing and debugging.
  - _RPC (Backfill):_ (Partial) Support for fetching historical blocks via standard RPC.
- **Protocol-Specific Parsing:**
  - **Jupiter:** Aggregator swap parsing.
  - **Raydium:** AMM swap parsing.
  - **Pump.fun:** Bonding curve trade parsing.
  - **SPL Token:** Standard token transfer parsing.
- **Real-time Notifications:** Integrated Telegram bot to alert on high-value transactions (whales).
- **Robust Architecture:**
  - **Async/Await:** Built on `tokio` for efficient non-blocking I/O.
  - **Buffering:** In-memory event buffering to handle bursty traffic.
  - **Dead Letter Queue (DLQ):** Captures failed events for later analysis.
- **Data Persistence:** Uses PostgreSQL with `sqlx` for type-safe database interactions.

## How It Works

### 1. **Ingest (Sources)**

- **gRPC Stream**: Connects to Yellowstone Geyser for live blocks.
- **File Replay**: Reprocesses historical block data from local files for testing.
- **RPC Backfill**: (Experimental) Fetches missing historical gaps via standard RPC.

### 2. **Parse (Transformation)**

- Decodes raw transaction data into protocol-specific domain events.
- Filters out failed transactions and non-relevant instructions.

### 3. **Buffer (Management)**

- **Memory Buffer**: Decouples ingestion from processing to handle backpressure.
- **DLQ**: Captures malformed or failed events for debugging without stopping the pipeline.

### 4. **Persist & Notify (Outputs)**

- **PostgreSQL**: Stores structured, queryable data (Swaps, Trades, Transfers).
- **Telegram**: Sends real-time alerts for high-value transactions (> 1 SOL).

## Architecture

### System Flow
<img width="2816" height="1536" alt="Indexer Architecture" src="https://github.com/user-attachments/assets/d0c854ce-6dcd-40af-869f-da4aaeccee9e" />

## Demo Screenshots
<img width="902" height="949" alt="image" src="https://github.com/user-attachments/assets/35f0fe26-f392-4e99-ae04-3b5fd9cabfb4" />
<img width="1699" height="706" alt="image" src="https://github.com/user-attachments/assets/e1fd722e-ce75-408c-925c-82593a159870" />
<img width="1699" height="706" alt="image" src="https://github.com/user-attachments/assets/41c42987-5519-4c30-9754-5ab6861495b2" />


## Project Structure

```
my-solana-indexer/
├── src/
│   ├── main.rs                 # Application entry point
│   ├── domain/                 # Core types & models (ChainEvent)
│   ├── application/            # Business logic & pipeline
│   ├── adapters/               # External interfaces
│   │   ├── inbound/            # gRPC & File sources
│   │   ├── outbound/           # Postgres & Telegram adapters
│   │   └── parsers/            # Protocol-specific logic
│   └── infrastructure/         # Low-level buffers & utils
├── migrations/                 # SQL database migrations
├── scripts/                    # TypeScript testing utilities
├── idls/                       # Protocol IDLs (Jupiter, Pump.fun)
└── docker-compose.yml          # Infrastructure setup
```

## Setup & Installation

### Prerequisites

- **Rust** 1.75+ (Edition 2024)
- **Docker & Docker Compose**
- **Solana RPC/gRPC URL** (e.g., Helius, QuickNode)

### Quick Start

1. **Clone the repository**

   ```bash
   git clone https://github.com/your-username/my-solana-indexer.git
   cd my-solana-indexer
   ```

2. **Configure Environment**
   Create a `.env` file:

   ```env
   # Source: 'grpc' or 'file'
   SOURCE_TYPE=grpc

   # Connectivity
   RPC_URL=https://api.mainnet-beta.solana.com
   GRPC_URL=http://127.0.0.1:10000
   DATABASE_URL=postgres://postgres:postgres@localhost:5432/solana_indexer

   # Optional: Telegram Alerts
   TELEGRAM_BOT_TOKEN=your_token
   TELEGRAM_CHAT_ID=your_chat_id
   ```

3. **Start Database**

   ```bash
   docker-compose up -d
   ```

4. **Run Migrations**

   ```bash
   cargo install sqlx-cli
   sqlx migrate run
   ```

5. **Launch Indexer**
   ```bash
   cargo run --release
   ```

## Supported Protocols

The indexer is currently equipped to parse and store data for:

| Protocol      | Type          | Key Data Points            |
| ------------- | ------------- | -------------------------- |
| **Jupiter**   | Aggregator    | Swaps, Routes, ExactOut/In |
| **Raydium**   | AMM           | Swaps, Liquidity Changes   |
| **Pump.fun**  | Bonding Curve | Buys, Sells, Curve State   |
| **SPL Token** | Standard      | Transfers, Mints, Burns    |

## License

This project is licensed under the [MIT License](./LICENSE).
