//! Meteora Dynamic AMM v2 (DAMMv2) pool detection and swap building.

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

pub struct MeteoraDammv2;

impl MeteoraDammv2 {
    fn event_authority() -> Pubkey {
        Pubkey::find_program_address(&[b"__event_authority"], &METEORA_DAMMV2).0
    }

    fn pool_authority(pool: &Pubkey) -> Pubkey {
        Pubkey::find_program_address(
            &[b"pool_authority", pool.as_ref()],
            &METEORA_DAMMV2,
        )
        .0
    }
}

impl DexHandler for MeteoraDammv2 {
    fn pool_init_log(&self) -> &'static str {
        // Anchor: "Program log: Instruction: InitializePool"
        "Instruction: InitializePool"
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
                .ok_or_else(|| BotError::Parse(format!("DAMMv2 ix_accounts[{idx}] missing")))? as usize;
            account_keys
                .get(ai)
                .copied()
                .ok_or_else(|| BotError::Parse(format!("DAMMv2 account_keys[{ai}] missing")))
        };
        Ok(PoolInfo {
            dex: Dex::MeteoraDammv2,
            pool: get(DAMMV2_INIT_POOL_IDX)?,
            base_mint: get(DAMMV2_INIT_MINT_A_IDX)?,
            quote_mint: get(DAMMV2_INIT_MINT_B_IDX)?,
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
        let (in_mint, out_mint) = if quote_to_base {
            (pool.quote_mint, pool.base_mint)
        } else {
            (pool.base_mint, pool.quote_mint)
        };

        let pk = |key: &str| -> Pubkey {
            pool.extra[key]
                .as_str()
                .and_then(|s| s.parse().ok())
                .unwrap_or_default()
        };

        let pool_authority = pk("pool_authority");
        let pool_authority = if pool_authority == Pubkey::default() {
            Self::pool_authority(&pool.pool)
        } else {
            pool_authority
        };

        let user_in = ata_address(user, &in_mint);
        let user_out = ata_address(user, &out_mint);
        let event_authority = Self::event_authority();

        // discriminator("swap") | in_amount(8) | minimum_out_amount(8)
        let disc = anchor_discriminator("swap");
        let mut data = Vec::with_capacity(24);
        data.extend_from_slice(&disc);
        data.extend_from_slice(&amount_in.to_le_bytes());
        data.extend_from_slice(&min_out.to_le_bytes());

        let accounts = vec![
            AccountMeta::new(pool.pool, false),
            AccountMeta::new(user_in, false),
            AccountMeta::new(user_out, false),
            AccountMeta::new(pk("vault_a"), false),
            AccountMeta::new(pk("vault_b"), false),
            AccountMeta::new_readonly(pool_authority, false),
            AccountMeta::new(*user, true),
            AccountMeta::new_readonly(SPL_TOKEN_PROGRAM, false),
            AccountMeta::new_readonly(SPL_TOKEN_PROGRAM, false),
            AccountMeta::new_readonly(in_mint, false),
            AccountMeta::new_readonly(out_mint, false),
            AccountMeta::new_readonly(event_authority, false),
            AccountMeta::new_readonly(METEORA_DAMMV2, false),
        ];

        Ok(vec![Instruction {
            program_id: METEORA_DAMMV2,
            accounts,
            data,
        }])
    }
}
