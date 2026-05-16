use anyhow::Result;
use async_trait::async_trait;
use crate::domain::SwapEvent;

#[async_trait]
pub trait Notifier: Send + Sync {
    async fn send_swap_alert(&self, swap: &SwapEvent) -> Result<()>;
}
