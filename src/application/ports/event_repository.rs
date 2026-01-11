use crate::application::AppResult;
use crate::domain::TransactionEvent;
use async_trait::async_trait;

#[async_trait]
pub trait EventRepository: Send + Sync {
    async fn save_event(&self, event: &TransactionEvent) -> AppResult<()>;
    async fn save_events(&self, events: &[TransactionEvent]) -> AppResult<()>;
}
