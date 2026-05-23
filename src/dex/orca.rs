//! Orca Whirlpool pool detection and swap building.

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

pub struct Orca;

impl Orca {
    fn tick_array_pda(whirlpool: &Pubkey, start_tick: i32) -> Pubkey {
        Pubkey::find_program_address(
            &[b"tick_array", whirlpool.as_ref(), &start_tick.to_le_bytes()],
            &ORCA_WHIRLPOOL,
        )
        .0
    }

    fn oracle_pda(whirlpool: &Pubkey) -> Pubkey {
        Pubkey::find_program_address(&[b"oracle", whirlpool.as_ref()], &ORCA_WHIRLPOOL).0
    }

    fn tick_array_starts(current_tick: i32, tick_spacing: i32, a_to_b: bool) -> [i32; 3] {
        let ticks_per = ORCA_TICKS_PER_ARRAY * tick_spacing;
        let base = current_tick.div_euclid(ticks_per) * ticks_per;
        if a_to_b {
            [base, base - ticks_per, base - 2 * ticks_per]
        } else {
            [base, base + ticks_per, base + 2 * ticks_per]
        }
    }
}

impl DexHandler for Orca {
    fn pool_init_log(&self) -> &'static str {
        "InitializePool"
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
                .ok_or_else(|| BotError::Parse(format!("Orca ix_accounts[{idx}] missing")))? as usize;
            account_keys
                .get(ai)
                .copied()
                .ok_or_else(|| BotError::Parse(format!("Orca account_keys[{ai}] missing")))
        };
        Ok(PoolInfo {
            dex: Dex::Orca,
            pool: get(ORCA_INIT_POOL_IDX)?,
            base_mint: get(ORCA_INIT_MINT_A_IDX)?,
            quote_mint: get(ORCA_INIT_MINT_B_IDX)?,
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
        let a_to_b = !quote_to_base; // a_to_b = true ⇒ sell base (A) for quote (B)

        let (in_mint, out_mint) = if a_to_b {
            (pool.base_mint, pool.quote_mint)
        } else {
            (pool.quote_mint, pool.base_mint)
        };

        let current_tick = pool.extra["current_tick"].as_i64().unwrap_or(0) as i32;
        let tick_spacing = pool.extra["tick_spacing"].as_i64().unwrap_or(64) as i32;

        let starts = Self::tick_array_starts(current_tick, tick_spacing, a_to_b);
        let tick_array_0 = Self::tick_array_pda(&pool.pool, starts[0]);
        let tick_array_1 = Self::tick_array_pda(&pool.pool, starts[1]);
        let tick_array_2 = Self::tick_array_pda(&pool.pool, starts[2]);
        let oracle = Self::oracle_pda(&pool.pool);

        let pk = |key: &str| -> Pubkey {
            pool.extra[key]
                .as_str()
                .and_then(|s| s.parse().ok())
                .unwrap_or_default()
        };

        let user_in = ata_address(user, &in_mint);
        let user_out = ata_address(user, &out_mint);

        let disc = anchor_discriminator("swap");
        let sqrt_price_limit: u128 = 0; // no limit
        let mut data = Vec::with_capacity(43);
        data.extend_from_slice(&disc);
        data.extend_from_slice(&amount_in.to_le_bytes());
        data.extend_from_slice(&min_out.to_le_bytes());
        data.extend_from_slice(&sqrt_price_limit.to_le_bytes());
        data.push(1u8);       // amount_specified_is_input = true
        data.push(a_to_b as u8);

        let accounts = vec![
            AccountMeta::new_readonly(SPL_TOKEN_PROGRAM, false),
            AccountMeta::new_readonly(*user, true),
            AccountMeta::new(pool.pool, false),
            AccountMeta::new(user_in, false),
            AccountMeta::new(pk("vault_a"), false),
            AccountMeta::new(user_out, false),
            AccountMeta::new(pk("vault_b"), false),
            AccountMeta::new(tick_array_0, false),
            AccountMeta::new(tick_array_1, false),
            AccountMeta::new(tick_array_2, false),
            AccountMeta::new_readonly(oracle, false),
        ];

        Ok(vec![Instruction {
            program_id: ORCA_WHIRLPOOL,
            accounts,
            data,
        }])
    }
}
