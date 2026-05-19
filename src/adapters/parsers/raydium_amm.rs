use anyhow::Result;
use borsh::{BorshDeserialize, BorshSerialize};
use prost::Message;
use solana_client::rpc_response::{OptionSerializer, UiInnerInstructions, UiInstruction, UiTransactionStatusMeta, UiTransactionTokenBalance};
use solana_sdk::transaction::VersionedTransaction;
use yellowstone_grpc_proto::{geyser::SubscribeUpdate, prelude::TokenBalance};

use crate::{
    adapters::parsers::VixenUtils,
    application::TransactionParser,
    domain::{self, RaydiumSwapEvent, SolanaTransaction, TransactionEvent, TxData},
};

#[derive(BorshDeserialize, BorshSerialize, Debug)]
pub struct RaydiumSwapInstruction {
    pub amount_in: u64,
    pub min_amount_out: u64,
}

pub struct RaydiumAmmParser;

impl RaydiumAmmParser {
    pub fn new() -> Self { Self }

    fn parse_protobuf(&self, raw_bytes: &[u8], block_time: i64) -> Result<Option<Vec<TransactionEvent>>> {
        let update = SubscribeUpdate::decode(raw_bytes)?;
        let mut events: Vec<TransactionEvent> = Vec::new();

        if let Some(yellowstone_grpc_proto::geyser::subscribe_update::UpdateOneof::Transaction(tx_info)) = update.update_oneof {
            let slot = tx_info.slot;
            let tx_details = tx_info.transaction.unwrap();
            let signature = bs58::encode(&tx_details.signature).into_string();
            let message = tx_details.transaction.unwrap().message.unwrap();
            let meta = tx_details.meta.unwrap();

            let all_accounts = VixenUtils::extract_accounts_from_grpc(
                &message.account_keys,
                &meta.loaded_writable_addresses,
                &meta.loaded_readonly_addresses,
            );
            let account_keys: Vec<String> = all_accounts.iter().map(|k| k.to_string()).collect();

            let raydium_idx = account_keys.iter().position(|k| k == domain::RAYDIUM_V4_PROGRAM_ID);

            if let Some(pgm_idx) = raydium_idx {
                for (ix_idx, ix) in message.instructions.iter().enumerate() {
                    if ix.program_id_index as usize != pgm_idx { continue; }
                    if ix.data.first().copied() != Some(9) { continue; }
                    if ix.accounts.len() <= 17 { continue; }

                    let args = RaydiumSwapInstruction::try_from_slice(&ix.data[1..17])
                        .map_err(|e| anyhow::anyhow!("Raydium decode error: {:?}", e))?;

                    let signer_idx = ix.accounts[17] as usize;
                    let amm_idx    = ix.accounts[1]  as usize;
                    let src_idx    = ix.accounts[15] as usize;
                    let dst_idx    = ix.accounts[16] as usize;

                    let amount_received = Self::find_cpi_amount_grpc(ix_idx, dst_idx, &meta.inner_instructions).unwrap_or(0);
                    let mint_source      = Self::resolve_mint_grpc(src_idx, &meta.pre_token_balances, &meta.post_token_balances);
                    let mint_destination = Self::resolve_mint_grpc(dst_idx, &meta.pre_token_balances, &meta.post_token_balances);

                    events.push(TransactionEvent::RaydiumSwap(RaydiumSwapEvent {
                        amm_pool: account_keys[amm_idx].clone(),
                        signer: account_keys[signer_idx].clone(),
                        amount_in: args.amount_in,
                        min_amount_out: args.min_amount_out,
                        amount_received,
                        mint_source,
                        mint_destination,
                        slot,
                        block_time,
                        signature: signature.clone(),
                    }));
                }
            }
        }

        Ok(Some(events))
    }

