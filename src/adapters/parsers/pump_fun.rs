use yellowstone_grpc_proto::geyser::SubscribeUpdate;
use prost::Message;
use crate::{
    application::ports::parser::TransactionParser,
    domain::{SolanaTransaction, TransactionEvent, PumpFunSwapEvent, TxData, PUMP_FUN_PROGRAM_ID},
};

pub struct PumpFunParser;

impl PumpFunParser {
    pub fn new() -> Self { Self }

    const BUY_DISCRIMINATOR: [u8; 8] = [102, 6, 61, 18, 1, 218, 235, 234];
    const SELL_DISCRIMINATOR: [u8; 8] = [51, 230, 133, 164, 1, 127, 131, 173];
}

impl TransactionParser for PumpFunParser {
    fn name(&self) -> &str {
        "PumpFunParser"
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

                        if let Some(pump_pgm_idx) = account_keys.iter().position(|k| k == PUMP_FUN_PROGRAM_ID) {
                            let pump_pgm_idx = pump_pgm_idx as u32;

                            for ix in message.instructions {
                                if ix.program_id_index == pump_pgm_idx {
                                    if ix.data.len() < 8 { continue; }
                                    
                                    let discriminator = &ix.data[0..8];
                                    
                                    if discriminator == Self::BUY_DISCRIMINATOR {
                                        // Buy Accounts: [global, fee_recipient, mint, bonding_curve, associated_bonding_curve, associated_user, user, ...]
                                        if ix.accounts.len() < 7 { continue; }
                                        if ix.data.len() < 24 { continue; }

                                        let mut amount_bytes = [0u8; 8];
                                        amount_bytes.copy_from_slice(&ix.data[8..16]);
                                        let token_amount = u64::from_le_bytes(amount_bytes);

                                        let mut sol_bytes = [0u8; 8];
                                        sol_bytes.copy_from_slice(&ix.data[16..24]);
                                        let max_sol_cost = u64::from_le_bytes(sol_bytes);

                                        let mint_idx = ix.accounts[2] as usize;
                                        let bonding_curve_idx = ix.accounts[3] as usize;
                                        let user_idx = ix.accounts[6] as usize;

                                        events.push(TransactionEvent::PumpFunSwap(PumpFunSwapEvent {
                                            signature: signature.clone(),
                                            slot,
                                            signer: account_keys[user_idx].clone(),
                                            mint: account_keys[mint_idx].clone(),
                                            is_buy: true,
                                            sol_amount: max_sol_cost, // This is max_sol, might want to refine with actual cost from inner ixs or logs later
                                            token_amount,
                                            bonding_curve: account_keys[bonding_curve_idx].clone(),
                                        }));
                                    } else if discriminator == Self::SELL_DISCRIMINATOR {
                                        // Sell Accounts: [global, fee_recipient, mint, bonding_curve, associated_bonding_curve, associated_user, user, ...]
                                        if ix.accounts.len() < 7 { continue; }
                                        if ix.data.len() < 24 { continue; }

                                        let mut amount_bytes = [0u8; 8];
                                        amount_bytes.copy_from_slice(&ix.data[8..16]);
                                        let token_amount = u64::from_le_bytes(amount_bytes);

                                        let mut sol_bytes = [0u8; 8];
                                        sol_bytes.copy_from_slice(&ix.data[16..24]);
                                        let min_sol_output = u64::from_le_bytes(sol_bytes);

                                        let mint_idx = ix.accounts[2] as usize;
                                        let bonding_curve_idx = ix.accounts[3] as usize;
                                        let user_idx = ix.accounts[6] as usize;

                                        events.push(TransactionEvent::PumpFunSwap(PumpFunSwapEvent {
                                            signature: signature.clone(),
                                            slot,
                                            signer: account_keys[user_idx].clone(),
                                            mint: account_keys[mint_idx].clone(),
                                            is_buy: false,
                                            sol_amount: min_sol_output,
                                            token_amount,
                                            bonding_curve: account_keys[bonding_curve_idx].clone(),
                                        }));
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
