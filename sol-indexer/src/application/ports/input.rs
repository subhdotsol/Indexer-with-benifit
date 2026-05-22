use async_trait::async_trait;

use crate::{application::AppResult, domain::{ChainEvent}};

#[async_trait]
// Data should be streamed from here we can't just send the string from here
pub trait TransactionSource: Send + Sync{
    async fn next_event(&mut self)->AppResult<Option<ChainEvent>>;
}