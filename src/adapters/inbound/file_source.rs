use std::time::Duration;
use async_trait::async_trait;
use tokio::{time::sleep};
use crate::{application::{AppResult, TransactionSource}, domain::SolanaTransaction};

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
    async fn next_transaction(&mut self)->AppResult<Option<SolanaTransaction>>{
        if self.current_count >= self.max_count {
            return Ok(None);
        }

        // Simulating DISK latency
        sleep(Duration::from_micros(10)).await;

        self.current_count += 1;

        Ok(Some(
            SolanaTransaction{
                success:true,
                slot: 1000 + self.current_count,
                raw_bytes: vec![],
                signature: format!("sig_{}",self.current_count),
            }
        ))
    }
}
