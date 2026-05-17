use std::{sync::Arc, time::Duration};

use tokio::sync::mpsc;

use crate::{application::Notifier, domain::SwapEvent};

pub struct NotificationService {
    tx: mpsc::Sender<SwapEvent>,
    threshold_amount: u64,
}

impl NotificationService {
    pub fn new(notifier: Arc<dyn Notifier>, threshold_amount: u64) -> Self {
        let (tx, mut rx) = mpsc::channel::<SwapEvent>(100);

        tokio::spawn(async move {
            tracing::info!("Notification service started");

            while let Some(swap) = rx.recv().await {
                tracing::info!("Sending alert for tx: {}", swap.signature());

                for attempt in 1..=3 {
                    match notifier.send_swap_alert(&swap).await {
                        Ok(_) => break,
                        Err(e) => {
                            tracing::warn!("Alert send failed (attempt {}): {}", attempt, e);
                            tokio::time::sleep(Duration::from_millis(500)).await;
                        }
                    }
                }
            }
        });

        Self { tx, threshold_amount }
    }

    pub async fn send_to_queue(&self, swap: &SwapEvent) {
        if swap.amount_in() < self.threshold_amount {
            return;
        }
        let _ = self.tx.send(swap.clone()).await;
    }
}
