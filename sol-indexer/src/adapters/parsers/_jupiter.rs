// NOT USED
use borsh::{BorshDeserialize, BorshSerialize};
use solana_client::rpc_response::{OptionSerializer, UiTransactionStatusMeta};
use solana_sdk::transaction::VersionedTransaction;

use crate::domain::TransactionEvent;


#[derive(BorshDeserialize,BorshSerialize,Debug)]
pub struct JupiterRouteArgs{
    pub route_plan:Vec<RoutePlanStep>,
    pub in_amount:u64,
    pub quoted_out_amount: u64,
    pub slippage_bps: u16,
    pub platform_fee_bps: u8,
}

#[derive(BorshDeserialize,BorshSerialize,Debug)]
pub struct RoutePlanStep{
    // Enum
    pub swap: SwapLeg,
    pub percent: u8,
    pub input_index: u8,
    pub output_index: u8,
}

#[derive(BorshSerialize,BorshDeserialize,Debug)]
pub enum SwapLeg{
    // This is a very long enum, as we just want the amount so we don't need it
    Raydium,
    Meteora,
}

// Parsing the RoutePlan, manually will be hard, cuz of huge Enum
// So instead of parsing the input data, we will parse the transfer event
// The Layout of `Route` instruction:
// [8 bytes]: Discriminator
// [4 bytes]: Vec length (u32)
// [N bytes]: Vec data (RoutePlan)
// [8 bytes]: in_amount  
// [8 bytes]: quoted_out_amount

pub struct JupiterParser;

impl JupiterParser {
    const JUP_PROGRAM_ID: &'static str = "JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4";

    // Discriminators derived from IDL
    // swap ix with exact input amount
    const DISC_ROUTE: [u8; 8] = [229, 23, 203, 151, 122, 227, 173, 42];
    // uses shared pgm account, uses Lookup table
    const DISC_SHARED_ROUTE: [u8; 8] = [193, 32, 155, 51, 65, 214, 156, 129];
    // swap ix with exact output amount
    const DISC_EXACT_OUT: [u8; 8] = [208, 51, 239, 151, 123, 43, 237, 92];
    const DISC_SHARED_EXACT_OUT: [u8; 8] = [176, 209, 105, 168, 154, 125, 69, 62];

    fn parse_instruction_data(data:&[u8])->Option<(u64,u64)>{
        if data.len() < 8 { return None; }

        let (disc,rest) = data.split_at(8);

        if disc != Self::DISC_ROUTE && disc != Self::DISC_SHARED_ROUTE && disc != Self::DISC_EXACT_OUT && disc != Self::DISC_SHARED_EXACT_OUT{
            return None;
        }

        // Manual Offset calculation is risky as we don't know the size of the vector 
        // so its better we read the Transfer Inner Instrcution

        None

    }

    pub fn parse_rpc(&self, tx: &VersionedTransaction, meta: &UiTransactionStatusMeta, slot: u64, sig: &str) -> Option<Vec<TransactionEvent>> {
        let mut events = Vec::new();
        let msg = &tx.message;
        
        let mut all_accounts: Vec<String> = msg.static_account_keys().iter().map(|k| k.to_string()).collect();

        let loaded_addresses = meta.loaded_addresses.clone();

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
        };

        let jup_idx = all_accounts.iter().position(|a| a == Self::JUP_PROGRAM_ID)? as u8;

        for (ix_idx, ix) in msg.instructions().iter().enumerate() {
            if ix.program_id_index != jup_idx { continue; }

            if ix.data.len() < 8 { continue; }
            let disc = &ix.data[0..8];
            
            let is_swap = disc == Self::DISC_ROUTE || disc == Self::DISC_SHARED_ROUTE;
            if is_swap {
                // we have to read the inner_ix
                // The "In Amount" is the transfer from User (signer) to an intermediate account.
                // The "Out Amount" is the transfer from intermediate to User.
                
                let user_account = &all_accounts[*ix.accounts.get(1)? as usize];

                events.push(TransactionEvent::RaydiumSwap(
                    crate::domain::RaydiumSwapEvent {
                        amm_pool: "Jupiter Aggregator".to_string(), 
                        signer: user_account.clone(),
                        amount_in: 0, 
                        min_amount_out: 0,
                        amount_received: 0, 
                        mint_source: "TODO".to_string(),
                        mint_destination: "TODO".to_string(),
                        slot,
                        signature: sig.to_string(),
                    }
                ));
            }
        }
        Some(events)
    }
}

