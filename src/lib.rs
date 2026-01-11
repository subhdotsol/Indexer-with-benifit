//! Solana Indexer Library
//!
//! A modular Solana blockchain indexer following hexagonal architecture:
//!
//! - `domain`: Core business models (transactions, events, swaps)
//! - `application`: Use cases, ports (traits), and error types
//! - `adapters`: Implementations (gRPC source, parsers, PostgreSQL repository)
//! - `infrastructure`: Cross-cutting concerns (logging, telemetry)

pub mod adapters;
pub mod application;
pub mod domain;
pub mod infrastructure;
