use async_trait::async_trait;
use crate::domain::{SolanaTransaction, TransactionEvent};

#[async_trait]
pub trait TransactionParser: Send + Sync {
    fn parse(&self, txn: &SolanaTransaction) -> Option<Vec<TransactionEvent>>;
    fn name(&self) -> &str;
}
