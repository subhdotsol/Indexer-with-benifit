use std::{ collections::HashMap};

use anyhow::{  Result};
use chrono::Utc;
use tonic::{ transport::{Channel, ClientTlsConfig}};
use yellowstone_grpc_proto::geyser::{SubscribeRequest, SubscribeRequestFilterBlocksMeta, SubscribeRequestFilterTransactions, SubscribeUpdate, geyser_client::GeyserClient};
use async_trait::async_trait;
use crate::{application::{AppError, AppResult, TransactionSource}, domain::{ChainEvent, SolanaTransaction, TxData}};
use prost::Message;

pub struct GrpcSourceAdaptor{
    stream: tonic::codec::Streaming<SubscribeUpdate>,
    // mapping slot -> block_time
    block_time_cache: HashMap<u64,i64>,
}

impl GrpcSourceAdaptor {
    pub async fn connect(endpoint:String,x_token:Option<String>)->Result<Self>{
        tracing::info!("Connecting to grpc endpoint: {}",endpoint);

        // Creating channel with TLS support for https endpoints
        let mut channel_builder = Channel::from_shared(endpoint.clone())?;

        // Enable TLS if using https
        if endpoint.starts_with("https://") {
            let tls_config = ClientTlsConfig::new().with_webpki_roots();
            channel_builder = channel_builder.tls_config(tls_config)?;
        }

        let channel = channel_builder.connect().await?;

        // Middleware - for every request we are attaching the API Key
        let mut client = GeyserClient::with_interceptor(channel, move | mut req : tonic::Request<()> |{
            if let Some(token) = &x_token {
                // if token exists adding that to the header
                if let Ok(val) = token.parse() {
                    req.metadata_mut().insert("x-token", val);
                }
            }
            Ok(req)

        });

        // creating subscription request & server side filters
        let mut transactions = HashMap::new();
        transactions.insert(
            "all_txs".to_string(), // name of the filter
        SubscribeRequestFilterTransactions{
            vote: Some(false),
            failed: Some(false),
            signature:None,
            account_exclude: vec![],
            account_include: vec![],
            account_required: vec![]
        });

        // also accepting block data also
        let mut blocks_meta = HashMap::new();
        blocks_meta.insert("all-blocks".to_string(), SubscribeRequestFilterBlocksMeta { });

        let request = SubscribeRequest{
            transactions,
            commitment: None,
            blocks_meta,
            ..Default::default()
        };

        // start stream
        // Geyser supports bi-directional streaming ( we can update the filters live ). Even though here we only send one request. The API expects a stream of requests
        let stream = client
            .subscribe(tokio_stream::iter(vec![request])) // send the request
            .await? // waiting for server to say "ACK"
            .into_inner(); // Tonic wraps it in Response<Streaming<SubscribeUpdate>>, we only need the stream.


        Ok(Self { stream, block_time_cache: HashMap::new()  } )
    }
}

#[async_trait]
impl TransactionSource for GrpcSourceAdaptor{
    async fn next_event(&mut self)->AppResult<Option<ChainEvent>>{
        loop{
            match self.stream.message().await {
                Ok(Some(update))=>{
                    match update.update_oneof {
                        Some(yellowstone_grpc_proto::geyser::subscribe_update::UpdateOneof::Transaction(ref tx_info))=>{
                            let tx = tx_info.transaction.as_ref().unwrap();
                            let signature = bs58::encode(&tx.signature).into_string();

                            let time_stamp = *self.block_time_cache.get(&tx_info.slot).unwrap_or(&Utc::now().timestamp());

                            // We re-serialize to raw bytes to keep our Domain Model generic
                            // Decoder will do the heavy lifting later
                            //  In a real app, we might map to a struct here, but passing raw bytes

                            let raw_bytes = update.encode_to_vec();

                            let block_time = Utc::now().timestamp();

                            // Here the txn can came early before the bloc_meta, so we can keep that in a buffer until the BlockMeta arrives.
                            return Ok(Some(ChainEvent::Transaction(SolanaTransaction {
                                signature,
                                success: true,
                                data:TxData::Grpc(raw_bytes),
                                slot: tx_info.slot,
                                block_time: time_stamp,
                            })))
                        },
                        Some(yellowstone_grpc_proto::geyser::subscribe_update::UpdateOneof::BlockMeta(block_meta_info))=>{

                            if let Some(time) = block_meta_info.block_time{
                                self.block_time_cache.insert(block_meta_info.slot, time.timestamp);

                                self.block_time_cache.retain(|&k,_|{
                                    // keep only the last 1000 entries
                                    k > block_meta_info.slot.saturating_sub(1000)
                                });

                            }

                            return Ok(Some(ChainEvent::BlockMeta {
                                slot: block_meta_info.slot,
                                block_hash: block_meta_info.blockhash,
                                parent_block_hash: block_meta_info.parent_blockhash
                            }))
                        }
                        _=> continue,
                    }
                },
                Ok(None)=> return Ok(None),
                Err(e)=>{
                    return Err(AppError::GrpcStreamingError)
                }
            }
        }

    }

}
