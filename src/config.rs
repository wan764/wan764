use crate::error::{BotError, Result};
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

#[derive(Debug, Clone)]
pub struct Config {
    // RPC
    pub rpc_url: String,
    pub ws_url: String,

    // Wallet
    pub private_key: String,

    // Jito
    pub use_jito: bool,
    pub jito_tip_lamports: u64,
    pub jito_block_engine_url: String,

    // Snipe parameters
    pub buy_amount_lamports: u64,
    pub max_slippage_bps: u64,
    pub compute_unit_price: u64,
    pub compute_unit_limit: u32,

    // DEX toggles
    pub enable_raydium_amm: bool,
    pub enable_raydium_cpmm: bool,
    pub enable_orca: bool,
    pub enable_meteora_dlmm: bool,
    pub enable_meteora_dammv2: bool,

    // Filters
    pub min_pool_liquidity_lamports: u64,
    pub quote_mints: Vec<Pubkey>,
    pub reject_mint_authority: bool,
    pub reject_freeze_authority: bool,

    // Auto-sell
    pub auto_sell: bool,
    pub take_profit_x: f64,
    pub stop_loss_x: f64,
    pub max_hold_secs: u64,
    pub position_check_ms: u64,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let get = |key: &str| -> Result<String> {
            std::env::var(key).map_err(|_| BotError::Config(format!("missing env var: {key}")))
        };
        let get_or = |key: &str, default: &str| -> String {
            std::env::var(key).unwrap_or_else(|_| default.to_string())
        };
        let bool_var = |key: &str, default: bool| -> bool {
            std::env::var(key)
                .map(|v| matches!(v.to_lowercase().as_str(), "true" | "1" | "yes"))
                .unwrap_or(default)
        };
        let f64_var = |key: &str, default: f64| -> f64 {
            std::env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
        };
        let u64_var = |key: &str, default: u64| -> u64 {
            std::env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
        };

        let buy_amount_sol = f64_var("BUY_AMOUNT_SOL", 0.1);
        let min_liquidity_sol = f64_var("MIN_POOL_LIQUIDITY_SOL", 1.0);
        const LAMPORTS_PER_SOL: f64 = 1_000_000_000.0;

        let quote_mints_str = get_or(
            "QUOTE_MINTS",
            "So11111111111111111111111111111111111111112,EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
        );
        let quote_mints: Vec<Pubkey> = quote_mints_str
            .split(',')
            .filter(|s| !s.is_empty())
            .map(|s| Pubkey::from_str(s.trim()).map_err(|e| BotError::Config(e.to_string())))
            .collect::<Result<Vec<_>>>()?;

        Ok(Config {
            rpc_url: get("RPC_ENDPOINT")?,
            ws_url: get("RPC_WS_ENDPOINT")?,
            private_key: get("PRIVATE_KEY")?,
            use_jito: bool_var("USE_JITO", true),
            jito_tip_lamports: u64_var("JITO_TIP_LAMPORTS", 100_000),
            jito_block_engine_url: get_or(
                "JITO_BLOCK_ENGINE_URL",
                "https://mainnet.block-engine.jito.wtf",
            ),
            buy_amount_lamports: (buy_amount_sol * LAMPORTS_PER_SOL) as u64,
            max_slippage_bps: u64_var("MAX_SLIPPAGE_BPS", 1500),
            compute_unit_price: u64_var("COMPUTE_UNIT_PRICE", 100_000),
            compute_unit_limit: u64_var("COMPUTE_UNIT_LIMIT", 300_000) as u32,
            enable_raydium_amm: bool_var("ENABLE_RAYDIUM_AMM", true),
            enable_raydium_cpmm: bool_var("ENABLE_RAYDIUM_CPMM", true),
            enable_orca: bool_var("ENABLE_ORCA", true),
            enable_meteora_dlmm: bool_var("ENABLE_METEORA_DLMM", true),
            enable_meteora_dammv2: bool_var("ENABLE_METEORA_DAMMV2", true),
            min_pool_liquidity_lamports: (min_liquidity_sol * LAMPORTS_PER_SOL) as u64,
            quote_mints,
            reject_mint_authority: bool_var("REJECT_MINT_AUTHORITY", true),
            reject_freeze_authority: bool_var("REJECT_FREEZE_AUTHORITY", true),
            auto_sell: bool_var("AUTO_SELL", true),
            take_profit_x: f64_var("TAKE_PROFIT_X", 2.0),
            stop_loss_x: f64_var("STOP_LOSS_X", 0.5),
            max_hold_secs: u64_var("MAX_HOLD_SECS", 300),
            position_check_ms: u64_var("POSITION_CHECK_MS", 5000),
        })
    }
}
