use std::collections::HashMap;

use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use prost::Message;
use tonic::transport::{Channel, ClientTlsConfig};
use yellowstone_grpc_proto::geyser::{
    SubscribeRequest, SubscribeRequestFilterBlocksMeta, SubscribeRequestFilterTransactions,
    SubscribeUpdate, geyser_client::GeyserClient,
};

use crate::{
    application::{AppError, AppResult, TransactionSource},
    domain::{ChainEvent, SolanaTransaction, TxData},
};

pub struct GrpcSourceAdaptor {
    stream: tonic::codec::Streaming<SubscribeUpdate>,
    // slot → block_time cache to assign accurate timestamps to transactions
    block_time_cache: HashMap<u64, i64>,
}

impl GrpcSourceAdaptor {
    pub async fn connect(endpoint: String, x_token: Option<String>) -> Result<Self> {
        tracing::info!("Connecting to gRPC endpoint: {}", endpoint);

        let mut channel_builder = Channel::from_shared(endpoint.clone())?;

        if endpoint.starts_with("https://") {
            let tls = ClientTlsConfig::new().with_webpki_roots();
            channel_builder = channel_builder.tls_config(tls)?;
        }

        let channel = channel_builder.connect().await?;

        let mut client = GeyserClient::with_interceptor(channel, move |mut req: tonic::Request<()>| {
            if let Some(token) = &x_token {
                if let Ok(val) = token.parse() {
                    req.metadata_mut().insert("x-token", val);
                }
            }
            Ok(req)
        });

        let mut transactions = HashMap::new();
        transactions.insert(
            "all_txs".to_string(),
            SubscribeRequestFilterTransactions {
                vote: Some(false),
                failed: Some(false),
                signature: None,
                account_exclude: vec![],
                account_include: vec![],
                account_required: vec![],
            },
        );

        let mut blocks_meta = HashMap::new();
        blocks_meta.insert("all-blocks".to_string(), SubscribeRequestFilterBlocksMeta {});

        let request = SubscribeRequest {
            transactions,
            blocks_meta,
            commitment: None,
            ..Default::default()
        };

        // Geyser uses bidirectional streaming — we send one request then only read
        let stream = client
            .subscribe(tokio_stream::iter(vec![request]))
            .await?
            .into_inner();

        Ok(Self { stream, block_time_cache: HashMap::new() })
    }
}

#[async_trait]
impl TransactionSource for GrpcSourceAdaptor {
    async fn next_event(&mut self) -> AppResult<Option<ChainEvent>> {
        loop {
            match self.stream.message().await {
                Ok(Some(update)) => {
                    match update.update_oneof {
                        Some(yellowstone_grpc_proto::geyser::subscribe_update::UpdateOneof::Transaction(ref tx_info)) => {
                            let tx = tx_info.transaction.as_ref().unwrap();
                            let signature = bs58::encode(&tx.signature).into_string();

                            // Use cached block_time if available, fall back to wall-clock
                            let block_time = *self.block_time_cache
                                .get(&tx_info.slot)
                                .unwrap_or(&Utc::now().timestamp());

                            let raw_bytes = update.encode_to_vec();

                            return Ok(Some(ChainEvent::Transaction(SolanaTransaction {
                                signature,
                                success: true,
                                data: TxData::Grpc(raw_bytes),
                                slot: tx_info.slot,
                                block_time,
                            })));
                        }

                        Some(yellowstone_grpc_proto::geyser::subscribe_update::UpdateOneof::BlockMeta(meta)) => {
                            if let Some(time) = meta.block_time {
                                self.block_time_cache.insert(meta.slot, time.timestamp);
                                // Evict old entries — keep only the last 1000 slots
                                self.block_time_cache.retain(|&k, _| k > meta.slot.saturating_sub(1000));
                            }

                            return Ok(Some(ChainEvent::BlockMeta {
                                slot: meta.slot,
                                block_hash: meta.blockhash,
                                parent_block_hash: meta.parent_blockhash,
                            }));
                        }

                        _ => continue,
                    }
                }
                Ok(None) => return Ok(None),
                Err(_) => return Err(AppError::GrpcStreamingError),
            }
        }
    }
}
