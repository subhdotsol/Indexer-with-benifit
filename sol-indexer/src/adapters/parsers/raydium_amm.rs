use borsh::{BorshDeserialize, BorshSerialize};
use prost::Message;
use solana_client::rpc_response::{OptionSerializer, UiInnerInstructions, UiInstruction, UiTransactionStatusMeta, UiTransactionTokenBalance};
use solana_sdk::transaction::VersionedTransaction;
use yellowstone_grpc_proto::{geyser::SubscribeUpdate, prelude::TokenBalance};
use anyhow::Result;

use crate::{adapters::parsers::VixenUtils, application::TransactionParser, domain::{self, RaydiumSwapEvent, SolanaTransaction, TransactionEvent, TxData}};

#[derive(BorshDeserialize,BorshSerialize,Debug)]
pub struct RaydiumSwapInstruction {
    pub amount_in: u64,
    pub min_amount_out: u64,
}

pub struct RaydiumAmmParser;

impl RaydiumAmmParser{

    pub fn new()->Self{ Self }

    pub fn parse_protobuf(&self,raw_bytes:&[u8],block_time:i64)->Result<Option<Vec<TransactionEvent>>>{
        let update = SubscribeUpdate::decode(raw_bytes).unwrap();

        let mut transfers:Vec<TransactionEvent> = Vec::new();

        if let Some(yellowstone_grpc_proto::geyser::subscribe_update::UpdateOneof::Transaction(tx_info)) = update.update_oneof{
            let slot = tx_info.slot;
            let tx_details = tx_info.transaction.unwrap();
            let signature = bs58::encode(tx_details.signature).into_string();
            let txn_message = tx_details.transaction.unwrap().message.unwrap();
            let meta = tx_details.meta.unwrap();

            let all_accounts = VixenUtils::extract_accounts_from_grpc(
                &txn_message.account_keys,
                &meta.loaded_writable_addresses,
                &meta.loaded_readonly_addresses,
            );
            let account_keys: Vec<String> = all_accounts.iter().map(|k| k.to_string()).collect();

            // Finding Raydium Program Address

            let raydium_amm_pgm_idx = account_keys.iter().position(|key| key == domain::RAYDIUM_V4_PROGRAM_ID);

            if let Some(raydium_pgm_idx) = raydium_amm_pgm_idx {
                let raydium_pgm_id = raydium_pgm_idx as u32;

                for (ix_idx,ix) in txn_message.instructions.iter().enumerate(){
                    // Opcode - 9 is for the SwapInstructionBaseIn
                    // Format -
                    // byte - 0 : Discriminator
                    // byte - 1..8(inclusive) : amount_in
                    // byte - 9..16 : minimum_amount_out
                    // The signer is at 16th index
                    // AMM pool is at 1st index
                    let opcode = ix.data.first().copied();

                    match opcode {
                        Some(9)=>{
                            if ix.accounts.len() <=17 { continue; }

                            // let mut amount_in_bytes = [0u8;8];
                            // amount_in_bytes.copy_from_slice(&ix.data[1..9]);
                            // let amount_in = u64::from_le_bytes(amount_in_bytes);

                            // let mut minimum_amount_out = [0u8;8];
                            // minimum_amount_out.copy_from_slice(&ix.data[9..17]);
                            // let minimum_amount_out = u64::from_le_bytes(minimum_amount_out);

                            let args = match RaydiumSwapInstruction::try_from_slice(&ix.data[1..17]){
                                Ok(data)=>{
                                    data
                                }
                                Err(e)=>{
                                    return Err(anyhow::anyhow!("Raydium Parse Error: {:?}", e));
                                },
                            };


                            let signer_idx = ix.accounts[17] as usize;
                            let amm_pool_idx = ix.accounts[1] as usize;
                            let src_acc_idx = ix.accounts[15] as usize;
                            let dest_acc_idx = ix.accounts[16] as usize;

                            // CPI Parsing
                            // We will look for a transfer to dest_acc inside inner ix of 'ix_idx'
                            let amount_received = Self::find_cpi_amount_grpc(
                                ix_idx,
                                dest_acc_idx,
                                &meta.inner_instructions,
                                // &txn_message.instructions,
                            ).unwrap_or(0);

                            let mint_source = Self::resolve_mint_grpc(src_acc_idx, &meta.pre_token_balances, &meta.post_token_balances);
                            let mint_destination = Self::resolve_mint_grpc(dest_acc_idx, &meta.pre_token_balances, &meta.post_token_balances);

                            transfers.push(
                                TransactionEvent::RaydiumSwap(RaydiumSwapEvent {
                                    amm_pool: account_keys[amm_pool_idx].clone(),
                                    signer: account_keys[signer_idx].clone(),
                                    amount_in:args.amount_in,
                                    min_amount_out: args.min_amount_out,
                                    amount_received,
                                    mint_source,
                                    mint_destination,
                                    slot,
                                    block_time,
                                    signature:signature.clone()
                                })
                            );
                        },
                        _=>continue
                    }

                }
            }
        };

        Ok(Some(transfers))
    }

