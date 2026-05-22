use std::sync::Arc;

use anyhow::Result;
use prost::Message;
use solana_client::rpc_response::OptionSerializer;
use solana_sdk::pubkey::Pubkey;
use yellowstone_grpc_proto::{geyser::SubscribeUpdate, prelude::InnerInstruction};
use yellowstone_vixen_core::{Parser, instruction::{InstructionShared, InstructionUpdate}};
use yellowstone_vixen_proc_macro::include_vixen_parser;

use crate::{adapters::parsers::VixenUtils, application::TransactionParser, domain::{self, PumpFunTrade, SYSTEM_PROGRAM, SolanaTransaction, TransactionEvent, TxData}};


include_vixen_parser!("idls/pump_fun.json");

pub struct PumpFunParser;

impl PumpFunParser {
    pub fn new() -> Self {
        Self {}
    }

    /// Find SOL transfers in inner instructions.
    /// For Buy: user pays SOL (look for transfers FROM user)
    /// For Sell: user receives SOL (look for transfers TO user)
    fn find_sol_transfer_from(&self, inner_ixs: &[InstructionUpdate], from: &yellowstone_vixen_parser::Pubkey) -> u64 {
        let system_program_bytes = [0u8; 32];
        let mut total = 0u64;

        for ix in inner_ixs {
            if ix.program.0 != system_program_bytes { continue; }
            if ix.data.len() < 12 { continue; }

            let disc = u32::from_le_bytes(ix.data[0..4].try_into().unwrap());
            if disc != 2 { continue; } // 2 = Transfer

            if ix.accounts.len() < 2 { continue; }

            if ix.accounts[0] == *from {
                let amount = u64::from_le_bytes(ix.data[4..12].try_into().unwrap());
                total = total.saturating_add(amount);
            }
        }
        total
    }

    fn find_sol_transfer_to(&self, inner_ixs: &[InstructionUpdate], to: &yellowstone_vixen_parser::Pubkey) -> u64 {
        let system_program_bytes = [0u8; 32];
        let mut total = 0u64;

        for ix in inner_ixs {
            if ix.program.0 != system_program_bytes { continue; }
            if ix.data.len() < 12 { continue; }

            let disc = u32::from_le_bytes(ix.data[0..4].try_into().unwrap());
            if disc != 2 { continue; } // 2 = Transfer

            if ix.accounts.len() < 2 { continue; }

            // Match transfers TO the specified account
            if ix.accounts[1] == *to {
                let amount = u64::from_le_bytes(ix.data[4..12].try_into().unwrap());
                total = total.saturating_add(amount);
            }
        }
        total
    }

