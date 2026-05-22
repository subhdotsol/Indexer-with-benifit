use prost::Message;
use borsh::BorshDeserialize;
use anyhow::Result;
use solana_client::rpc_response::{OptionSerializer, UiInstruction, UiParsedInstruction, UiTransactionStatusMeta};
use solana_sdk::transaction::VersionedTransaction;
use yellowstone_grpc_proto::geyser::SubscribeUpdate;
use crate::{application::TransactionParser, domain::{self, SolanaTransaction, TokenTransfer, TransactionEvent, TxData}};

#[derive(BorshDeserialize, Debug)]
pub struct SplTransferInstruction {
    pub amount: u64,
}

#[derive(BorshDeserialize, Debug)]
pub struct SplTransferCheckedInstruction {
    pub amount: u64,
    pub decimals: u8,
}

pub struct SplTokenTransfer;

impl SplTokenTransfer{

    pub fn new()->Self {Self}

    pub fn parse_protobuf(raw_bytes:&[u8])->Result<Option<Vec<TransactionEvent>>>{
        let update = SubscribeUpdate::decode(raw_bytes).unwrap();
        let mut transfers:Vec<TransactionEvent> = Vec::new();

        if let Some(yellowstone_grpc_proto::geyser::subscribe_update::UpdateOneof::Transaction( tx_info)) = update.update_oneof{
            let slot = tx_info.slot;
            let tx_details = tx_info.transaction.unwrap();
            let signature = bs58::encode(&tx_details.signature).into_string();
            let message = tx_details.transaction.unwrap().message.unwrap();

            let meta = tx_details.meta.as_ref().ok_or(anyhow::anyhow!("Meta missing"))?;
            
            let mut account_keys = message.account_keys.iter().map(|account| {
                bs58::encode(account).into_string()
            }).collect::<Vec<String>>();

            // Finding token program index
            let token_program_index = account_keys.iter().position(|k|{
                k == domain::TOKEN_PROGRAM_ID
            });

            for acc in &meta.loaded_writable_addresses {
                account_keys.push(bs58::encode(acc).into_string());
            }

            for acc in &meta.loaded_readonly_addresses {
                account_keys.push(bs58::encode(acc).into_string());
            }
            

            if let Some(pgm_idx) = token_program_index{
                let pgm_idx = pgm_idx as u32;

                // Scanning the Instructions
                for ix in message.instructions {
                    // We will check for the opcode 3 = Transfer
                    // Data : [3, ....]
                    // tracing::info!("Instrcution Data {:?}",ix.data.first());
                    // For Transfer IX - Opcode - 3 , For TransferChecked Opcode - 12
                    // Transfer IX contains 3 accounts - source, destination , authority
                    // Transfer Checked IX contains 4 accounst - source, mint , destination, authority

                    // Binary layout of Transfer 
                    // byte - 0 : Instruction discriminator (u8)
                    // bytes 1..8 : amount
                    // Total Bytes - 9 bytes

                    // Binary Layout of TransferChecked 
                    // byte - 0 : IX discriminator 
                    // bytes 1..8 : amount 
                    // byte  9 : decimal
                    // Total Size : 10 bytes

                    if ix.program_id_index != pgm_idx as u32 {
                        continue;
                    }

                    let first_byte = ix.data.first().copied(); 

                    match first_byte {
                        Some(3) if ix.data.len() >= 9 => {
                            // Transfer IX
                            if let Ok(args) = SplTransferInstruction::try_from_slice(&ix.data[1..9]) {
                                if ix.accounts.len() < 2 { continue; }
                                let from_idx = ix.accounts[0] as usize;
                                let to_idx = ix.accounts[1] as usize;

                                // Bounds check for account_keys
                            
                                if from_idx >= account_keys.len() || to_idx >= account_keys.len() {
                                    continue;
                                }

                                transfers.push(TransactionEvent::TokenTransfer(TokenTransfer { 
                                    from: account_keys[from_idx].clone(), 
                                    to: account_keys[to_idx].clone(), 
                                    mint: None,
                                    slot, 
                                    amount: args.amount, 
                                    signature: signature.clone(),
                                }));
                            }
                        }
                        Some(12) if ix.data.len() >= 10 => {
                            // Transfer Checked IX
                            if let Ok(args) = SplTransferCheckedInstruction::try_from_slice(&ix.data[1..10]) {
                                if ix.accounts.len() < 3 { continue; }
                                let from_idx = ix.accounts[0] as usize;
                                let mint = ix.accounts[1] as usize;
                                let to_idx = ix.accounts[2] as usize;

                                // Bounds check for account_keys
                                if from_idx >= account_keys.len() || to_idx >= account_keys.len() || mint >= account_keys.len() {
                                    continue;
                                }

                                transfers.push(TransactionEvent::TokenTransfer(TokenTransfer { 
                                    from: account_keys[from_idx].clone(), 
                                    to: account_keys[to_idx].clone(), 
                                    mint: Some(account_keys[mint].clone()),
                                    slot, 
                                    amount: args.amount, 
                                    signature: signature.clone(),
                                }));
                            }
                        }
                        _ => continue
                    }
                }
            }
        }
        Ok(Some(transfers))
    }

