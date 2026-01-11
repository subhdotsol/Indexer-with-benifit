use crate::{
    application::ports::parser::TransactionParser,
    domain::{SolanaTransaction, TransactionEvent, JUP_PROGRAM_ID, TxData, JupiterSwapEvent},
};
use prost::Message;
use yellowstone_grpc_proto::geyser::SubscribeUpdate;

pub struct JupiterParser;

impl JupiterParser {
    pub fn new() -> Self { Self }

    // Jupiter's various instruction discriminators
    const DISC_ROUTE: [u8; 8] = [229, 23, 203, 151, 122, 227, 173, 42];
}

impl TransactionParser for JupiterParser {
    fn name(&self) -> &str {
        "JupiterParser"
    }

    fn parse(&self, txn: &SolanaTransaction) -> Option<Vec<TransactionEvent>> {
        let mut events = Vec::new();

        match &txn.data {
            TxData::Grpc(bytes) => {
                if let Ok(update) = SubscribeUpdate::decode(bytes.as_slice()) {
                    if let Some(yellowstone_grpc_proto::geyser::subscribe_update::UpdateOneof::Transaction(tx_info)) = update.update_oneof {
                        let slot = tx_info.slot;
                        let tx_details = tx_info.transaction.unwrap();
                        let signature = bs58::encode(&tx_details.signature).into_string();
                        let message = tx_details.transaction.unwrap().message.unwrap();

                        let account_keys = message.account_keys.iter().map(|k| bs58::encode(k).into_string()).collect::<Vec<String>>();

                        if let Some(jup_pgm_idx) = account_keys.iter().position(|k| k == JUP_PROGRAM_ID) {
                            let jup_pgm_idx = jup_pgm_idx as u32;

                            for ix in message.instructions {
                                if ix.program_id_index == jup_pgm_idx {
                                    if ix.data.len() >= 8 {
                                        let disc = &ix.data[0..8];
                                        if disc == Self::DISC_ROUTE {
                                            // Basic Jupiter routing event
                                            events.push(TransactionEvent::JupiterSwap(JupiterSwapEvent {
                                                signature: signature.clone(),
                                                slot,
                                                signer: account_keys[ix.accounts[1] as usize].clone(),
                                                amm_pool: "Jupiter Aggregator".to_string(),
                                                mint_in: "unknown".to_string(),
                                                mint_out: "unknown".to_string(),
                                                amount_in: 0,
                                                amount_out: 0,
                                                slippage_bps: 0,
                                                platform_fee_bps: 0,
                                                route_plan: vec![],
                                            }));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }

        if events.is_empty() { None } else { Some(events) }
    }
}
