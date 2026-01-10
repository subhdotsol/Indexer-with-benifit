use async_trait::async_trait;
use crate::{application::AppResult, domain::SolanaTransaction};

#[async_trait]
pub trait TransactionSource: Send + Sync{
    async fn next_transaction(&mut self)->AppResult<Option<SolanaTransaction>>;
}
