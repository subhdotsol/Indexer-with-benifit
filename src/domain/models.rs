use serde::{Deserialize, Serialize};

#[derive(Debug,Clone,Serialize,Deserialize)]
pub struct SolanaTransaction{
    pub signature:String,
    pub success:bool,
    pub raw_bytes: Vec<u8>, // will parse it later
    // It is for ordering & finality is also expressed in terms of slot
    pub slot: u64,
}
