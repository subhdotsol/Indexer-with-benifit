use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TokenTransfer {
    pub from: String,
    pub to: String,
    pub slot: u64,
    pub amount: u64,
    pub signature: String,
    pub mint: Option<String>,
}
