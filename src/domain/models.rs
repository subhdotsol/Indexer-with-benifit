use serde::{Deserialize, Serialize};
use solana_sdk::transaction::VersionedTransaction;
use solana_transaction_status::UiTransactionStatusMeta;

#[derive(Debug, Clone)]
pub enum ChainEvent {
    Transaction(SolanaTransaction),
    BlockMeta {
        slot: u64,
        block_hash: String,
        parent_block_hash: String,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum TransactionEvent {
    TokenTransfer(TokenTransfer),
    RaydiumSwap(RaydiumSwapEvent),
    JupiterSwap(JupiterSwapEvent),
    PumpFunSwap(PumpFunSwapEvent),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TokenTransfer {
    pub from: String,
    pub to: String,
    pub slot: u64,
    pub amount: u64,
    pub signature: String,
    pub mint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaydiumSwapEvent {
    pub amm_pool: String,
    pub signer: String,
    pub amount_in: u64,
    pub min_amount_out: u64,
    pub amount_received: u64,
    pub mint_source: String,
    pub mint_destination: String,
    pub slot: u64,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JupiterSwapEvent {
    pub signature: String,
    pub slot: u64,
    pub signer: String,
    pub amm_pool: String,
    pub mint_in: String,
    pub mint_out: String,
    pub amount_in: u64,
    pub amount_out: u64,
    pub slippage_bps: u16,
    pub platform_fee_bps: u8,
    pub route_plan: Vec<RouteStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteStep {
    pub swap_label: String,
    pub percent: u8,
    pub input_index: u8,
    pub output_index: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PumpFunSwapEvent {
    pub signature: String,
    pub slot: u64,
    pub signer: String,
    pub mint: String,
    pub is_buy: bool,
    pub sol_amount: u64,
    pub token_amount: u64,
    pub bonding_curve: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolanaTransaction {
    pub signature: String,
    pub success: bool,
    pub slot: u64,
    pub data: TxData,
    pub block_time: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum TxData {
    Grpc(Vec<u8>), // From gRPC
    Rpc {
        tx: VersionedTransaction,
        meta: UiTransactionStatusMeta,
    }, // From RPC
    Raw(Vec<u8>),
}

pub const RAYDIUM_V4_PROGRAM_ID: &'static str = "675k1P952h926S9fXN1v8B2YyW6QiX48zSt5q35XV55";
pub const JUP_PROGRAM_ID: &'static str = "JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4";
pub const PUMP_FUN_PROGRAM_ID: &'static str = "6EF8rrecthR5DkZ9G5zE4v5Yp9h1hR143r166h7qBPrL"; // Pump.fun Program ID
