use solana_sdk::pubkey::Pubkey;
use std::fmt;

// ── DEX enum ──────────────────────────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Dex {
    RaydiumAmm,
    RaydiumCpmm,
    Orca,
    MeteoraDlmm,
    MeteoraDammv2,
}

impl fmt::Display for Dex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Dex::RaydiumAmm    => write!(f, "Raydium AMM"),
            Dex::RaydiumCpmm   => write!(f, "Raydium CPMM"),
            Dex::Orca          => write!(f, "Orca Whirlpool"),
            Dex::MeteoraDlmm   => write!(f, "Meteora DLMM"),
            Dex::MeteoraDammv2 => write!(f, "Meteora DAMMv2"),
        }
    }
}

impl Dex {
    pub fn program_id(&self) -> Pubkey {
        use crate::constants::*;
        match self {
            Dex::RaydiumAmm    => RAYDIUM_AMM_V4,
            Dex::RaydiumCpmm   => RAYDIUM_CPMM,
            Dex::Orca          => ORCA_WHIRLPOOL,
            Dex::MeteoraDlmm   => METEORA_DLMM,
            Dex::MeteoraDammv2 => METEORA_DAMMV2,
        }
    }
}

// ── Pool information discovered from chain ────────────────────
#[derive(Debug, Clone)]
pub struct PoolInfo {
    pub dex: Dex,
    pub pool: Pubkey,
    pub base_mint: Pubkey,
    pub quote_mint: Pubkey,
    pub base_decimals: u8,
    pub quote_decimals: u8,
    /// Extra per-DEX state (serialised as JSON, lazily)
    pub extra: serde_json::Value,
    pub detected_at_ms: u64,
    pub tx_signature: String,
}

// ── Open trading position ──────────────────────────────────────
#[derive(Debug, Clone)]
pub struct Position {
    pub id: String,
    pub pool: PoolInfo,
    pub buy_tx: String,
    pub entry_price: f64,
    pub base_amount: u64,
    pub sol_spent: f64,
    pub opened_at: u64,
    pub status: PositionStatus,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PositionStatus {
    Open,
    Closed { reason: CloseReason, sell_tx: String, pnl_sol: f64 },
    Error(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum CloseReason {
    TakeProfit,
    StopLoss,
    Timeout,
    Manual,
}

impl fmt::Display for CloseReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CloseReason::TakeProfit => write!(f, "take-profit"),
            CloseReason::StopLoss   => write!(f, "stop-loss"),
            CloseReason::Timeout    => write!(f, "timeout"),
            CloseReason::Manual     => write!(f, "manual"),
        }
    }
}

// ── Internal channel message from listener → executor ─────────
#[derive(Debug)]
pub enum BotEvent {
    NewPool(Box<PoolInfo>),
    Shutdown,
}
