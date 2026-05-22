use std::sync::Arc;

use anyhow::Result;
use prost::Message;
use solana_sdk::{pubkey::Pubkey, transaction::VersionedTransaction};
use solana_transaction_status::{UiTransactionStatusMeta, option_serializer::OptionSerializer};
use yellowstone_grpc_proto::geyser::SubscribeUpdate;
use yellowstone_vixen_core::{Parser, instruction::{InstructionShared, InstructionUpdate, Path}};
use yellowstone_vixen_proc_macro::include_vixen_parser;

use crate::{
    adapters::parsers::VixenUtils,
    application::TransactionParser,
    domain::{JupiterSwapEvent, RouteStep, SolanaTransaction, TransactionEvent, TxData},
};

include_vixen_parser!("idls/jupiter_v6.json");

pub struct JupiterVixenParser;

impl JupiterVixenParser {
    pub fn new() -> Self { Self }

    fn map_route_plan(plan: Vec<jupiter_v6::RoutePlanStep>) -> Vec<RouteStep> {
        plan.into_iter()
            .map(|s| RouteStep {
                swap_label: format!("{:?}", s.swap),
                percent: s.percent,
                input_index: s.input_index,
                output_index: s.output_index,
            })
            .collect()
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

            let pre_balances = VixenUtils::convert_token_balances_grpc(&meta.pre_token_balances);
            let mut events = Vec::new();

            for (ix_idx, ix) in message.instructions.iter().enumerate() {
                let pgm_idx = ix.program_id_index as usize;
                if pgm_idx >= all_accounts.len() { continue; }
                if all_accounts[pgm_idx].to_string() != crate::domain::JUPITER_V6_PROGRAM_ID { continue; }

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

                let update = InstructionUpdate {
                    program: yellowstone_vixen_parser::Pubkey::from(all_accounts[pgm_idx].to_bytes()),
                    accounts: vixen_accounts,
                    data: ix.data.clone(),
                    shared,
                    inner,
                    path: Path::from(vec![]),
                    log_range: 0..0,
                };

                let parsed = tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(jupiter_v6::InstructionParser.parse(&update))
                });

