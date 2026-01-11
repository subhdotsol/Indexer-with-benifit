use crate::application::AppResult;
use crate::domain::TransactionEvent;
use async_trait::async_trait;

#[async_trait]
pub trait EventRepository: Send + Sync {
    async fn save_event(&self, event: &TransactionEvent) -> AppResult<()>;
    async fn save_events(&self, events: &[TransactionEvent]) -> AppResult<()>;
    /// Batch insert events using a single transaction for better performance.
    /// Returns the number of events persisted.
    async fn save_events_batch(&self, events: Vec<TransactionEvent>) -> AppResult<usize>;
}
