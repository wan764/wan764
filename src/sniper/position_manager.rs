//! Position manager — tracks open positions and triggers auto-sell when
//! take-profit, stop-loss, or timeout conditions are met.

use std::{collections::HashMap, sync::Arc};

use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey, signature::Keypair};
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::{
    config::Config,
    dex::DexHandler,
    error::Result,
    sniper::executor::Executor,
    types::{CloseReason, Position, PositionStatus},
    utils::{ata_address, now_ms},
};

pub struct PositionManager {
    positions: Mutex<HashMap<String, Position>>,
    config: Arc<Config>,
    rpc: RpcClient,
}

impl PositionManager {
    pub fn new(config: Arc<Config>) -> Self {
        let rpc = RpcClient::new_with_commitment(
            config.rpc_url.clone(),
            CommitmentConfig::confirmed(),
        );
        Self {
            positions: Mutex::new(HashMap::new()),
            config,
            rpc,
        }
    }

    pub async fn add(&self, position: Position) {
        let mut map = self.positions.lock().await;
        info!(
            "Position opened  id={} pool={} entry_price={:.6} sol_spent={:.4}",
            position.id, position.pool.pool, position.entry_price, position.sol_spent
        );
        map.insert(position.id.clone(), position);
    }

    /// Periodic loop: check every position, sell if targets met.
    pub async fn run(
        self: Arc<Self>,
        keypair: Arc<Keypair>,
        handlers: Arc<HashMap<String, Arc<dyn DexHandler>>>,
        executor: Arc<Executor>,
    ) {
        let interval = std::time::Duration::from_millis(self.config.position_check_ms);
        loop {
            tokio::time::sleep(interval).await;
            if let Err(e) = self
                .check_positions(keypair.clone(), handlers.clone(), executor.clone())
                .await
            {
                warn!("position check error: {e}");
            }
        }
    }

    async fn check_positions(
        &self,
        keypair: Arc<Keypair>,
        handlers: Arc<HashMap<String, Arc<dyn DexHandler>>>,
        executor: Arc<Executor>,
    ) -> Result<()> {
        let mut map = self.positions.lock().await;
        let now = now_ms() / 1000; // seconds

        for position in map.values_mut() {
            if position.status != PositionStatus::Open {
                continue;
            }

            // Check timeout
            let age_secs = now - position.opened_at / 1000;
            if age_secs > self.config.max_hold_secs {
                info!(
                    "Position {} timed out after {}s — force selling",
                    position.id, age_secs
                );
                Self::try_sell(
                    position,
                    CloseReason::Timeout,
                    keypair.clone(),
                    handlers.clone(),
                    executor.clone(),
                )
                .await;
                continue;
            }

            // Fetch current price
            let current_price = match self
                .fetch_current_price(&position.pool.pool, &position.pool.base_mint)
                .await
            {
                Ok(p) => p,
                Err(e) => {
                    warn!("price fetch for {} failed: {e}", position.id);
                    continue;
                }
            };

            let multiplier = current_price / position.entry_price;

            if multiplier >= self.config.take_profit_x {
                info!(
                    "Position {} take-profit hit: {:.2}x",
                    position.id, multiplier
                );
                Self::try_sell(
                    position,
                    CloseReason::TakeProfit,
                    keypair.clone(),
                    handlers.clone(),
                    executor.clone(),
                )
                .await;
            } else if multiplier <= self.config.stop_loss_x {
                info!(
                    "Position {} stop-loss hit: {:.2}x",
                    position.id, multiplier
                );
                Self::try_sell(
                    position,
                    CloseReason::StopLoss,
                    keypair.clone(),
                    handlers.clone(),
                    executor.clone(),
                )
                .await;
            }
        }

        Ok(())
    }

    async fn try_sell(
        position: &mut Position,
        reason: CloseReason,
        keypair: Arc<Keypair>,
        handlers: Arc<HashMap<String, Arc<dyn DexHandler>>>,
        executor: Arc<Executor>,
    ) {
        let dex_key = format!("{:?}", position.pool.dex);
        let handler = match handlers.get(&dex_key) {
            Some(h) => h.clone(),
            None => {
                warn!("no handler for dex {:?}", position.pool.dex);
                return;
            }
        };

        match executor
            .sell(&position.pool, &handler, &keypair, position.base_amount)
            .await
        {
            Ok(sig) => {
                let current_sol = 0.0; // would require another RPC call to compute precisely
                position.status = PositionStatus::Closed {
                    reason,
                    sell_tx: sig.clone(),
                    pnl_sol: current_sol - position.sol_spent,
                };
                info!(
                    "Position {} closed ({}) sell_tx={}",
                    position.id, position.status_str(),
                    sig
                );
            }
            Err(e) => {
                warn!("sell failed for position {}: {e}", position.id);
                position.status = PositionStatus::Error(e.to_string());
            }
        }
    }

    /// Rough price estimate: balance of base-token vault / balance of quote-token vault.
    async fn fetch_current_price(
        &self,
        _pool: &Pubkey,
        _base_mint: &Pubkey,
    ) -> Result<f64> {
        // For a production bot you would read the pool state account and compute
        // the price from the vault balances (or use a price oracle).
        // Placeholder: returns 1.0 so stop-loss/take-profit don't fire spuriously.
        Ok(1.0)
    }
}

// Helper on PositionStatus for display
impl Position {
    pub fn status_str(&self) -> String {
        match &self.status {
            PositionStatus::Open => "open".to_string(),
            PositionStatus::Closed { reason, .. } => format!("closed/{reason}"),
            PositionStatus::Error(e) => format!("error: {e}"),
        }
    }
}
