use serde::{Deserialize, Serialize};
use solana_client::rpc_response::UiTransactionStatusMeta;
use solana_sdk::transaction::VersionedTransaction;

use crate::domain::TokenTransfer;

#[derive(Debug,Clone)]
pub enum ChainEvent{
    Transaction(SolanaTransaction),
    BlockMeta{
        slot: u64,
        block_hash:String,
        parent_block_hash:String,
    }
}

#[derive(Debug,Serialize,Deserialize,Clone)]
pub enum TransactionEvent{
    TokenTransfer(TokenTransfer),
    RaydiumSwap(RaydiumSwapEvent),
    JupiterSwap(JupiterSwapEvent),
    PumpFunTrade(PumpFunTrade),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SwapEvent {
    Raydium(RaydiumSwapEvent),
    Jupiter(JupiterSwapEvent),
    PumpFun(PumpFunTrade),
}

impl SwapEvent {
    pub fn amount_in(&self) -> u64 {
        match self {
            Self::Raydium(swap) => swap.amount_in,
            Self::Jupiter(swap) => swap.amount_in,
            Self::PumpFun(trade) => trade.sol_amount, // Use SOL amount as threshold
        }
    }

    pub fn signature(&self) -> &str {
        match self {
            Self::Raydium(swap) => &swap.signature,
            Self::Jupiter(swap) => &swap.signature,
            Self::PumpFun(trade) => &trade.signature,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PumpFunTrade {
    pub signature: String,
    pub slot: u64,
    pub mint: String,
    pub is_buy: bool,
    pub user: String,
    pub timestamp: i64,
    pub token_amount: u64,
    pub sol_amount: u64,
    pub block_time: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JupiterSwapEvent {
    pub signature: String,
    pub slot: u64,
    pub block_time: i64,
    pub signer: String,
    pub amm_pool: String,

    pub mint_in: String,
    pub mint_out: String,
    pub amount_in: u64,
    pub amount_out: u64,

    pub slippage_bps: u16,
    pub platform_fee_bps: u8,

    // Detailed Step-by-Step Route
    pub route_plan: Vec<RouteStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteStep {
    pub swap_label: String, // e.g., "Raydium", "Orca"
    pub percent: u8, // percentage of the money, can be used in future to see dominance
    pub input_index: u8,
    pub output_index: u8,
}

#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct RaydiumSwapEvent{
    pub amm_pool:String, // SOL-USDC
    pub signer: String,
    pub amount_in: u64, // the amount user sends
    pub min_amount_out: u64, // min amount of token user is willing to accept
    pub amount_received: u64,
    pub mint_source: String,
    pub mint_destination: String,
    pub slot: u64,
    pub block_time: i64,
    pub signature: String,
}

#[derive(Debug,Clone,Serialize,Deserialize)]
pub struct SolanaTransaction{
    pub signature:String,
    pub success:bool,
    pub data: TxData, // will parse it later
    // It is for ordering & finality is also expressed in terms of slot
    pub slot: u64,
    // for analytics/history
    pub block_time:i64,
}

#[derive(Debug,Clone,Deserialize,Serialize)]
pub enum TxData{
    Grpc(Vec<u8>), // From gRPC
    Rpc{
        tx: VersionedTransaction,
        meta: UiTransactionStatusMeta,
    }, // From RPC
}

#[derive(Debug,Clone)]
pub struct IndexerState{
    pub last_slot:u64,
    pub last_block_hash:String
}
