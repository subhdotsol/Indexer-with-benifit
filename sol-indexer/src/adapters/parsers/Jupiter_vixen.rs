use std::{str::FromStr, sync::Arc};

use prost::Message;
use solana_client::rpc_response::{OptionSerializer, UiInnerInstructions, UiInstruction, UiParsedInstruction, UiTransactionStatusMeta, UiTransactionTokenBalance};
use solana_sdk::{pubkey::Pubkey, transaction::VersionedTransaction};
use yellowstone_grpc_proto::{geyser::SubscribeUpdate, prelude::InnerInstruction};
use yellowstone_vixen_core::{Parser, instruction::{InstructionShared, InstructionUpdate}};
use yellowstone_vixen_proc_macro::include_vixen_parser;
use anyhow::Result;

use crate::{adapters::parsers::VixenUtils, application::TransactionParser, domain::{JupiterSwapEvent, RouteStep, SolanaTransaction, TransactionEvent, TxData}};

include_vixen_parser!("idls/jupiter_v6.json");

pub struct JupiterVixenParser;

impl JupiterVixenParser {

    pub fn new() -> Self { Self }

    /// Helper for RPC (Backfill) path
    fn to_vixen_update_rpc(
        program_id: &Pubkey,
        data: &[u8],
        accounts: &[Pubkey],
        signature: &str,
        slot: u64,
        inner_ixs: Option<&UiInnerInstructions>,
    ) -> InstructionUpdate {
        let shared = Arc::new(InstructionShared {
            signature: signature.as_bytes().to_vec(),
            slot,
            ..Default::default()
        });

        // Convert RPC inner instructions
        let inner_updates = if let Some(inner_container) = inner_ixs {
            inner_container.instructions.iter()
                .filter_map(|ui_ix| Self::convert_rpc_inner_instruction(ui_ix, accounts, shared.clone()))
                .collect()
        } else {
            Vec::new()
        };

        InstructionUpdate {
            program: yellowstone_vixen_parser::Pubkey::from(program_id.to_bytes()),
            accounts: accounts.iter().map(|a| yellowstone_vixen_parser::Pubkey::from(a.to_bytes())).collect(),
            data: data.to_vec(),
            shared,
            inner: inner_updates,
        }
    }

    pub fn convert_rpc_inner_instruction(
        ui_ix: &UiInstruction,
        all_accounts: &[Pubkey],
        shared: Arc<InstructionShared>,
    ) -> Option<InstructionUpdate> {
        match ui_ix {
            UiInstruction::Compiled(compiled) => {
                let prog_id = all_accounts.get(compiled.program_id_index as usize)?;
                let accs: Vec<yellowstone_vixen_parser::Pubkey> = compiled.accounts.iter()
                    .filter_map(|&idx| all_accounts.get(idx as usize))
                    .map(|a| yellowstone_vixen_parser::Pubkey::from(a.to_bytes()))
                    .collect();

                let data = bs58::decode(&compiled.data).into_vec().ok()?;

                Some(InstructionUpdate {
                    program: yellowstone_vixen_parser::Pubkey::from(prog_id.to_bytes()),
                    accounts: accs,
                    data,
                    shared,
                    inner: vec![],
                })
            },
            UiInstruction::Parsed(parsed) => {
                match parsed {
                    UiParsedInstruction::PartiallyDecoded(pd) => {
                        let prog_id = pd.program_id.parse::<Pubkey>().ok()?;
                        let accs: Vec<yellowstone_vixen_parser::Pubkey> = pd.accounts.iter()
                            .filter_map(|a| a.parse::<Pubkey>().ok())
                            .map(|a| yellowstone_vixen_parser::Pubkey::from(a.to_bytes()))
                            .collect();
                        let data = bs58::decode(&pd.data).into_vec().ok()?;


                        Some(InstructionUpdate {
                            program: yellowstone_vixen_parser::Pubkey::from(prog_id.to_bytes()),
                            accounts: accs,
                            data,
                            shared,
                            inner: vec![]
                        })
                    },
                    // If it's fully Parsed JSON (e.g. SPL Token), we CANNOT get the original bytes easily.
                    // Vixen requires `Vec<u8>`, so we must skip these or warn.
                    UiParsedInstruction::Parsed(_json) => {
                        tracing::warn!("Skipping JSON-parsed inner instruction (Vixen needs raw bytes)");
                        None
                    }
                }
            }
        }
    }

    pub fn convert_protobuf_inner_instruction(
        inner_ixs:&Vec<InnerInstruction>,
        all_accounts: &Vec<Pubkey>,
        shared:Arc<InstructionShared>
    )->Option<Vec<InstructionUpdate>>{
        // inner_ixs.iter().map(||)
        let data = inner_ixs.iter().filter_map(|ix|{

            if ix.program_id_index > all_accounts.len() as u32 {
                return None;
            }

            let pgm_id = all_accounts[ix.program_id_index as usize];

            let accs = all_accounts.iter().map(|acc|{
                yellowstone_vixen_parser::Pubkey::from(acc.to_bytes())
            }).collect();

            Some(InstructionUpdate{
                data:ix.data.clone(),
                shared:shared.clone(),
                inner:vec![],
                accounts:accs,
                program:yellowstone_vixen_parser::Pubkey::from(pgm_id.to_bytes()),
            })
        }).collect::<Vec<InstructionUpdate>>();

        Some(data)
    }