    /// Calculate SOL received by user from pre/post balance changes
    /// This is needed for Sell because Pump.fun doesn't use System Program Transfer
    fn calculate_sol_received(
        user_account_idx: usize,
        pre_balances: &[u64],
        post_balances: &[u64],
    ) -> u64 {
        let pre = pre_balances.get(user_account_idx).copied().unwrap_or(0);
        let post = post_balances.get(user_account_idx).copied().unwrap_or(0);
        post.saturating_sub(pre)
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

            // Proto stores static keys in `message.`account_keys and dynamic in `meta.loaded_...`
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

                if program_id.to_string() != domain::PUMP_FUN_PROGRAM_ID { continue; }

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

                let pre_token_balances: OptionSerializer<Vec<solana_client::rpc_response::UiTransactionTokenBalance>> = OptionSerializer::Some(
                meta.pre_token_balances.iter().map(|b| solana_client::rpc_response::UiTransactionTokenBalance {
                    account_index: b.account_index as u8,
                    mint: b.mint.clone(),
                    ui_token_amount: solana_client::rpc_response::UiTokenAmount {
                        ui_amount: b.ui_token_amount.clone().and_then(|a| Some(a.ui_amount)),
                        decimals: b.ui_token_amount.as_ref().map(|a| a.decimals as u8).unwrap_or(0),
                        amount: b.ui_token_amount.as_ref().map(|a| a.amount.clone()).unwrap_or_default(),
                        ui_amount_string: b.ui_token_amount.as_ref().map(|a| a.ui_amount_string.clone()).unwrap_or_default(),
                    },
                    owner: OptionSerializer::Some(b.owner.clone()),
                    program_id: OptionSerializer::Some(b.program_id.clone())
                }).collect()
            );

                let update = InstructionUpdate {
                    program: yellowstone_vixen_parser::Pubkey::from(program_id.to_bytes()),
                    accounts: vixen_accounts,
                    data: ix.data.clone(), // data is already Vec<u8>
                    shared,
                    inner: inner_ixs,
                };

                // Log instruction discriminator for debugging
                if ix.data.len() >= 8 {
                    let disc = &ix.data[0..8];
                    tracing::trace!(
                        "Pump.fun instruction discriminator: {:02x?}, data length: {}",
                        disc,
                        ix.data.len()
                    );
                }

                // Using block_in_place if running inside async runtime
                let parsed = tokio::task::block_in_place(|| {
                        tokio::runtime::Handle::current().block_on(pump::InstructionParser.parse(&update))
                });

                if let Ok(parsed_ix) = parsed {
                        match parsed_ix {
                            pump::PumpInstruction::Buy { accounts, args }=>{
                                // For Buy: user sends SOL (find transfers FROM user)
                                let sol_spent = self.find_sol_transfer_from(&update.inner, &accounts.user);

                                 tracing::info!("PumpFun gRPC Buy: Token: {} SOL: {} User: {}", args.amount, sol_spent, accounts.user);

                                events.push(TransactionEvent::PumpFunTrade(PumpFunTrade {
                                    signature: signature_str.clone(),
                                    slot,
                                    block_time,
                                    timestamp: block_time,
                                    mint: accounts.mint.to_string(),
                                    is_buy: true,
                                    user: accounts.user.to_string(),
                                    token_amount: args.amount,
                                    sol_amount: sol_spent
                                }));
                            },
                            pump::PumpInstruction::Sell { accounts, args }=>{
                                // For Sell: user receives SOL - calculate from balance changes
                                // Find user's account index in all_accounts
                                let user_pubkey = Pubkey::try_from(accounts.user.0).ok();
                                let user_idx = user_pubkey.and_then(|pk| all_accounts.iter().position(|a| a == &pk));

                                let sol_received = match user_idx {
                                    Some(idx) => Self::calculate_sol_received(idx, &meta.pre_balances, &meta.post_balances),
                                    None => 0,
                                };

                                 tracing::info!("PumpFun gRPC Sell: Token: {} SOL: {} User: {}", args.amount, sol_received, accounts.user);

                                events.push(TransactionEvent::PumpFunTrade(PumpFunTrade {
                                    signature: signature_str.clone(),
                                    slot,
                                    block_time,
                                    timestamp: block_time,
                                    mint: accounts.mint.to_string(),
                                    is_buy: false,
                                    user: accounts.user.to_string(),
                                    token_amount: args.amount,
                                    sol_amount: sol_received
                                }));
                            },
                            _ => {
                                // Other Pump.fun instructions (Create, Initialize, etc.) - skip silently
                            }
                        }
                }else if let Err(_e) = parsed {
                    // Vixen parse errors for unrecognized instructions are expected - skip silently
                    // Only actual Buy/Sell parse failures would have matched above
                    continue;
                }
            }

            if !events.is_empty() {
                return Ok(Some(events));
            }
        }

        Ok(None)
    }


}

impl TransactionParser for PumpFunParser {
    fn name(&self) -> &str { "pump_fun" }

    fn parse (&self, txn:SolanaTransaction)->Result<Option<Vec<TransactionEvent>>>{
         match txn.data{
            TxData::Grpc(bytes)=>{
                Self::parse_protobuf(&self, &bytes,txn.block_time)
            },
            TxData::Rpc { tx, meta }=>{
                // Self::parse_rpc(&self, tx, meta, txn.slot, &txn.signature)
                Ok(None) // RPC parsing not implemented yet
            }
        }
    }

}
