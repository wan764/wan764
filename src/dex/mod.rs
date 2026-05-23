pub mod meteora_dammv2;
pub mod meteora_dlmm;
pub mod orca;
pub mod raydium;

use crate::error::Result;
use crate::types::PoolInfo;
use solana_sdk::{instruction::Instruction, pubkey::Pubkey};

/// Common interface every DEX handler must implement.
pub trait DexHandler: Send + Sync {
    /// Log prefix that indicates a pool-creation transaction.
    fn pool_init_log(&self) -> &'static str;

    /// Extract a [PoolInfo] from the flat account-key list and instruction
    /// account-index slice of the pool-creation instruction.
    fn parse_pool(
        &self,
        account_keys: &[Pubkey],
        ix_accounts: &[u8],
        signature: &str,
    ) -> Result<PoolInfo>;

    /// Build the swap instruction(s) that buy `amount_in` of the quote token
    /// for as many base tokens as possible (min output enforced by `min_out`).
    fn build_swap_ix(
        &self,
        pool: &PoolInfo,
        user: &Pubkey,
        amount_in: u64,
        min_out: u64,
        quote_to_base: bool,
    ) -> Result<Vec<Instruction>>;
}