                match parsed {
                    Ok(jupiter_v6::Instructions { instruction: jupiter_v6::instruction::Instruction::Route { accounts, args } }) => {
                        let mint_in = VixenUtils::get_mint(&accounts.user_source_token_account, &all_accounts, &pre_balances);
                        events.push(TransactionEvent::JupiterSwap(JupiterSwapEvent {
                            amm_pool: "Jupiter V6".to_string(),
                            signer: accounts.user_transfer_authority.to_string(),
                            amount_in: args.in_amount,
                            amount_out: args.quoted_out_amount,
                            mint_in,
                            mint_out: accounts.destination_mint.to_string(),
                            slot,
                            signature: sig_str.clone(),
                            block_time,
                            platform_fee_bps: args.platform_fee_bps,
                            route_plan: Self::map_route_plan(args.route_plan),
                            slippage_bps: args.slippage_bps,
                        }));
                    }
                    Ok(jupiter_v6::Instructions { instruction: jupiter_v6::instruction::Instruction::SharedAccountsRoute { accounts, args } }) => {
                        events.push(TransactionEvent::JupiterSwap(JupiterSwapEvent {
                            amm_pool: "Jupiter V6 Shared".to_string(),
                            signer: accounts.user_transfer_authority.to_string(),
                            amount_in: args.in_amount,
                            amount_out: args.quoted_out_amount,
                            mint_in: accounts.source_mint.to_string(),
                            mint_out: accounts.destination_mint.to_string(),
                            slot,
                            signature: sig_str.clone(),
                            block_time,
                            platform_fee_bps: args.platform_fee_bps,
                            route_plan: Self::map_route_plan(args.route_plan),
                            slippage_bps: args.slippage_bps,
                        }));
                    }
                    _ => {}
                }
            }

            if !events.is_empty() { return Ok(Some(events)); }
        }

        Ok(None)
    }

    fn parse_rpc(
        &self,
        tx: VersionedTransaction,
        meta: UiTransactionStatusMeta,
        slot: u64,
        signature: &str,
        block_time: i64,
    ) -> Result<Option<Vec<TransactionEvent>>> {
        let mut events: Vec<TransactionEvent> = Vec::new();
        let msg = &tx.message;

        let mut all_accounts: Vec<Pubkey> = msg.static_account_keys().to_vec();
        if let OptionSerializer::Some(loaded) = &meta.loaded_addresses {
            for a in &loaded.writable { if let Ok(pk) = a.parse() { all_accounts.push(pk); } }
            for a in &loaded.readonly  { if let Ok(pk) = a.parse() { all_accounts.push(pk); } }
        }

        for (ix_idx, ix) in msg.instructions().iter().enumerate() {
            let pgm_idx = ix.program_id_index as usize;
            if pgm_idx >= all_accounts.len() { continue; }
            if all_accounts[pgm_idx].to_string() != crate::domain::JUPITER_V6_PROGRAM_ID { continue; }

            let inner_group = if let OptionSerializer::Some(ref groups) = meta.inner_instructions {
                groups.iter().find(|g| g.index == ix_idx as u8)
            } else {
                None
            };

            let update = VixenUtils::to_vixen_update_rpc(
                &all_accounts[pgm_idx],
                &ix.data,
                &all_accounts,
                signature,
                slot,
                inner_group,
            );

            let parsed = tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(jupiter_v6::InstructionParser.parse(&update))
            });

            match parsed {
                Ok(jupiter_v6::Instructions { instruction: jupiter_v6::instruction::Instruction::Route { accounts, args } }) => {
                    let mint_in = VixenUtils::get_mint(&accounts.user_source_token_account, &all_accounts, &meta.pre_token_balances);
                    events.push(TransactionEvent::JupiterSwap(JupiterSwapEvent {
                        amm_pool: "Jupiter V6".to_string(),
                        signer: accounts.user_transfer_authority.to_string(),
                        amount_in: args.in_amount,
                        amount_out: args.quoted_out_amount,
                        mint_in,
                        mint_out: accounts.destination_mint.to_string(),
                        slot,
                        signature: signature.to_string(),
                        block_time,
                        platform_fee_bps: args.platform_fee_bps,
                        route_plan: Self::map_route_plan(args.route_plan),
                        slippage_bps: args.slippage_bps,
                    }));
                }
                Ok(jupiter_v6::Instructions { instruction: jupiter_v6::instruction::Instruction::SharedAccountsRoute { accounts, args } }) => {
                    events.push(TransactionEvent::JupiterSwap(JupiterSwapEvent {
                        amm_pool: "Jupiter V6 Shared".to_string(),
                        signer: accounts.user_transfer_authority.to_string(),
                        amount_in: args.in_amount,
                        amount_out: args.quoted_out_amount,
                        mint_in: accounts.source_mint.to_string(),
                        mint_out: accounts.destination_mint.to_string(),
                        slot,
                        signature: signature.to_string(),
                        block_time,
                        platform_fee_bps: args.platform_fee_bps,
                        route_plan: Self::map_route_plan(args.route_plan),
                        slippage_bps: args.slippage_bps,
                    }));
                }
                _ => {}
            }
        }

        if events.is_empty() { Ok(None) } else { Ok(Some(events)) }
    }
}

impl TransactionParser for JupiterVixenParser {
    fn name(&self) -> &str { "jupiter_vixen" }

    fn parse(&self, txn: SolanaTransaction) -> Result<Option<Vec<TransactionEvent>>> {
        match txn.data {
            TxData::Grpc(bytes) => self.parse_protobuf(&bytes, txn.block_time),
            TxData::Rpc { tx, meta } => self.parse_rpc(tx, meta, txn.slot, &txn.signature, txn.block_time),
        }
    }
}
