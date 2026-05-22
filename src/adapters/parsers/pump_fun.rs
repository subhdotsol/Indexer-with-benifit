use std::sync::Arc;

use anyhow::Result;
use prost::Message;
use solana_sdk::pubkey::Pubkey;
use yellowstone_grpc_proto::geyser::SubscribeUpdate;
use yellowstone_vixen_core::{Parser, instruction::{InstructionShared, InstructionUpdate, Path}};
use yellowstone_vixen_proc_macro::include_vixen_parser;

use crate::{
    adapters::parsers::VixenUtils,
    application::TransactionParser,
    domain::{self, PumpFunTrade, SolanaTransaction, TransactionEvent, TxData},
};

include_vixen_parser!("idls/pump_fun.json");

pub struct PumpFunParser;

impl PumpFunParser {
    pub fn new() -> Self { Self }

    /// Sum SOL sent FROM a specific account in System Program Transfer inner ixs
    fn sol_sent_from(inner_ixs: &[InstructionUpdate], from: &yellowstone_vixen_parser::Pubkey) -> u64 {
        let system_pgm = [0u8; 32];
        let mut total = 0u64;

        for ix in inner_ixs {
            if ix.program.0 != system_pgm || ix.data.len() < 12 { continue; }
            if u32::from_le_bytes(ix.data[0..4].try_into().unwrap()) != 2 { continue; }
            if ix.accounts.get(0) == Some(from) {
                total = total.saturating_add(u64::from_le_bytes(ix.data[4..12].try_into().unwrap()));
            }
        }
        total
    }

    /// Read SOL balance change for a given account index from pre/post balances
    fn sol_received_from_balances(idx: usize, pre: &[u64], post: &[u64]) -> u64 {
        post.get(idx).copied().unwrap_or(0)
            .saturating_sub(pre.get(idx).copied().unwrap_or(0))
    }

    fn parse_protobuf(&self, raw_bytes: &[u8], block_time: i64) -> Result<Option<Vec<TransactionEvent>>> {
        let update = match SubscribeUpdate::decode(raw_bytes) {
            Ok(u) => u,
            Err(_) => return Ok(None),
        };

        if let Some(yellowstone_grpc_proto::geyser::subscribe_update::UpdateOneof::Transaction(tx_info)) = update.update_oneof {
            let slot = tx_info.slot;
            let tx_details = match tx_info.transaction { Some(t) => t, None => return Ok(None) };
            let sig_bytes = tx_details.signature;
            let sig_str = bs58::encode(&sig_bytes).into_string();
            let meta = match tx_details.meta { Some(m) => m, None => return Ok(None) };
            let message = match tx_details.transaction.and_then(|t| t.message) { Some(m) => m, None => return Ok(None) };

            let all_accounts = VixenUtils::extract_accounts_from_grpc(
                &message.account_keys,
                &meta.loaded_writable_addresses,
                &meta.loaded_readonly_addresses,
            );

            let mut events = Vec::new();

            for (ix_idx, ix) in message.instructions.iter().enumerate() {
                let pgm_idx = ix.program_id_index as usize;
                if pgm_idx >= all_accounts.len() { continue; }
                if all_accounts[pgm_idx].to_string() != domain::PUMP_FUN_PROGRAM_ID { continue; }

                let shared = Arc::new(InstructionShared {
                    signature: sig_bytes.clone(),
                    slot,
                    ..Default::default()
                });

                let vixen_accounts: Vec<yellowstone_vixen_parser::Pubkey> = ix.accounts.iter()
                    .filter_map(|&i| all_accounts.get(i as usize))
                    .map(|a| yellowstone_vixen_parser::Pubkey::from(a.to_bytes()))
                    .collect();

                let inner_group = match meta.inner_instructions.iter().find(|g| g.index == ix_idx as u32) {
                    Some(g) => g,
                    None => continue,
                };

                let inner = match VixenUtils::convert_protobuf_inner_instruction(&inner_group.instructions, &all_accounts, shared.clone()) {
                    Some(v) => v,
                    None => continue,
                };

                let instruction_update = InstructionUpdate {
                    program: yellowstone_vixen_parser::Pubkey::from(all_accounts[pgm_idx].to_bytes()),
                    accounts: vixen_accounts,
                    data: ix.data.clone(),
                    shared,
                    inner,
                    path: Path::from(vec![]),
                    log_range: 0..0,
                };

                let parsed = tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(pump::InstructionParser.parse(&instruction_update))
                });

                match parsed {
                    Ok(pump::Instructions { instruction: pump::instruction::Instruction::Buy { accounts, args } }) => {
                        let sol_spent = Self::sol_sent_from(&instruction_update.inner, &accounts.user);
                        events.push(TransactionEvent::PumpFunTrade(PumpFunTrade {
                            signature: sig_str.clone(),
                            slot,
                            block_time,
                            timestamp: block_time,
                            mint: accounts.mint.to_string(),
                            is_buy: true,
                            user: accounts.user.to_string(),
                            token_amount: args.amount,
                            sol_amount: sol_spent,
                        }));
                    }
                    Ok(pump::Instructions { instruction: pump::instruction::Instruction::Sell { accounts, args } }) => {
                        let user_pubkey = Pubkey::try_from(accounts.user.0).ok();
                        let user_idx = user_pubkey.and_then(|pk| all_accounts.iter().position(|a| *a == pk));
                        let sol_received = user_idx
                            .map(|i| Self::sol_received_from_balances(i, &meta.pre_balances, &meta.post_balances))
                            .unwrap_or(0);

                        events.push(TransactionEvent::PumpFunTrade(PumpFunTrade {
                            signature: sig_str.clone(),
                            slot,
                            block_time,
                            timestamp: block_time,
                            mint: accounts.mint.to_string(),
                            is_buy: false,
                            user: accounts.user.to_string(),
                            token_amount: args.amount,
                            sol_amount: sol_received,
                        }));
                    }
                    _ => {}
                }
            }

            if !events.is_empty() { return Ok(Some(events)); }
        }

        Ok(None)
    }
}

impl TransactionParser for PumpFunParser {
    fn name(&self) -> &str { "pump_fun" }

    fn parse(&self, txn: SolanaTransaction) -> Result<Option<Vec<TransactionEvent>>> {
        match txn.data {
            TxData::Grpc(bytes) => self.parse_protobuf(&bytes, txn.block_time),
            TxData::Rpc { .. } => Ok(None), // RPC backfill not yet supported for PumpFun
        }
    }
}