    pub fn parse_rpc(&self,tx:VersionedTransaction,meta:UiTransactionStatusMeta,slot: u64,signature: &str,)->Result<Option<Vec<TransactionEvent>>>{
        let mut transfers:Vec<TransactionEvent> = Vec::new();
        let tx_message = &tx.message;


        let mut all_accounts = tx_message.static_account_keys().iter().map(|key|{
            key.to_string()
        }).collect::<Vec<String>>();

        let loaded_addresses = meta.loaded_addresses;

        match loaded_addresses{
            OptionSerializer::Some(data)=>{
                for acc in data.writable {
                    all_accounts.push(acc);
                }

                for acc in data.readonly {
                    all_accounts.push(acc);
                }
            },
            _=>{}
        }

        let raydium_pgm_idx = all_accounts.iter().position(|acc| acc == domain::RAYDIUM_V4_PROGRAM_ID);

        if let Some(raydium_pgm_idx) = raydium_pgm_idx{
            let raydium_pgm_idx = raydium_pgm_idx as u8;

            for (ix_idx,ix) in tx_message.instructions().iter().enumerate() {
                if ix.program_id_index != raydium_pgm_idx {
                    continue;
                }

                if ix.data.len() >=17 && ix.data[0] == 9{
                    let args = match RaydiumSwapInstruction::try_from_slice(&ix.data[1..17]){
                        Ok(data)=>{
                            data
                        }
                        Err(e)=>{
                            return Err(anyhow::anyhow!("Raydium Parse Error: {:?}", e));
                        }
                    };

                    let amm_pool_idx = *ix.accounts.get(1).ok_or_else(|| anyhow::anyhow!("Missing amm_pool account"))? as usize;
                    let src_acc_idx = *ix.accounts.get(15).ok_or_else(|| anyhow::anyhow!("Missing src account"))? as usize;
                    let dst_acc_idx = *ix.accounts.get(16).ok_or_else(|| anyhow::anyhow!("Missing dst account"))? as usize;
                    let signer_idx = *ix.accounts.get(17).ok_or_else(|| anyhow::anyhow!("Missing signer account"))? as usize;

                    let amount_received = Self::find_cpi_amount_rpc(
                        ix_idx,
                        dst_acc_idx as usize,
                        &meta.inner_instructions,
                    ).unwrap_or(0);

                    let pre_token_balances = meta.pre_token_balances.as_ref().map(|x| x.as_slice()).unwrap_or(&[]);
                    let post_token_balance = meta.post_token_balances.as_ref().map(|x| x.as_slice()).unwrap_or(&[]);
                    let mint_source = Self::resolve_mint_rpc(src_acc_idx as usize, pre_token_balances, post_token_balance);
                    let mint_destination = Self::resolve_mint_rpc(dst_acc_idx as usize, pre_token_balances, post_token_balance);

                    transfers.push(
                        TransactionEvent::RaydiumSwap(RaydiumSwapEvent {
                            amm_pool: all_accounts[amm_pool_idx].clone(),
                            signer: all_accounts[signer_idx].clone(),
                            amount_in:args.amount_in,
                            min_amount_out:args.min_amount_out,
                            amount_received,
                            mint_source,
                            mint_destination,
                            slot,
                            signature:signature.to_string()
                        })
                    );
                }
            }
        }

        Ok(Some(transfers))
    }

    // CPI LOGIC