    pub fn map_route_plan(vixen_plan: Vec<jupiter_v6::RoutePlanStep>) -> Vec<RouteStep> {
        vixen_plan.into_iter().map(|plan|{
            let label = format!("{:?}",plan.swap);
            RouteStep{
                swap_label:label,
                percent: plan.percent,
                input_index:plan.input_index,
                output_index:plan.output_index
            }
        }).collect()
    }

    pub fn parse_protobuf(&self, raw_bytes: &[u8],block_time:i64) -> Result<Option<Vec<TransactionEvent>>> {
        let update = match SubscribeUpdate::decode(raw_bytes) {
            std::result::Result::Ok(u) => u,
            std::result::Result::Err(_) => return Ok(None),
        };

        if let Some(yellowstone_grpc_proto::geyser::subscribe_update::UpdateOneof::Transaction(tx_info)) = update.update_oneof {
            let slot = tx_info.slot;
            let tx_details = match tx_info.transaction {
                Some(t) => t,
                None => return Ok(None),
            };
            let signature_bytes = tx_details.signature;
            let signature_str = bs58::encode(&signature_bytes).into_string();

            let meta = match tx_details.meta {
                Some(m) => m,
                None => return Ok(None),
            };
            let tx_body = match tx_details.transaction {
                Some(t) => t,
                None => return Ok(None),
            };
            let message = match tx_body.message {
                Some(m) => m,
                None => return Ok(None),
            };

            // Proto stores static keys in `message.account_keys` and dynamic in `meta.loaded_...`
            let all_accounts = VixenUtils::extract_accounts_from_grpc(
                &message.account_keys,
                &meta.loaded_writable_addresses,
                &meta.loaded_readonly_addresses,
            );

            let mut events = Vec::new();

            for (ix_idx,ix) in message.instructions.iter().enumerate() {
                let prog_id_idx = ix.program_id_index as usize;

                if prog_id_idx >= all_accounts.len() { continue; }
                let program_id = &all_accounts[prog_id_idx];

                if program_id.to_string() != crate::domain::JUPITER_V6_PROGRAM_ID { continue; }

                let vixen_accounts: Vec<yellowstone_vixen_parser::Pubkey> = ix.accounts.iter()
                    .filter_map(|&idx| all_accounts.get(idx as usize))
                    .map(|a| yellowstone_vixen_parser::Pubkey::from(a.to_bytes()))
                    .collect();

                let shared = Arc::new(InstructionShared {
                    signature: signature_bytes.clone(),
                    slot,
                    ..Default::default()
                });

                let inner_ixs_of_this_ix = match meta.inner_instructions.iter().find(|ixs|{
                    ixs.index == ix_idx as u32
                }) {
                    Some(ixs) => ixs,
                    None => continue, // No inner instructions for this ix, skip
                };

                let inner_ixs = match VixenUtils::convert_protobuf_inner_instruction(&inner_ixs_of_this_ix.instructions, &all_accounts, shared.clone()) {
                    Some(ixs) => ixs,
                    None => continue,
                };

                let pre_token_balances = VixenUtils::convert_token_balances_grpc(&meta.pre_token_balances);

                let update = InstructionUpdate {
                    program: yellowstone_vixen_parser::Pubkey::from(program_id.to_bytes()),
                    accounts: vixen_accounts,
                    data: ix.data.clone(), // data is already Vec<u8>
                    shared,
                    inner: inner_ixs,
                };

                // Using block_in_place if running inside async runtime
                let parsed = tokio::task::block_in_place(|| {
                     tokio::runtime::Handle::current().block_on(jupiter_v6::InstructionParser.parse(&update))
                });

                if let Ok(parsed_ix) = parsed {
                     match parsed_ix {
                        jupiter_v6::Jupiter_v6Instruction::Route { accounts, args } => {
                            tracing::info!("Jupiter gRPC Swap: {} -> {}", args.in_amount, args.quoted_out_amount);

                            let mint_in = VixenUtils::get_mint(&accounts.user_source_token_account, &all_accounts, &pre_token_balances);

                            events.push(TransactionEvent::JupiterSwap(JupiterSwapEvent {
                                amm_pool: "Jupiter V6".to_string(),
                                signer: accounts.user_transfer_authority.to_string(),
                                amount_in: args.in_amount,
                                amount_out: args.quoted_out_amount,
                                mint_in: mint_in,
                                mint_out: accounts.destination_mint.to_string(),
                                slot,
                                signature: signature_str.clone(),
                                block_time,
                                platform_fee_bps:args.platform_fee_bps,
                                route_plan: Self::map_route_plan(args.route_plan),
                                slippage_bps:args.slippage_bps,
                            }));
                        },
                        jupiter_v6::Jupiter_v6Instruction::SharedAccountsRoute { accounts, args } => {
                            tracing::info!("Jupiter Shared gRPC Swap: {} -> {}", args.in_amount, args.quoted_out_amount);

                            events.push(TransactionEvent::JupiterSwap(JupiterSwapEvent {
                                amm_pool: "Jupiter V6 Shared".to_string(),
                                signer: accounts.user_transfer_authority.to_string(),
                                amount_in: args.in_amount,
                                amount_out: args.quoted_out_amount,
                                mint_in: accounts.source_mint.to_string(),
                                mint_out: accounts.destination_mint.to_string(),
                                slot,
                                signature: signature_str.clone(),
                                block_time,
                                platform_fee_bps: args.platform_fee_bps,
                                route_plan: Self::map_route_plan(args.route_plan),
                                slippage_bps: args.slippage_bps,
                            }));
                        },
                        _ => {
                            // Other Jupiter instructions - skip silently
                        }
                     }
                }else if let Err(_e) = parsed {
                    // Unrecognized instructions - skip silently
                    continue;
                }
            }

            if !events.is_empty() {
                return Ok(Some(events));
            }
        }

        Ok(None)
    }

