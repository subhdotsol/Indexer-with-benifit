use async_trait::async_trait;
use crate::{application::AppResult, domain::ChainEvent};

#[async_trait]
pub trait TransactionSource: Send {
    async fn next_event(&mut self)->AppResult<Option<ChainEvent>>;
}
