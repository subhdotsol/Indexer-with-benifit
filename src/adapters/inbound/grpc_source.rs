use std::collections::HashMap;
use anyhow::Result;
use tonic::transport::{Channel, ClientTlsConfig, Endpoint};
use yellowstone_grpc_proto::geyser::{
    SubscribeRequest, SubscribeRequestFilterBlocksMeta, SubscribeRequestFilterTransactions,
    SubscribeUpdate, geyser_client::GeyserClient,
};
use async_trait::async_trait;
use crate::{
    application::{AppError, AppResult, TransactionSource},
    domain::{ChainEvent, SolanaTransaction, TxData},
};
use prost::Message;

pub struct GrpcSourceAdaptor {
    stream: tonic::codec::Streaming<SubscribeUpdate>,
}

impl GrpcSourceAdaptor {
    pub async fn connect(endpoint: String, x_token: Option<String>) -> Result<Self> {
        tracing::info!("Connecting to gRPC endpoint: {}", endpoint);

        let mut endpoint_builder = Endpoint::from_shared(endpoint)?;
        
        if endpoint_builder.uri().scheme_str() == Some("https") {
            endpoint_builder = endpoint_builder.tls_config(ClientTlsConfig::new().with_native_roots())?;
        }

        let channel = endpoint_builder.connect().await?;

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
            commitment: None,
            blocks_meta,
            ..Default::default()
        };

        let stream = client
            .subscribe(tokio_stream::iter(vec![request]))
            .await?
            .into_inner();

        Ok(Self { stream })
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
                            let raw_bytes = update.encode_to_vec();
                            let block_time = chrono::Utc::now().timestamp();

                            return Ok(Some(ChainEvent::Transaction(SolanaTransaction {
                                signature,
                                success: true,
                                data: TxData::Grpc(raw_bytes),
                                slot: tx_info.slot,
                                block_time: Some(block_time),
                            })));
                        }
                        Some(yellowstone_grpc_proto::geyser::subscribe_update::UpdateOneof::BlockMeta(block_meta_info)) => {
                            return Ok(Some(ChainEvent::BlockMeta {
                                slot: block_meta_info.slot,
                                block_hash: block_meta_info.blockhash,
                                parent_block_hash: block_meta_info.parent_blockhash,
                            }));
                        }
                        _ => continue,
                    }
                }
                Ok(None) => return Ok(None),
                Err(_e) => return Err(AppError::InvalidSource), // Replace with more specific error if needed
            }
        }
    }
}