    fn find_cpi_amount_grpc(
        parent_ix_idx:usize,
        target_dest_acc_idx:usize,
        inner_instructions: &[yellowstone_grpc_proto::prelude::InnerInstructions],
        // _all_ix: &[yellowstone_grpc_proto::prelude::CompiledInstruction]
    )->Option<u64>{
        // Finding the group of inner Ix that belong to provided parent Ix Index
        let inner_group = inner_instructions.iter().find(|inner_ixs| inner_ixs.index == parent_ix_idx as u32 )?;

        for ix in &inner_group.instructions{
            // Checking for SPL token transfer (opcode 3) or TransferChecked (Opcode 12)
            // For now skipping the Pgm Id check

            let (amount , dst_idx) = if ix.data.first() == Some(&3) && ix.data.len() >=9{
                // Transfer : [3,amount]
                // Accounts : [src,dest,authority]
                if ix.accounts.len() < 2 {continue;}
                let mut amount_in_bytes = [0u8;8];
                amount_in_bytes.copy_from_slice(&ix.data[1..9]);
                (u64::from_le_bytes(amount_in_bytes),ix.accounts.get(1)?)
            } else if ix.data.first() == Some(&12) && ix.data.len() >=9 {
                // TransferChecked: [12, amount(8), decimals(1)]
                 // Accounts: [src, mint, dst, auth]
                 if ix.accounts.len() < 3 { continue; }
                 let mut amt_bytes = [0u8; 8];
                 amt_bytes.copy_from_slice(&ix.data[1..9]);
                 (u64::from_le_bytes(amt_bytes),ix.accounts.get(2)?)
            }else {
                continue;
            };

            if *dst_idx == target_dest_acc_idx as u8 {
                return Some(amount);
            }
        }
        return None;
    }

    fn find_cpi_amount_rpc(
        parent_ix_id:usize,
        target_dest_acc_idx:usize,
        inner_instructions: &OptionSerializer<Vec<UiInnerInstructions>>
    )->Option<u64>{
        if let OptionSerializer::Some(inner_inx) = inner_instructions {
            let ix_group = inner_inx.iter().find(|ixs| ixs.index == parent_ix_id as u8)?;

            for ix in &ix_group.instructions{
                match ix {
                    UiInstruction::Compiled(compiled)=>{
                        let raw_bytes = bs58::decode(&compiled.data).into_vec().unwrap();
                        let (amount, dst_idx) = if raw_bytes.first() == Some(&3) && raw_bytes.len() >= 9 {
                            let mut amt_bytes = [0u8; 8];
                            amt_bytes.copy_from_slice(&raw_bytes[1..9]);
                            (u64::from_le_bytes(amt_bytes), compiled.accounts.get(1)?)
                        } else if raw_bytes.first() == Some(&12) && raw_bytes.len() >= 9 {
                            let mut amt_bytes = [0u8; 8];
                            amt_bytes.copy_from_slice(&raw_bytes[1..9]);
                            (u64::from_le_bytes(amt_bytes), compiled.accounts.get(2)?)
                        } else {
                            continue;
                        };

                        if *dst_idx as usize == target_dest_acc_idx {
                            return Some(amount);
                        }
                    },
                    UiInstruction::Parsed(UiParsedInstruction)=>{
                        continue;
                    }
                }
            }
        }
        None
    }

    fn resolve_mint_rpc(
        account_idx:usize,
        pre: &[UiTransactionTokenBalance],
        post: &[UiTransactionTokenBalance],
    )->String{
        pre
        .iter()
        .find(|acc| acc.account_index == account_idx as u8)
        .map(|b| b.mint.clone())
        .or_else(|| post.iter().find(|iter| iter.account_index == account_idx as u8)
            .map(|b| b.mint.clone()))
        .unwrap_or_else(|| "unkown".to_string())
    }

    fn resolve_mint_grpc(
        account_idx: usize,
        pre: &[TokenBalance],
        post: &[TokenBalance],
    ) -> String {
        pre
        .iter()
        .find(|token_balance| token_balance.account_index == account_idx as u32)
        .map(|val| val.mint.clone())
        .or_else(|| post.iter().find(|token_balance| token_balance.account_index == account_idx as u32)
            .map(|val| val.mint.clone())
        ).unwrap_or_else(|| "unknown".to_string())
    }
}

impl TransactionParser for RaydiumAmmParser{
    fn name(&self)->&str {
        "raydium_amm"
    }

    fn parse (&self, txn:SolanaTransaction)->Result<Option<Vec<TransactionEvent>>> {
        match txn.data{
            TxData::Grpc(bytes)=>{
                Self::parse_protobuf(&self, &bytes,txn.block_time)
            },
            TxData::Rpc { tx, meta }=>{
                Self::parse_rpc(&self, tx, meta, txn.slot, &txn.signature)
            }
        }
    }
}
