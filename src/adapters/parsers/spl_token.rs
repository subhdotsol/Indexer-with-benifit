use anyhow::Result;
use borsh::BorshDeserialize;
use prost::Message;
use solana_client::rpc_response::{OptionSerializer, UiInstruction, UiParsedInstruction, UiTransactionStatusMeta};
use solana_sdk::transaction::VersionedTransaction;
use yellowstone_grpc_proto::geyser::SubscribeUpdate;

use crate::{
    application::TransactionParser,
    domain::{self, SolanaTransaction, TokenTransfer, TransactionEvent, TxData},
};

#[derive(BorshDeserialize, Debug)]
struct SplTransferArgs {
    pub amount: u64,
}

#[derive(BorshDeserialize, Debug)]
struct SplTransferCheckedArgs {
    pub amount: u64,
    pub decimals: u8,
}

pub struct SplTokenTransfer;

impl SplTokenTransfer {
    pub fn new() -> Self { Self }

    fn parse_protobuf(raw_bytes: &[u8]) -> Result<Option<Vec<TransactionEvent>>> {
        let update = SubscribeUpdate::decode(raw_bytes)?;
        let mut transfers: Vec<TransactionEvent> = Vec::new();

        if let Some(yellowstone_grpc_proto::geyser::subscribe_update::UpdateOneof::Transaction(tx_info)) = update.update_oneof {
            let slot = tx_info.slot;
            let tx_details = tx_info.transaction.unwrap();
            let signature = bs58::encode(&tx_details.signature).into_string();
            let message = tx_details.transaction.unwrap().message.unwrap();
            let meta = tx_details.meta.as_ref().ok_or_else(|| anyhow::anyhow!("Missing meta"))?;

            let mut account_keys: Vec<String> = message.account_keys.iter()
                .map(|k| bs58::encode(k).into_string())
                .collect();

            let token_prog_idx = account_keys.iter().position(|k| k == domain::TOKEN_PROGRAM_ID);

            for acc in &meta.loaded_writable_addresses {
                account_keys.push(bs58::encode(acc).into_string());
            }
            for acc in &meta.loaded_readonly_addresses {
                account_keys.push(bs58::encode(acc).into_string());
            }

            if let Some(pgm_idx) = token_prog_idx {
                let pgm_idx = pgm_idx as u32;

                for ix in message.instructions {
                    if ix.program_id_index != pgm_idx { continue; }

                    match ix.data.first().copied() {
                        Some(3) if ix.data.len() >= 9 => {
                            if let Ok(args) = SplTransferArgs::try_from_slice(&ix.data[1..9]) {
                                if ix.accounts.len() < 2 { continue; }
                                let from_idx = ix.accounts[0] as usize;
                                let to_idx = ix.accounts[1] as usize;
                                if from_idx >= account_keys.len() || to_idx >= account_keys.len() { continue; }
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
                            if let Ok(args) = SplTransferCheckedArgs::try_from_slice(&ix.data[1..10]) {
                                if ix.accounts.len() < 3 { continue; }
                                let from_idx = ix.accounts[0] as usize;
                                let mint_idx = ix.accounts[1] as usize;
                                let to_idx = ix.accounts[2] as usize;
                                if from_idx >= account_keys.len() || to_idx >= account_keys.len() || mint_idx >= account_keys.len() { continue; }
                                transfers.push(TransactionEvent::TokenTransfer(TokenTransfer {
                                    from: account_keys[from_idx].clone(),
                                    to: account_keys[to_idx].clone(),
                                    mint: Some(account_keys[mint_idx].clone()),
                                    slot,
                                    amount: args.amount,
                                    signature: signature.clone(),
                                }));
                            }
                        }
                        _ => continue,
                    }
                }
            }
        }

        Ok(Some(transfers))
    }

    fn parse_rpc(
        tx: &VersionedTransaction,
        meta: &UiTransactionStatusMeta,
        slot: u64,
        sig: &str,
    ) -> Result<Option<Vec<TransactionEvent>>> {
        let mut transfers: Vec<TransactionEvent> = Vec::new();
        let message = &tx.message;

        let mut all_keys: Vec<String> = message.static_account_keys().iter().map(|k| k.to_string()).collect();
        if let OptionSerializer::Some(loaded) = &meta.loaded_addresses {
            for acc in &loaded.writable { all_keys.push(acc.clone()); }
            for acc in &loaded.readonly { all_keys.push(acc.clone()); }
        }

        let token_prog_idx = match all_keys.iter().position(|k| k == domain::TOKEN_PROGRAM_ID) {
            Some(idx) => idx as u8,
            None => return Ok(Some(transfers)),
        };

        let parse_ix = |pgm_id: u8, data: &[u8], accounts: &[u8]| -> Option<TokenTransfer> {
            if pgm_id != token_prog_idx { return None; }
            match data.first() {
                Some(3) if data.len() >= 9 => {
                    let args = SplTransferArgs::try_from_slice(&data[1..9]).ok()?;
                    let from = all_keys.get(*accounts.get(0)? as usize)?.clone();
                    let to = all_keys.get(*accounts.get(1)? as usize)?.clone();
                    Some(TokenTransfer { from, to, mint: None, slot, amount: args.amount, signature: sig.to_string() })
                }
                Some(12) if data.len() >= 10 => {
                    let args = SplTransferCheckedArgs::try_from_slice(&data[1..10]).ok()?;
                    let from = all_keys.get(*accounts.get(0)? as usize)?.clone();
                    let mint = Some(all_keys.get(*accounts.get(1)? as usize)?.clone());
                    let to = all_keys.get(*accounts.get(2)? as usize)?.clone();
                    Some(TokenTransfer { from, to, mint, slot, amount: args.amount, signature: sig.to_string() })
                }
                _ => None,
            }
        };

        for ix in message.instructions() {
            if let Some(t) = parse_ix(ix.program_id_index, &ix.data, &ix.accounts) {
                transfers.push(TransactionEvent::TokenTransfer(t));
            }
        }

        if let OptionSerializer::Some(inner_groups) = &meta.inner_instructions {
            for group in inner_groups {
                for inner_ix in &group.instructions {
                    if let UiInstruction::Compiled(c) = inner_ix {
                        if let Ok(raw) = bs58::decode(&c.data).into_vec() {
                            if let Some(t) = parse_ix(c.program_id_index, &raw, &c.accounts) {
                                transfers.push(TransactionEvent::TokenTransfer(t));
                            }
                        }
                    }
                }
            }
        }

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
