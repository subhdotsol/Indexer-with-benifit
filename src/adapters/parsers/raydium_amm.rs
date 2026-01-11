use borsh::{BorshDeserialize, BorshSerialize};
use prost::Message;
use yellowstone_grpc_proto::geyser::SubscribeUpdate;
use crate::{
    application::ports::parser::TransactionParser,
    domain::{SolanaTransaction, TransactionEvent, RaydiumSwapEvent, TxData, RAYDIUM_V4_PROGRAM_ID},
};

#[derive(BorshDeserialize, BorshSerialize, Debug)]
pub struct RaydiumSwapInstruction {
    pub amount_in: u64,
    pub min_amount_out: u64,
}

pub struct RaydiumAmmParser;

impl RaydiumAmmParser {
    pub fn new() -> Self { Self }

    fn find_cpi_amount_grpc(
        parent_ix_idx: usize,
        target_dest_acc_idx: usize,
        inner_instructions: &[yellowstone_grpc_proto::prelude::InnerInstructions],
    ) -> Option<u64> {
        let inner_group = inner_instructions.iter().find(|ii| ii.index == parent_ix_idx as u32)?;

        for ix in &inner_group.instructions {
            // Discriminators for SPL Token: 3 (Transfer), 12 (TransferChecked)
            let first_byte = ix.data.first().copied();
            let (amount, dst_idx) = match first_byte {
                Some(3) if ix.data.len() >= 9 => {
                    let mut amt_bytes = [0u8; 8];
                    amt_bytes.copy_from_slice(&ix.data[1..9]);
                    (u64::from_le_bytes(amt_bytes), ix.accounts.get(1))
                }
                Some(12) if ix.data.len() >= 9 => {
                    let mut amt_bytes = [0u8; 8];
                    amt_bytes.copy_from_slice(&ix.data[1..9]);
                    (u64::from_le_bytes(amt_bytes), ix.accounts.get(2))
                }
                _ => continue,
            };

            if let Some(&dst) = dst_idx {
                if dst as usize == target_dest_acc_idx {
                    return Some(amount);
                }
            }
        }
        None
    }
}

impl TransactionParser for RaydiumAmmParser {
    fn name(&self) -> &str {
        "RaydiumAmmParser"
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

                        let account_keys = message.account_keys.iter().map(|k| bs58::encode(k).into_string()).collect::<Vec<String>>();

                        if let Some(raydium_pgm_idx) = account_keys.iter().position(|k| k == RAYDIUM_V4_PROGRAM_ID) {
                            let raydium_pgm_idx = raydium_pgm_idx as u32;

                            for (ix_idx, ix) in message.instructions.iter().enumerate() {
                                if ix.program_id_index == raydium_pgm_idx {
                                    let opcode = ix.data.first().copied();

                                    match opcode {
                                        Some(9) => { // SwapBaseIn
                                            if ix.accounts.len() < 18 { continue; }

                                            if let Ok(args) = RaydiumSwapInstruction::try_from_slice(&ix.data[1..17]) {
                                                let amm_pool_idx = ix.accounts[1] as usize;
                                                let _src_acc_idx = ix.accounts[15] as usize;
                                                let dest_acc_idx = ix.accounts[16] as usize;
                                                let signer_idx = ix.accounts[17] as usize;

                                                let amount_received = Self::find_cpi_amount_grpc(
                                                    ix_idx,
                                                    dest_acc_idx,
                                                    &meta.inner_instructions,
                                                ).unwrap_or(0);

                                                events.push(TransactionEvent::RaydiumSwap(RaydiumSwapEvent {
                                                    amm_pool: account_keys[amm_pool_idx].clone(),
                                                    signer: account_keys[signer_idx].clone(),
                                                    amount_in: args.amount_in,
                                                    min_amount_out: args.min_amount_out,
                                                    amount_received,
                                                    mint_source: "unknown".to_string(), // Simplified for now
                                                    mint_destination: "unknown".to_string(),
                                                    slot,
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
            _ => {}
        }

        if events.is_empty() { None } else { Some(events) }
    }
}
