use std::sync::Arc;
use futures::{StreamExt, stream};
use solana_client::{rpc_client::RpcClient, rpc_config::CommitmentConfig, rpc_response::EncodedTransactionWithStatusMeta};

use crate::{application::EventBuffer, domain::{ChainEvent, SolanaTransaction, TxData}};

// This function does one thing: it wakes up, fetches data, puts it in the buffer, and dies.
pub async fn run_backfill_producer(
    rpc_url: String,
    buffer: Arc<dyn EventBuffer>,
    start_slot: u64,
    end_slot: u64,
) {
    tracing::info!("Starting Backfill Producer: {} to {}", start_slot, end_slot);

    let client = Arc::new(RpcClient::new_with_commitment(
        rpc_url,
        CommitmentConfig::confirmed(),
    ));

    let slot_stream = stream::iter(start_slot..=end_slot);

    let mut fetch_stream = slot_stream.map(|slot| {
        let client = client.clone();
        async move {
            match client.get_block(slot) {
                Ok(block) => Some((slot, block)),
                Err(e) => {
                    tracing::warn!("Skipped slot {} due to error: {}", slot, e);
                    None
                }
            }
        }
    }).buffered(10); 

    // Consume the stream and write to Buffer
    while let Some(result) = fetch_stream.next().await {
        if let Some((slot, block)) = result {
            
            let meta = ChainEvent::BlockMeta {
                slot,
                block_hash: block.blockhash.clone(),
                parent_block_hash: block.previous_blockhash.clone(),
            };

            let block_time = block.block_time.unwrap_or(0);
            
            if let Err(_) = buffer.produce(meta).await {
                tracing::error!("Pipeline closed unexpectedly");
                break;
            }

            for tx_data in block.transactions {
                if let Some(sol_tx) = map_rpx_tx(tx_data, slot,block_time) {
                    let event = ChainEvent::Transaction(sol_tx);
                    if let Err(_) = buffer.produce(event).await {
                        break; 
                    }
                }
            }
        }
    }

    tracing::info!("Backfill Producer Finished. Dropping buffer.");
    // When this function ends, 'buffer' is dropped. 
    // This automatically closes the channel, signaling the Pipeline to stop.
}

fn map_rpx_tx(tx:EncodedTransactionWithStatusMeta,slot:u64,block_time:i64)->Option<SolanaTransaction>{
    // This contains the Input of Tx
    let decoded_txn = tx
    .transaction
    .decode()
    .expect("RPC must return binary transaction");

    let signature = decoded_txn.signatures[0].to_string();

    // If we have converted it to bytes then we only get the Input of the TX, we will miss the Inner IX and output
    // let raw_bytes = bincode::serialize(&decoded_txn)
    //     .expect("Solana wire format invariant");

    let meta = tx.meta?;

    Some(SolanaTransaction {
        signature,
        success: true,
        data:TxData::Rpc { 
            tx: decoded_txn , 
            meta,
        },
        slot,
        block_time,
    })
}