    fn parse_rpc(
        &self,
        tx: VersionedTransaction,
        meta: UiTransactionStatusMeta,
        slot: u64,
        signature: &str,
    ) -> Result<Option<Vec<TransactionEvent>>> {
        let mut events: Vec<TransactionEvent> = Vec::new();
        let message = &tx.message;

        let mut all_keys: Vec<String> = message.static_account_keys().iter().map(|k| k.to_string()).collect();
        if let OptionSerializer::Some(loaded) = &meta.loaded_addresses {
            for a in &loaded.writable { all_keys.push(a.clone()); }
            for a in &loaded.readonly { all_keys.push(a.clone()); }
        }

        let raydium_idx = all_keys.iter().position(|k| k == domain::RAYDIUM_V4_PROGRAM_ID);
        if let Some(pgm_idx) = raydium_idx {
            let pgm_idx = pgm_idx as u8;

            for (ix_idx, ix) in message.instructions().iter().enumerate() {
                if ix.program_id_index != pgm_idx { continue; }
                if ix.data.len() < 17 || ix.data[0] != 9 { continue; }

                let args = RaydiumSwapInstruction::try_from_slice(&ix.data[1..17])
                    .map_err(|e| anyhow::anyhow!("Raydium decode error: {:?}", e))?;

                let amm_idx    = *ix.accounts.get(1).ok_or_else(|| anyhow::anyhow!("missing amm_pool"))? as usize;
                let src_idx    = *ix.accounts.get(15).ok_or_else(|| anyhow::anyhow!("missing src"))? as usize;
                let dst_idx    = *ix.accounts.get(16).ok_or_else(|| anyhow::anyhow!("missing dst"))? as usize;
                let signer_idx = *ix.accounts.get(17).ok_or_else(|| anyhow::anyhow!("missing signer"))? as usize;

                let amount_received = Self::find_cpi_amount_rpc(ix_idx, dst_idx, &meta.inner_instructions).unwrap_or(0);

                let pre  = meta.pre_token_balances.as_deref().unwrap_or(&[]);
                let post = meta.post_token_balances.as_deref().unwrap_or(&[]);
                let mint_source      = Self::resolve_mint_rpc(src_idx, pre, post);
                let mint_destination = Self::resolve_mint_rpc(dst_idx, pre, post);

                events.push(TransactionEvent::RaydiumSwap(RaydiumSwapEvent {
                    amm_pool: all_keys[amm_idx].clone(),
                    signer: all_keys[signer_idx].clone(),
                    amount_in: args.amount_in,
                    min_amount_out: args.min_amount_out,
                    amount_received,
                    mint_source,
                    mint_destination,
                    slot,
                    block_time: 0,
                    signature: signature.to_string(),
                }));
            }
        }

        Ok(Some(events))
    }

    // ─── CPI helpers ────────────────────────────────────────────────────────────

    fn find_cpi_amount_grpc(
        parent_idx: usize,
        target_dst: usize,
        inner_ixs: &[yellowstone_grpc_proto::prelude::InnerInstructions],
    ) -> Option<u64> {
        let group = inner_ixs.iter().find(|g| g.index == parent_idx as u32)?;

        for ix in &group.instructions {
            let (amount, dst) = if ix.data.first() == Some(&3) && ix.data.len() >= 9 {
                let mut b = [0u8; 8];
                b.copy_from_slice(&ix.data[1..9]);
                (u64::from_le_bytes(b), ix.accounts.get(1)?)
            } else if ix.data.first() == Some(&12) && ix.data.len() >= 9 {
                let mut b = [0u8; 8];
                b.copy_from_slice(&ix.data[1..9]);
                (u64::from_le_bytes(b), ix.accounts.get(2)?)
            } else {
                continue;
            };

            if *dst == target_dst as u8 { return Some(amount); }
        }
        None
    }

    fn find_cpi_amount_rpc(
        parent_idx: usize,
        target_dst: usize,
        inner_ixs: &OptionSerializer<Vec<UiInnerInstructions>>,
    ) -> Option<u64> {
        if let OptionSerializer::Some(groups) = inner_ixs {
            let group = groups.iter().find(|g| g.index == parent_idx as u8)?;
            for ix in &group.instructions {
                if let UiInstruction::Compiled(c) = ix {
                    let raw = bs58::decode(&c.data).into_vec().ok()?;
                    let (amount, dst) = if raw.first() == Some(&3) && raw.len() >= 9 {
                        let mut b = [0u8; 8];
                        b.copy_from_slice(&raw[1..9]);
                        (u64::from_le_bytes(b), c.accounts.get(1)?)
                    } else if raw.first() == Some(&12) && raw.len() >= 9 {
                        let mut b = [0u8; 8];
                        b.copy_from_slice(&raw[1..9]);
                        (u64::from_le_bytes(b), c.accounts.get(2)?)
                    } else {
                        continue;
                    };
                    if *dst as usize == target_dst { return Some(amount); }
                }
            }
        }
        None
    }

    fn resolve_mint_grpc(idx: usize, pre: &[TokenBalance], post: &[TokenBalance]) -> String {
        pre.iter()
            .find(|b| b.account_index == idx as u32)
            .or_else(|| post.iter().find(|b| b.account_index == idx as u32))
            .map(|b| b.mint.clone())
            .unwrap_or_else(|| "unknown".to_string())
    }

    fn resolve_mint_rpc(idx: usize, pre: &[UiTransactionTokenBalance], post: &[UiTransactionTokenBalance]) -> String {
        pre.iter()
            .find(|b| b.account_index == idx as u8)
            .or_else(|| post.iter().find(|b| b.account_index == idx as u8))
            .map(|b| b.mint.clone())
            .unwrap_or_else(|| "unknown".to_string())
    }
}

impl TransactionParser for RaydiumAmmParser {
    fn name(&self) -> &str { "raydium_amm" }

    fn parse(&self, txn: SolanaTransaction) -> Result<Option<Vec<TransactionEvent>>> {
        match txn.data {
            TxData::Grpc(bytes) => self.parse_protobuf(&bytes, txn.block_time),
            TxData::Rpc { tx, meta } => self.parse_rpc(tx, meta, txn.slot, &txn.signature),
        }
    }
}
