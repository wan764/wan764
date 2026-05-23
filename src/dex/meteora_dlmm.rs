//! Meteora DLMM pool detection and swap building.

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

pub struct MeteoraDlmm;

impl MeteoraDlmm {
    fn bin_array_pda(lb_pair: &Pubkey, index: i64) -> Pubkey {
        Pubkey::find_program_address(
            &[b"bin_array", lb_pair.as_ref(), &index.to_le_bytes()],
            &METEORA_DLMM,
        )
        .0
    }

    fn event_authority() -> Pubkey {
        Pubkey::find_program_address(&[b"__event_authority"], &METEORA_DLMM).0
    }

    fn bin_array_indices(active_bin: i32, swap_for_y: bool) -> [i64; 2] {
        const BINS_PER_ARRAY: i32 = 70;
        let base = active_bin.div_euclid(BINS_PER_ARRAY) as i64;
        if swap_for_y {
            [base - 1, base]
        } else {
            [base, base + 1]
        }
    }
}

impl DexHandler for MeteoraDlmm {
    fn pool_init_log(&self) -> &'static str {
        "InitializeLbPair"
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
                .ok_or_else(|| BotError::Parse(format!("DLMM ix_accounts[{idx}] missing")))? as usize;
            account_keys
                .get(ai)
                .copied()
                .ok_or_else(|| BotError::Parse(format!("DLMM account_keys[{ai}] missing")))
        };
        Ok(PoolInfo {
            dex: Dex::MeteoraDlmm,
            pool: get(DLMM_INIT_POOL_IDX)?,
            base_mint: get(DLMM_INIT_MINT_X_IDX)?,
            quote_mint: get(DLMM_INIT_MINT_Y_IDX)?,
            base_decimals: 0,
            quote_decimals: 9,
            extra: serde_json::Value::Null,
            detected_at_ms: now_ms(),
            tx_signature: signature.to_string(),
        })
    }

    fn build_swap_ix(
        &self,
        pool: &PoolInfo,
        user: &Pubkey,
        amount_in: u64,
        min_out: u64,
        quote_to_base: bool,
    ) -> Result<Vec<Instruction>> {
        let swap_for_y = quote_to_base;
        let (in_mint, out_mint) = if quote_to_base {
            (pool.quote_mint, pool.base_mint)
        } else {
            (pool.base_mint, pool.quote_mint)
        };

        let active_bin = pool.extra["active_bin"].as_i64().unwrap_or(0) as i32;
        let indices = Self::bin_array_indices(active_bin, swap_for_y);
        let bin_array_0 = Self::bin_array_pda(&pool.pool, indices[0]);
        let bin_array_1 = Self::bin_array_pda(&pool.pool, indices[1]);
        let event_authority = Self::event_authority();

        let pk = |key: &str| -> Pubkey {
            pool.extra[key]
                .as_str()
                .and_then(|s| s.parse().ok())
                .unwrap_or_default()
        };

        let user_in = ata_address(user, &in_mint);
        let user_out = ata_address(user, &out_mint);

        // discriminator("swap") | amount_in(8) | swap_for_y(1) | min_out(8)
        let disc = anchor_discriminator("swap");
        let mut data = Vec::with_capacity(25);
        data.extend_from_slice(&disc);
        data.extend_from_slice(&amount_in.to_le_bytes());
        data.push(swap_for_y as u8);
        data.extend_from_slice(&min_out.to_le_bytes());

        let accounts = vec![
            AccountMeta::new(pool.pool, false),
            AccountMeta::new(bin_array_0, false),
            AccountMeta::new(bin_array_1, false),
            AccountMeta::new(pk("reserve_x"), false),
            AccountMeta::new(pk("reserve_y"), false),
            AccountMeta::new(user_in, false),
            AccountMeta::new(user_out, false),
            AccountMeta::new_readonly(pool.base_mint, false),
            AccountMeta::new_readonly(pool.quote_mint, false),
            AccountMeta::new_readonly(pk("oracle"), false),
            AccountMeta::new(*user, true),
            AccountMeta::new_readonly(SPL_TOKEN_PROGRAM, false),
            AccountMeta::new_readonly(SPL_TOKEN_PROGRAM, false),
            AccountMeta::new_readonly(event_authority, false),
            AccountMeta::new_readonly(METEORA_DLMM, false),
        ];

        Ok(vec![Instruction {
            program_id: METEORA_DLMM,
            accounts,
            data,
        }])
    }
}