    pub fn parse_rpc(&self, tx: VersionedTransaction, meta: UiTransactionStatusMeta, slot: u64, signature: &str,block_time:i64) -> Result<Option<Vec<TransactionEvent>>> {
        let mut events: Vec<TransactionEvent> = Vec::new();
        let msg = &tx.message;

        // Reconstruct accounts list for RPC
        let mut all_accounts = msg.static_account_keys().to_vec();
        if let OptionSerializer::Some(data) = &meta.loaded_addresses {
            for acc in &data.writable {
                if let Ok(pubkey) = Pubkey::from_str(acc) { all_accounts.push(pubkey); }
            }
            for acc in &data.readonly {
                if let Ok(pubkey) = Pubkey::from_str(acc) { all_accounts.push(pubkey); }
            }
        }

        for (ix_idx, ix) in msg.instructions().iter().enumerate() {
            let program_id_idx = ix.program_id_index as usize;
            if program_id_idx >= all_accounts.len() { continue; }
            let program_id = &all_accounts[program_id_idx];

            if program_id.to_string() != crate::domain::JUPITER_V6_PROGRAM_ID { continue; }

            let inner_ixs = if let OptionSerializer::Some(ref inner_ixs) = meta.inner_instructions {
                inner_ixs.iter().find(|ixs| ixs.index == ix_idx as u8)
            } else {
                None // Fixed: Was `continue` which skipped valid instructions without inner ixs
            };

            // Call the RPC-specific helper
            let update = VixenUtils::to_vixen_update_rpc(program_id, &ix.data, &all_accounts, signature, slot, inner_ixs);

            let parsed = tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(jupiter_v6::InstructionParser.parse(&update))
            });

            if let Ok(parsed) = parsed {
                match parsed {
                    jupiter_v6::Jupiter_v6Instruction::Route { accounts, args } => {

                        let mint_in = VixenUtils::get_mint(&accounts.user_source_token_account, &all_accounts, &meta.pre_token_balances);

                        events.push(TransactionEvent::JupiterSwap(JupiterSwapEvent {
                            amm_pool: "Jupiter V6".to_string(),
                            signer: accounts.user_transfer_authority.to_string(),
                            block_time,
                            amount_in: args.in_amount,
                            amount_out: args.quoted_out_amount,
                            mint_in,
                            mint_out: accounts.destination_mint.to_string(),
                            slot,
                            signature: signature.to_string(),
                            route_plan: Self::map_route_plan(args.route_plan),
                            platform_fee_bps: args.platform_fee_bps,
                            slippage_bps:args.slippage_bps,
                        }));
                    },
                     jupiter_v6::Jupiter_v6Instruction::SharedAccountsRoute { accounts, args } => {

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
                            platform_fee_bps:args.platform_fee_bps,
                            route_plan: Self::map_route_plan(args.route_plan),
                            slippage_bps:args.slippage_bps
                        }));
                    },
                    _ => {
                        // Other Jupiter instructions - skip silently
                    }
                }
            }else if let Err(_e) = parsed {
                // Unrecognized instructions - skip silently
                continue;
            }
        }

        if events.is_empty() { Ok(None) } else { Ok(Some(events)) }
    }
}

impl TransactionParser for JupiterVixenParser {
    fn name(&self) -> &str { "jupiter_vixen" }

    fn parse(&self, txn: SolanaTransaction) -> Result<Option<Vec<TransactionEvent>>> {
        match txn.data {
            TxData::Grpc(bytes) => Self::parse_protobuf(&self, &bytes,txn.block_time),
            TxData::Rpc { tx, meta } => Self::parse_rpc(&self, tx, meta, txn.slot, &txn.signature,txn.block_time)
        }
    }
}
