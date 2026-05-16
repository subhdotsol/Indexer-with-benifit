use anyhow::Result;
use async_trait::async_trait;
use crate::domain::{IndexerState, SolanaTransaction, TransactionEvent};

#[async_trait]
pub trait TransactionRepository: Send + Sync {
    async fn get_state(&self) -> Result<IndexerState>;
    async fn get_last_slot(&self) -> Result<u64>;
    async fn save_batch(&self, events: &[TransactionEvent], current_slot: u64) -> Result<()>;
    async fn save_dlq(&self, txn: &SolanaTransaction, parser_name: &str, error: &str) -> Result<()>;
}
