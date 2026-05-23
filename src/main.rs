mod config;
mod constants;
mod dex;
mod error;
mod sniper;
mod types;
mod utils;
mod wallet;

use std::{collections::HashMap, sync::Arc};

use tokio::sync::mpsc;
use tracing::{error, info};
use tracing_subscriber::{fmt, EnvFilter};

use config::Config;
use dex::{
    meteora_dammv2::MeteoraDammv2,
    meteora_dlmm::MeteoraDlmm,
    orca::Orca,
    raydium::{RaydiumAmm, RaydiumCpmm},
    DexHandler,
};
use sniper::{
    executor::Executor,
    filter::Filter,
    listener::run_listener,
    position_manager::PositionManager,
};
use solana_sdk::signature::Keypair;
use types::{BotEvent, Dex, Position};
use wallet::Wallet;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ── Logging ──────────────────────────────────────────────
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("sniper=info".parse()?))
        .with_target(false)
        .init();

    // ── Config ────────────────────────────────────────────────
    dotenv::dotenv().ok();
    let config = Arc::new(Config::from_env().expect("failed to load config from .env"));

    // ── Wallet ────────────────────────────────────────────────
    let wallet = Arc::new(
        Wallet::from_base58(&config.private_key).expect("invalid PRIVATE_KEY"),
    );
    info!("Sniper wallet: {}", wallet.pubkey());

    // ── Build handler map ──────────────────────────────────────
    // key = format!("{:?}", Dex::*) so position_manager can look them up
    let mut handlers: HashMap<String, Arc<dyn DexHandler>> = HashMap::new();

    let active_dexes: Vec<(Dex, Arc<dyn DexHandler>)> = {
        let mut v: Vec<(Dex, Arc<dyn DexHandler>)> = vec![];
        if config.enable_raydium_amm {
            v.push((Dex::RaydiumAmm, Arc::new(RaydiumAmm)));
        }
        if config.enable_raydium_cpmm {
            v.push((Dex::RaydiumCpmm, Arc::new(RaydiumCpmm)));
        }
        if config.enable_orca {
            v.push((Dex::Orca, Arc::new(Orca)));
        }
        if config.enable_meteora_dlmm {
            v.push((Dex::MeteoraDlmm, Arc::new(MeteoraDlmm)));
        }
        if config.enable_meteora_dammv2 {
            v.push((Dex::MeteoraDammv2, Arc::new(MeteoraDammv2)));
        }
        v
    };

    for (dex, handler) in &active_dexes {
        handlers.insert(format!("{dex:?}"), handler.clone());
    }
    let handlers = Arc::new(handlers);

    if active_dexes.is_empty() {
        error!("No DEXes enabled — set at least one ENABLE_* env var to true");
        return Ok(());
    }

    info!(
        "Monitoring {} DEX(es): {}",
        active_dexes.len(),
        active_dexes
            .iter()
            .map(|(d, _)| d.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );

    // ── Channel: listeners → main loop ────────────────────────
    let (tx, mut rx) = mpsc::channel::<BotEvent>(256);

    // ── Spawn one listener task per DEX ───────────────────────
    for (dex, handler) in active_dexes.iter().cloned() {
        let config_clone = config.clone();
        let tx_clone = tx.clone();
        let handler_clone = handler.clone();
        tokio::spawn(async move {
            run_listener(dex, handler_clone, config_clone, tx_clone).await;
        });
    }

    // ── Executor & Position manager ───────────────────────────
    let executor = Arc::new(Executor::new(config.clone()));
    let position_manager = Arc::new(PositionManager::new(config.clone()));

    if config.auto_sell {
        let pm = position_manager.clone();
        let kp_bytes = wallet.keypair().to_bytes();
        let ex = executor.clone();
        let hm = handlers.clone();
        tokio::spawn(async move {
            let kp = Keypair::try_from(kp_bytes.as_slice()).expect("keypair bytes");
            pm.run(Arc::new(kp), hm, ex).await;
        });
    }

    // ── Main event loop ───────────────────────────────────────
    let filter = Filter::new((*config).clone());

    info!("Bot is live — waiting for new pools…");

    while let Some(event) = rx.recv().await {
        match event {
            BotEvent::Shutdown => {
                info!("Shutdown signal received");
                break;
            }
            BotEvent::NewPool(pool) => {
                let pool = *pool;

                // Run filter checks
                if let Err(e) = filter.check(&pool).await {
                    info!("Pool {} rejected: {e}", pool.pool);
                    continue;
                }

                info!(
                    "Pool {} passed filters — sniping on {}",
                    pool.pool, pool.dex
                );

                // Look up the handler for this DEX
                let handler = match handlers.get(&format!("{:?}", pool.dex)) {
                    Some(h) => h.clone(),
                    None => {
                        error!("No handler for dex {:?}", pool.dex);
                        continue;
                    }
                };

                // Clone what we need for the async task
                let config_clone = config.clone();
                let executor_clone = executor.clone();
                let position_manager_clone = position_manager.clone();
                let kp_bytes = wallet.keypair().to_bytes();
                let keypair = Keypair::try_from(kp_bytes.as_slice()).expect("keypair bytes");

                tokio::spawn(async move {
                    match executor_clone.buy(&pool, &handler, &keypair).await {
                        Ok((sig, base_amount)) => {
                            let position = Position {
                                id: uuid::Uuid::new_v4().to_string(),
                                entry_price: 1.0, // refined by position_manager
                                base_amount,
                                sol_spent: utils::lamports_to_sol(
                                    config_clone.buy_amount_lamports,
                                ),
                                opened_at: utils::now_ms(),
                                buy_tx: sig,
                                status: types::PositionStatus::Open,
                                pool: pool.clone(),
                            };
                            if config_clone.auto_sell {
                                position_manager_clone.add(position).await;
                            }
                        }
                        Err(e) => {
                            error!("Buy failed for pool {}: {e}", pool.pool);
                        }
                    }
                });
            }
        }
    }

    Ok(())
}
