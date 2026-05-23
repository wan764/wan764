use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    system_program,
};
use std::time::{SystemTime, UNIX_EPOCH};

// ── Well-known program IDs (without importing spl crates) ─────
const SPL_TOKEN_PROGRAM: Pubkey =
    solana_sdk::pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
const SPL_TOKEN_2022_PROGRAM: Pubkey =
    solana_sdk::pubkey!("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");
const ASSOCIATED_TOKEN_PROGRAM: Pubkey =
    solana_sdk::pubkey!("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJe1bD");

// ── ATA derivation ────────────────────────────────────────────

pub fn ata_address(wallet: &Pubkey, mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[wallet.as_ref(), SPL_TOKEN_PROGRAM.as_ref(), mint.as_ref()],
        &ASSOCIATED_TOKEN_PROGRAM,
    )
    .0
}

pub fn ata_address_2022(wallet: &Pubkey, mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[wallet.as_ref(), SPL_TOKEN_2022_PROGRAM.as_ref(), mint.as_ref()],
        &ASSOCIATED_TOKEN_PROGRAM,
    )
    .0
}

/// Create-idempotent instruction for the associated token account program.
pub fn create_ata_idempotent_ix(payer: &Pubkey, wallet: &Pubkey, mint: &Pubkey) -> Instruction {
    let ata = ata_address(wallet, mint);
    Instruction {
        program_id: ASSOCIATED_TOKEN_PROGRAM,
        accounts: vec![
            AccountMeta::new(*payer, true),
            AccountMeta::new(ata, false),
            AccountMeta::new_readonly(*wallet, false),
            AccountMeta::new_readonly(*mint, false),
            AccountMeta::new_readonly(system_program::id(), false),
            AccountMeta::new_readonly(SPL_TOKEN_PROGRAM, false),
        ],
        data: vec![1u8], // CreateIdempotent opcode
    }
}

// ── SPL Mint parsing (82-byte layout, no spl-token crate) ─────

#[derive(Debug)]
pub struct MintInfo {
    pub mint_authority: Option<Pubkey>,
    pub supply: u64,
    pub decimals: u8,
    pub is_initialized: bool,
    pub freeze_authority: Option<Pubkey>,
}

/// Parse a raw SPL Mint account (82 bytes).
pub fn parse_mint(data: &[u8]) -> Option<MintInfo> {
    if data.len() < 82 {
        return None;
    }
    let coption_pubkey = |tag_off: usize, key_off: usize| -> Option<Pubkey> {
        if u32::from_le_bytes(data[tag_off..tag_off + 4].try_into().ok()?) == 1 {
            Pubkey::try_from(&data[key_off..key_off + 32]).ok()
        } else {
            None
        }
    };
    let mint_authority = coption_pubkey(0, 4);
    let supply = u64::from_le_bytes(data[36..44].try_into().ok()?);
    let decimals = data[44];
    let is_initialized = data[45] != 0;
    let freeze_authority = coption_pubkey(46, 50);

    Some(MintInfo {
        mint_authority,
        supply,
        decimals,
        is_initialized,
        freeze_authority,
    })
}

// ── Time helpers ──────────────────────────────────────────────

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub fn lamports_to_sol(lamports: u64) -> f64 {
    lamports as f64 / 1_000_000_000.0
}

pub fn sol_to_lamports(sol: f64) -> u64 {
    (sol * 1_000_000_000.0) as u64
}

pub fn apply_slippage_down(amount: u64, slippage_bps: u64) -> u64 {
    amount.saturating_sub(amount * slippage_bps / 10_000)
}

pub fn short_key(key: &Pubkey) -> String {
    let s = key.to_string();
    format!("{}…{}", &s[..4], &s[s.len() - 4..])
}