    pub fn parse_rpc(tx: &VersionedTransaction, meta: &UiTransactionStatusMeta, slot: u64, sig: &str) -> Result<Option<Vec<TransactionEvent>>> {
        let mut transfers: Vec<TransactionEvent> = Vec::new();
        let message = &tx.message;

        let mut all_accounts: Vec<String> = message.static_account_keys().iter().map(|k| k.to_string()).collect();

        match &meta.loaded_addresses {
            OptionSerializer::Some(loaded_addresses) => {
                for acc in &loaded_addresses.writable { all_accounts.push(acc.to_string()); }
                for acc in &loaded_addresses.readonly { all_accounts.push(acc.to_string()); }
            },
            _ => {}
        }

        let token_prog_idx = match all_accounts.iter().position(|acc| acc == domain::TOKEN_PROGRAM_ID) {
            Some(val) => val as u8,
            None => return Err(anyhow::anyhow!("Token program not found")),
        };

        // Helper closure
        let parse_ix = |pgm_id: u8, data: &[u8], account_indexes: &[u8]| -> Option<TokenTransfer> {
            if pgm_id != token_prog_idx { return None; }

            match data.first() {
                Some(3) if data.len() >= 9 => {
                    // Transfer IX
                    let args = SplTransferInstruction::try_from_slice(&data[1..9]).ok()?;
                    
                    let from_idx = *account_indexes.get(0)? as usize;
                    let to_idx = *account_indexes.get(1)? as usize;

                    if from_idx >= all_accounts.len() || to_idx >= all_accounts.len() {
                        tracing::warn!("Account index out of bounds in parser!");
                        return None;
                    }

                    Some(TokenTransfer { 
                        from: all_accounts[from_idx].clone(), 
                        to: all_accounts[to_idx].clone(), 
                        mint: None,
                        slot, 
                        amount: args.amount, 
                        signature: sig.to_string(),
                    })
                }
                Some(12) if data.len() >= 10 => {
                    // Transfer Checked IX
                    let args = SplTransferCheckedInstruction::try_from_slice(&data[1..10]).ok()?;

                    let from_idx = *account_indexes.get(0)? as usize;
                    let mint_idx = *account_indexes.get(1)? as usize;
                    let to_idx = *account_indexes.get(2)? as usize;

                    if from_idx >= all_accounts.len() || to_idx >= all_accounts.len() || mint_idx >= all_accounts.len() {
                        tracing::warn!("Account index out of bounds in parser!");
                        return None;
                    }

                    Some(TokenTransfer { 
                        from: all_accounts[from_idx].clone(), 
                        to: all_accounts[to_idx].clone(), 
                        mint: Some(all_accounts[mint_idx].clone()),
                        slot, 
                        amount: args.amount, 
                        signature: sig.to_string(),
                    })
                }
                _ => None,
            }
        };

        // 1. Parse Top-Level Instructions
        for ix in message.instructions() {
            if let Some(t) = parse_ix(ix.program_id_index, &ix.data, &ix.accounts) {
                transfers.push(TransactionEvent::TokenTransfer(t));
            }
        };

        // 2. Parse Inner Instructions
        if let OptionSerializer::Some(inner_ixs_groups) = &meta.inner_instructions {
            for inner_ixs in inner_ixs_groups {
                for inner_ix in &inner_ixs.instructions {
                    match inner_ix {
                        UiInstruction::Compiled(compiled_ix) => {
                            let raw_pgm_id = compiled_ix.program_id_index;
                            if let Ok(raw_data) = bs58::decode(&compiled_ix.data).into_vec() {
                                let raw_account_indexes = &compiled_ix.accounts;
                                if let Some(t) = parse_ix(raw_pgm_id, &raw_data, raw_account_indexes) {
                                    transfers.push(TransactionEvent::TokenTransfer(t));
                                }
                            }
                        },
                        UiInstruction::Parsed(parsed_ix) => {
                            match parsed_ix {
                                UiParsedInstruction::Parsed(_) => {
                                    // Already JSON parsed
                                },
                                UiParsedInstruction::PartiallyDecoded(partially_decoded_ix) => {
                                    let pgm_id = &partially_decoded_ix.program_id;
                                    if let Some(pgm_id_index) = all_accounts.iter().position(|acc| acc == pgm_id) {
                                         if let Ok(raw_data) = bs58::decode(&partially_decoded_ix.data).into_vec() {
                                            // Map accounts to indices manually
                                            let mut raw_account_indexes = Vec::new();
                                            for acc in &partially_decoded_ix.accounts {
                                                if let Some(idx) = all_accounts.iter().position(|a| a == acc) {
                                                    raw_account_indexes.push(idx as u8);
                                                }
                                            }
                                            if let Some(t) = parse_ix(pgm_id_index as u8, &raw_data, &raw_account_indexes) {
                                                transfers.push(TransactionEvent::TokenTransfer(t));
                                            }
                                         }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        };

        Ok(Some(transfers))
    }
}

impl TransactionParser for SplTokenTransfer {
    fn name(&self) -> &str { "spl_token_transfer" }

    fn parse(&self, txn: SolanaTransaction) -> Result<Option<Vec<TransactionEvent>>> {
        match txn.data {
            TxData::Grpc(bytes) => Self::parse_protobuf(&bytes),
            TxData::Rpc { tx, meta } => Self::parse_rpc(&tx, &meta, txn.slot, &txn.signature),
        }
    }
}