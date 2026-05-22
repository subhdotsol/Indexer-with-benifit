use std::sync::Arc;

use solana_account_decoder_client_types::token::UiTokenAmount;
use solana_transaction_status::{UiInnerInstructions, UiInstruction, UiParsedInstruction, UiTransactionTokenBalance, option_serializer::OptionSerializer};
use solana_sdk::pubkey::Pubkey;
use yellowstone_grpc_proto::prelude::InnerInstruction;
use yellowstone_vixen_core::instruction::{InstructionShared, InstructionUpdate, Path};

pub struct VixenUtils;

impl VixenUtils {
    /// Convert gRPC inner instructions into Vixen InstructionUpdates
    pub fn convert_protobuf_inner_instruction(
        inner_ixs: &Vec<InnerInstruction>,
        all_accounts: &Vec<Pubkey>,
        shared: Arc<InstructionShared>,
    ) -> Option<Vec<InstructionUpdate>> {
        let updates = inner_ixs
            .iter()
            .filter_map(|ix| {
                if ix.program_id_index as usize >= all_accounts.len() {
                    return None;
                }
                let pgm = all_accounts[ix.program_id_index as usize];
                let accs = all_accounts
                    .iter()
                    .map(|a| yellowstone_vixen_parser::Pubkey::from(a.to_bytes()))
                    .collect();

                Some(InstructionUpdate {
                    data: ix.data.clone(),
                    shared: shared.clone(),
                    inner: vec![],
                    accounts: accs,
                    program: yellowstone_vixen_parser::Pubkey::from(pgm.to_bytes()),
                    path: Path::from(vec![]),
                    log_range: 0..0,
                })
            })
            .collect();

        Some(updates)
    }

    /// Convert a single RPC UiInstruction into a Vixen InstructionUpdate
    pub fn convert_rpc_inner_instruction(
        ui_ix: &UiInstruction,
        all_accounts: &[Pubkey],
        shared: Arc<InstructionShared>,
    ) -> Option<InstructionUpdate> {
        match ui_ix {
            UiInstruction::Compiled(c) => {
                let pgm = all_accounts.get(c.program_id_index as usize)?;
                let accs = c
                    .accounts
                    .iter()
                    .filter_map(|&i| all_accounts.get(i as usize))
                    .map(|a| yellowstone_vixen_parser::Pubkey::from(a.to_bytes()))
                    .collect();
                let data = bs58::decode(&c.data).into_vec().ok()?;

                Some(InstructionUpdate {
                    program: yellowstone_vixen_parser::Pubkey::from(pgm.to_bytes()),
                    accounts: accs,
                    data,
                    shared,
                    inner: vec![],
                    path: Path::from(vec![]),
                    log_range: 0..0,
                })
            }
            UiInstruction::Parsed(parsed) => match parsed {
                UiParsedInstruction::PartiallyDecoded(pd) => {
                    let pgm = pd.program_id.parse::<Pubkey>().ok()?;
                    let accs = pd
                        .accounts
                        .iter()
                        .filter_map(|a| a.parse::<Pubkey>().ok())
                        .map(|a| yellowstone_vixen_parser::Pubkey::from(a.to_bytes()))
                        .collect();
                    let data = bs58::decode(&pd.data).into_vec().ok()?;

                    Some(InstructionUpdate {
                        program: yellowstone_vixen_parser::Pubkey::from(pgm.to_bytes()),
                        accounts: accs,
                        data,
                        shared,
                        inner: vec![],
                        path: Path::from(vec![]),
                        log_range: 0..0,
                    })
                }
                UiParsedInstruction::Parsed(_) => {
                    tracing::warn!("Skipping fully-parsed JSON inner ix (no raw bytes available)");
                    None
                }
            },
        }
    }

    /// Build a Vixen InstructionUpdate from an RPC instruction + optional inner instructions
    pub fn to_vixen_update_rpc(
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

        let inner = if let Some(container) = inner_ixs {
            container
                .instructions
                .iter()
                .filter_map(|ix| Self::convert_rpc_inner_instruction(ix, accounts, shared.clone()))
                .collect()
        } else {
            vec![]
        };

        InstructionUpdate {
            program: yellowstone_vixen_parser::Pubkey::from(program_id.to_bytes()),
            accounts: accounts.iter().map(|a| yellowstone_vixen_parser::Pubkey::from(a.to_bytes())).collect(),
            data: data.to_vec(),
            shared,
            inner,
            path: Path::from(vec![]),
            log_range: 0..0,
        }
    }

    /// Convert gRPC token balances to the RPC-style OptionSerializer format
    pub fn convert_token_balances_grpc(
        balances: &[yellowstone_grpc_proto::prelude::TokenBalance],
    ) -> OptionSerializer<Vec<UiTransactionTokenBalance>> {
        OptionSerializer::Some(
            balances
                .iter()
                .map(|b| UiTransactionTokenBalance {
                    account_index: b.account_index as u8,
                    mint: b.mint.clone(),
                    ui_token_amount: UiTokenAmount {
                        ui_amount: b.ui_token_amount.as_ref().and_then(|a| Some(a.ui_amount)),
                        decimals: b.ui_token_amount.as_ref().map(|a| a.decimals as u8).unwrap_or(0),
                        amount: b.ui_token_amount.as_ref().map(|a| a.amount.clone()).unwrap_or_default(),
                        ui_amount_string: b.ui_token_amount.as_ref().map(|a| a.ui_amount_string.clone()).unwrap_or_default(),
                    },
                    owner: OptionSerializer::Some(b.owner.clone()),
                    program_id: OptionSerializer::Some(b.program_id.clone()),
                })
                .collect(),
        )
    }

    /// Look up the mint for a token account pubkey using pre-token-balance data
    pub fn get_mint(
        account: &yellowstone_vixen_parser::Pubkey,
        all_accounts: &[Pubkey],
        balances: &OptionSerializer<Vec<UiTransactionTokenBalance>>,
    ) -> String {
        let target = Pubkey::new_from_array(account.0);
        let idx = all_accounts.iter().position(|k| *k == target);

        if let (Some(idx), OptionSerializer::Some(bals)) = (idx, balances) {
            if let Some(b) = bals.iter().find(|b| b.account_index == idx as u8) {
                return b.mint.clone();
            }
        }

        // Wrapped SOL is the most common fallback for unlisted token accounts
        crate::domain::WSOL_MINT.to_string()
    }

    /// Reconstruct the full account list from gRPC message keys + loaded addresses
    pub fn extract_accounts_from_grpc(
        static_keys: &[Vec<u8>],
        loaded_writable: &[Vec<u8>],
        loaded_readonly: &[Vec<u8>],
    ) -> Vec<Pubkey> {
        let mut accounts = Vec::with_capacity(static_keys.len() + loaded_writable.len() + loaded_readonly.len());

        for bytes in static_keys.iter().chain(loaded_writable).chain(loaded_readonly) {
            if let Ok(arr) = bytes.clone().try_into() {
                accounts.push(Pubkey::new_from_array(arr));
            }
        }

        accounts
    }
}
