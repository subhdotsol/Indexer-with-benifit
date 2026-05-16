use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::{
    application::{AppError, AppResult, EventBuffer},
    domain::ChainEvent,
};

pub struct MemoryBuffer {
    tx: mpsc::Sender<ChainEvent>,
    capacity: usize,
}

impl MemoryBuffer {
    pub fn new(capacity: usize) -> (Self, mpsc::Receiver<ChainEvent>) {
        let (tx, rx) = mpsc::channel::<ChainEvent>(capacity);
        (Self { tx, capacity }, rx)
    }
}

#[async_trait]
impl EventBuffer for MemoryBuffer {
    async fn produce(&self, event: ChainEvent) -> AppResult<()> {
        self.tx
            .send(event)
            .await
            .map_err(|_| AppError::ErrorSendingMessageViaBuffer)
    }

    fn len(&self) -> usize {
        self.capacity - self.tx.capacity()
    }

    fn is_empty(&self) -> bool {
        self.tx.capacity() == self.capacity
    }
}
