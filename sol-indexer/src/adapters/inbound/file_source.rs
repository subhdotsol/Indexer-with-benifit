
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use tokio::{time::sleep};

use crate::{application::{ AppResult, TransactionSource}, domain::{ChainEvent, SolanaTransaction}};

pub struct FileSourceAdaptor{
    // We will have BufferReader<File instead>
    current_count:u64,
    max_count:u64
}

impl FileSourceAdaptor{
    pub fn new(max_count:u64)->Self{
        Self { 
            current_count :0,
            max_count
        }
    }
}

#[async_trait]
impl TransactionSource for FileSourceAdaptor{
    async fn next_event(&mut self)->AppResult<Option<ChainEvent>>{

        if self.current_count >= self.max_count {
            return Ok(None);
        }

        // Simulating DISK latency

        sleep(Duration::from_micros(10)).await;

        self.current_count += 1;

        Ok(Some(ChainEvent::Transaction(
            SolanaTransaction{
                success:true,
                slot: 1000 + self.current_count,
                data: crate::domain::TxData::Grpc(Vec::new()),
                signature: format!("sig_{}",self.current_count),
                block_time: Utc::now().timestamp(),
            })
        )
        )

    }
}