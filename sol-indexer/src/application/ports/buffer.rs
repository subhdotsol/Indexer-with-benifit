use async_trait::async_trait;

use crate::{application::{AppResult}, domain::ChainEvent};

#[async_trait]
pub trait EventBuffer: Send+Sync {
    async fn produce(&self, event:ChainEvent)->AppResult<()>;
    fn len(&self)->usize;
    fn is_empty(&self)->bool;
} 