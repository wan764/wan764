//! Raydium AMM V4 (legacy) + CPMM pool detection and swap building.

use crate::{
    constants::*,
    dex::DexHandler,
    error::{BotError, Result},
    types::{Dex, PoolInfo},
    utils::{ata_address, now_ms},
};
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};

// ─────────────────────────────────────────────────────────────
// Raydium AMM V4
// ─────────────────────────────────────────────────────────────

pub struct RaydiumAmm;

impl DexHandler for RaydiumAmm {
    fn pool_init_log(&self) -> &'static str {
        // Raydium AMM V4 is NOT Anchor — it emits `msg!("initialize")` → lowercase
        "initialize"
    }

    fn parse_pool(
        &self,
        account_keys: &[Pubkey],
        ix_accounts: &[u8],
        signature: &str,
    ) -> Result<PoolInfo> {
        let get = |idx: usize| -> Result<Pubkey> {
            let ai = *ix_accounts
                .get(idx)
                .ok_or_else(|| BotError::Parse(format!("AMM V4 ix_accounts[{idx}] missing")))? as usize;
            account_keys
                .get(ai)
                .copied()
                .ok_or_else(|| BotError::Parse(format!("AMM V4 account_keys[{ai}] missing")))
        };
        Ok(PoolInfo {
            dex: Dex::RaydiumAmm,
            pool: get(AMM_V4_INIT_POOL_IDX)?,
            base_mint: get(AMM_V4_INIT_COIN_MINT)?,
            quote_mint: get(AMM_V4_INIT_PC_MINT)?,
            base_decimals: 0,
            quote_decimals: 9,
            extra: serde_json::Value::Null,
            detected_at_ms: now_ms(),
            tx_signature: signature.to_string(),
        })
    }

    /// Raydium AMM V4 `swapBaseIn` instruction (type byte = 9).
    fn build_swap_ix(
        &self,
        pool: &PoolInfo,
        user: &Pubkey,
        amount_in: u64,
        min_out: u64,
        quote_to_base: bool,
    ) -> Result<Vec<Instruction>> {
        let (src_mint, dst_mint) = if quote_to_base {
            (pool.quote_mint, pool.base_mint)
        } else {
            (pool.base_mint, pool.quote_mint)
        };

        let (amm_authority, _) =
            Pubkey::find_program_address(&[b"amm authority"], &RAYDIUM_AMM_V4);

        let user_src = ata_address(user, &src_mint);
        let user_dst = ata_address(user, &dst_mint);

        let pk = |key: &str| -> Pubkey {
            pool.extra[key]
                .as_str()
                .and_then(|s| s.parse().ok())
                .unwrap_or_default()
        };

        let mut data = Vec::with_capacity(17);
        data.push(AMM_V4_SWAP_BASE_IN);
        data.extend_from_slice(&amount_in.to_le_bytes());
        data.extend_from_slice(&min_out.to_le_bytes());

        let accounts = vec![
            AccountMeta::new_readonly(SPL_TOKEN_PROGRAM, false),
            AccountMeta::new(pool.pool, false),
            AccountMeta::new_readonly(amm_authority, false),
            AccountMeta::new(pk("open_orders"), false),
            AccountMeta::new(pk("coin_vault"), false),
            AccountMeta::new(pk("pc_vault"), false),
            AccountMeta::new_readonly(OPENBOOK_PROGRAM, false),
            AccountMeta::new(pk("market"), false),
            AccountMeta::new(pk("market_bids"), false),
            AccountMeta::new(pk("market_asks"), false),
            AccountMeta::new(pk("market_event_queue"), false),
            AccountMeta::new(pk("market_coin_vault"), false),
            AccountMeta::new(pk("market_pc_vault"), false),
            AccountMeta::new_readonly(pk("market_vault_signer"), false),
            AccountMeta::new(user_src, false),
            AccountMeta::new(user_dst, false),
            AccountMeta::new_readonly(*user, true),
        ];

        Ok(vec![Instruction {
            program_id: RAYDIUM_AMM_V4,
            accounts,
            data,
        }])
    }
}

// ─────────────────────────────────────────────────────────────
// Raydium CPMM
// ─────────────────────────────────────────────────────────────

pub struct RaydiumCpmm;

impl DexHandler for RaydiumCpmm {
    fn pool_init_log(&self) -> &'static str {
        // Anchor program emits "Program log: Instruction: Initialize"
        "Instruction: Initialize"
    }

    fn parse_pool(
        &self,
        account_keys: &[Pubkey],
        ix_accounts: &[u8],
        signature: &str,
    ) -> Result<PoolInfo> {
        let get = |idx: usize| -> Result<Pubkey> {
            let ai = *ix_accounts
                .get(idx)
                .ok_or_else(|| BotError::Parse(format!("CPMM ix_accounts[{idx}] missing")))? as usize;
            account_keys
                .get(ai)
                .copied()
                .ok_or_else(|| BotError::Parse(format!("CPMM account_keys[{ai}] missing")))
        };
        Ok(PoolInfo {
            dex: Dex::RaydiumCpmm,
            pool: get(CPMM_INIT_POOL_IDX)?,
            base_mint: get(CPMM_INIT_MINT0_IDX)?,
            quote_mint: get(CPMM_INIT_MINT1_IDX)?,
            base_decimals: 0,
            quote_decimals: 9,
            extra: serde_json::Value::Null,
            detected_at_ms: now_ms(),
            tx_signature: signature.to_string(),
        })
    }

    /// Raydium CPMM `swap_base_input` (Anchor instruction).
    fn build_swap_ix(
        &self,
        pool: &PoolInfo,
        user: &Pubkey,
        amount_in: u64,
        min_out: u64,
        quote_to_base: bool,
    ) -> Result<Vec<Instruction>> {
        let (in_mint, out_mint) = if quote_to_base {
            (pool.quote_mint, pool.base_mint)
        } else {
            (pool.base_mint, pool.quote_mint)
        };

        let (authority, _) =
            Pubkey::find_program_address(&[b"vault_and_lp_mint_auth_seed"], &RAYDIUM_CPMM);

        let pk = |key: &str| -> Pubkey {
            pool.extra[key]
                .as_str()
                .and_then(|s| s.parse().ok())
                .unwrap_or_default()
        };

        let user_in = ata_address(user, &in_mint);
        let user_out = ata_address(user, &out_mint);

        let disc = anchor_discriminator("swap_base_input");
        let mut data = Vec::with_capacity(24);
        data.extend_from_slice(&disc);
        data.extend_from_slice(&amount_in.to_le_bytes());
        data.extend_from_slice(&min_out.to_le_bytes());

        let accounts = vec![
            AccountMeta::new(*user, true),
            AccountMeta::new_readonly(authority, false),
            AccountMeta::new_readonly(pk("amm_config"), false),
            AccountMeta::new(pool.pool, false),
            AccountMeta::new(user_in, false),
            AccountMeta::new(user_out, false),
            AccountMeta::new(pk("input_vault"), false),
            AccountMeta::new(pk("output_vault"), false),
            AccountMeta::new_readonly(SPL_TOKEN_PROGRAM, false),
            AccountMeta::new_readonly(SPL_TOKEN_PROGRAM, false),
            AccountMeta::new_readonly(in_mint, false),
            AccountMeta::new_readonly(out_mint, false),
            AccountMeta::new(pk("observation"), false),
        ];

        Ok(vec![Instruction {
            program_id: RAYDIUM_CPMM,
            accounts,
            data,
        }])
    }
}
