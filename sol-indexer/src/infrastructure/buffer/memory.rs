use tokio::sync::mpsc;
use async_trait::async_trait;
use crate::{application::{AppError, AppResult, EventBuffer}, domain::ChainEvent};

pub struct MemoryBuffer{
    tx: mpsc::Sender<ChainEvent>,
    // rx: Option<mpsc::Receiver<ChainEvent>>,  // It is a option, so that we can move it from here.
    capacity: usize,
}

impl MemoryBuffer {
    pub fn new(capacity:usize)->(Self,mpsc::Receiver<ChainEvent>){
        let (tx,rx) = mpsc::channel::<ChainEvent>(capacity);

        (Self { 
            tx, 
            capacity 
        },rx)
    }
}

#[async_trait]
impl EventBuffer for MemoryBuffer{
    async fn produce(&self, event:ChainEvent)->AppResult<()>{

        if let Err(err) = self.tx.send(event).await {
            return Err(AppError::ErrorSendingMessageViaBuffer);
        }

        Ok(())
    }

    // async fn consume(&mut self)->AppResult<Option<ChainEvent>>{
    //     unimplemented!("For MPSC, the receiver is extracted at creation");
    // }
    
    fn len(&self)->usize{
        self.capacity - self.tx.capacity()
    }
    
    fn is_empty(&self)->bool{
        self.tx.capacity() == self.capacity
    }
}