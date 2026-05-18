use std::sync::Arc;

use futures::{StreamExt, stream};
use solana_client::{
    rpc_client::RpcClient,
    rpc_config::CommitmentConfig,
    rpc_response::EncodedTransactionWithStatusMeta,
};

use crate::{
    application::EventBuffer,
    domain::{ChainEvent, SolanaTransaction, TxData},
};

/// Fetches historical blocks from an RPC node and pushes them into the shared buffer.
/// Designed to run as a one-shot tokio task — drops the buffer handle when done,
/// which automatically signals the downstream pipeline to stop if no other producers exist.
pub async fn run_backfill_producer(
    rpc_url: String,
    buffer: Arc<dyn EventBuffer>,
    start_slot: u64,
    end_slot: u64,
) {
    tracing::info!("Backfill producer started: slots {} → {}", start_slot, end_slot);

    let client = Arc::new(RpcClient::new_with_commitment(
        rpc_url,
        CommitmentConfig::confirmed(),
    ));

    let slot_range = stream::iter(start_slot..=end_slot);

    let mut fetch_stream = slot_range
        .map(|slot| {
            let client = client.clone();
            async move {
                match client.get_block(slot) {
                    Ok(block) => Some((slot, block)),
                    Err(e) => {
                        tracing::warn!("Skipped slot {}: {}", slot, e);
                        None
                    }
                }
            }
        })
        .buffered(10);

    while let Some(result) = fetch_stream.next().await {
        if let Some((slot, block)) = result {
            let block_time = block.block_time.unwrap_or(0);

            let meta_event = ChainEvent::BlockMeta {
                slot,
                block_hash: block.blockhash.clone(),
                parent_block_hash: block.previous_blockhash.clone(),
            };

            if buffer.produce(meta_event).await.is_err() {
                tracing::error!("Pipeline closed during backfill");
                break;
            }

            for tx_with_meta in block.transactions {
                if let Some(sol_tx) = decode_rpc_transaction(tx_with_meta, slot, block_time) {
                    if buffer.produce(ChainEvent::Transaction(sol_tx)).await.is_err() {
                        break;
                    }
                }
            }
        }
    }

    tracing::info!("Backfill producer finished");
}

fn decode_rpc_transaction(
    tx: EncodedTransactionWithStatusMeta,
    slot: u64,
    block_time: i64,
) -> Option<SolanaTransaction> {
    let decoded = tx.transaction.decode()?;
    let signature = decoded.signatures[0].to_string();
    let meta = tx.meta?;

    Some(SolanaTransaction {
        signature,
        success: true,
        data: TxData::Rpc { tx: decoded, meta },
        slot,
        block_time,
    })
}
