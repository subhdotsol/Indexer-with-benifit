use prost::Message;
use yellowstone_grpc_proto::geyser::SubscribeUpdate;
use crate::{
    application::ports::parser::TransactionParser,
    domain::{SolanaTransaction, TransactionEvent, TokenTransfer, TxData},
};

pub struct SplTokenParser;

impl SplTokenParser {
    pub const TOKEN_PROGRAM_ID: &'static str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
}

impl TransactionParser for SplTokenParser {
    fn name(&self) -> &str {
        "SplTokenParser"
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
                        let meta = tx_details.meta.unwrap();
                        
                        let mut account_keys = message.account_keys.iter().map(|account| {
                            bs58::encode(account).into_string()
                        }).collect::<Vec<String>>();

                        // Resolve Address Lookup Tables (ALTs)
                        for addr in meta.loaded_writable_addresses {
                            account_keys.push(bs58::encode(addr).into_string());
                        }
                        for addr in meta.loaded_readonly_addresses {
                            account_keys.push(bs58::encode(addr).into_string());
                        }

                        let token_program_index = account_keys.iter().position(|k| k == Self::TOKEN_PROGRAM_ID);

                        if let Some(pgm_idx) = token_program_index {
                            let pgm_idx = pgm_idx as u32;

                            for ix in message.instructions {
                                if ix.program_id_index == pgm_idx {
                                    let first_byte = ix.data.first().copied();
                                    
                                    match first_byte {
                                        Some(3) => { // Transfer
                                            if ix.data.len() >= 9 {
                                                let mut amount_bytes = [0u8; 8];
                                                amount_bytes.copy_from_slice(&ix.data[1..9]);
                                                let amount = u64::from_le_bytes(amount_bytes);

                                                let from_idx = ix.accounts[0] as usize;
                                                let to_idx = ix.accounts[1] as usize;

                                                events.push(TransactionEvent::TokenTransfer(TokenTransfer {
                                                    from: account_keys[from_idx].clone(),
                                                    to: account_keys[to_idx].clone(),
                                                    mint: None,
                                                    slot,
                                                    amount,
                                                    signature: signature.clone(),
                                                }));
                                            }
                                        }
                                        Some(12) => { // TransferChecked
                                            if ix.data.len() >= 9 {
                                                let mut amount_bytes = [0u8; 8];
                                                amount_bytes.copy_from_slice(&ix.data[1..9]);
                                                let amount = u64::from_le_bytes(amount_bytes);

                                                let from_idx = ix.accounts[0] as usize;
                                                let mint_idx = ix.accounts[1] as usize;
                                                let to_idx = ix.accounts[2] as usize;

                                                events.push(TransactionEvent::TokenTransfer(TokenTransfer {
                                                    from: account_keys[from_idx].clone(),
                                                    to: account_keys[to_idx].clone(),
                                                    mint: Some(account_keys[mint_idx].clone()),
                                                    slot,
                                                    amount,
                                                    signature: signature.clone(),
                                                }));
                                            }
                                        }
                                        _ => continue,
                                    }
                                }
                            }
                        }
                    }
                }
            }
            _ => {} // Other data types not implemented in this parser
        }

        if events.is_empty() {
            None
        } else {
            Some(events)
        }
    }
}
