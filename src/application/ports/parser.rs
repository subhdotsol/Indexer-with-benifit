use anyhow::Result;
use crate::domain::{SolanaTransaction, TransactionEvent};

pub trait TransactionParser: Send + Sync {
    fn parse(&self, txn: SolanaTransaction) -> Result<Option<Vec<TransactionEvent>>>;
    fn name(&self) -> &str;
}
