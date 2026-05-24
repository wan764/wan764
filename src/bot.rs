//! Async bot runner — called from the GUI when the user clicks "Start".

use std::{collections::HashMap, sync::Arc};

use tokio::sync::mpsc as async_mpsc;

use crate::{
    config::Config,
    dex::{
        meteora_dammv2::MeteoraDammv2, meteora_dlmm::MeteoraDlmm, orca::Orca,
        raydium::{RaydiumAmm, RaydiumCpmm},
        DexHandler,
    },
    gui_event as gui,
    sniper::{
        executor::Executor, filter::Filter, listener::run_listener,
        position_manager::PositionManager,
    },
    types::{BotEvent, Dex, Position},
    utils::{lamports_to_sol, now_ms},
};
use solana_sdk::signature::Keypair;

pub async fn run_bot(config: Config, bot_tx: std::sync::mpsc::Sender<crate::gui_event::GuiEvent>) {
    // Register the GUI sender so any module can call gui::info! / gui::error!
    crate::gui_event::set_sender(bot_tx);

    gui::success("Bot initializing…");

    // ── Wallet ────────────────────────────────────────────────
    let wallet = match crate::wallet::Wallet::from_base58(&config.private_key) {
        Ok(w) => Arc::new(w),
        Err(e) => {
            gui::error(format!("Invalid private key: {e}"));
            return;
        }
    };
    gui::info(format!("Wallet: {}", wallet.pubkey()));

    // ── Handlers ──────────────────────────────────────────────
    let mut handler_map: HashMap<String, Arc<dyn DexHandler>> = HashMap::new();
    let mut active: Vec<(Dex, Arc<dyn DexHandler>)> = vec![];

    if config.enable_raydium_amm  { active.push((Dex::RaydiumAmm,    Arc::new(RaydiumAmm))); }
    if config.enable_raydium_cpmm { active.push((Dex::RaydiumCpmm,   Arc::new(RaydiumCpmm))); }
    if config.enable_orca         { active.push((Dex::Orca,          Arc::new(Orca))); }
    if config.enable_meteora_dlmm { active.push((Dex::MeteoraDlmm,   Arc::new(MeteoraDlmm))); }
    if config.enable_meteora_dammv2 { active.push((Dex::MeteoraDammv2, Arc::new(MeteoraDammv2))); }

    if active.is_empty() {
        gui::error("No DEXes enabled — enable at least one in settings.");
        return;
    }

    for (dex, h) in &active {
        handler_map.insert(format!("{dex:?}"), h.clone());
    }
    let handler_map = Arc::new(handler_map);

    let dex_names: Vec<String> = active.iter().map(|(d, _)| d.to_string()).collect();
    gui::success(format!("Monitoring: {}", dex_names.join(", ")));

    // Show what we are targeting
    match &config.target_token_mint {
        Some(mint) => gui::success(format!("🎯 Target token: {mint}")),
        None => gui::warn("⚠ No target token set — bot will buy EVERY new pool!"),
    }

    // ── Channels ──────────────────────────────────────────────
    let (pool_tx, mut pool_rx) = async_mpsc::channel::<BotEvent>(256);

    // ── Spawn listeners ───────────────────────────────────────
    let config_arc = Arc::new(config.clone());
    for (dex, handler) in active.iter().cloned() {
        let cfg = config_arc.clone();
        let tx  = pool_tx.clone();
        tokio::spawn(async move {
            run_listener(dex, handler, cfg, tx).await;
        });
    }

    // ── Executor & position manager ───────────────────────────
    let executor         = Arc::new(Executor::new(config_arc.clone()));
    let position_manager = Arc::new(PositionManager::new(config_arc.clone()));

    if config.auto_sell {
        let pm  = position_manager.clone();
        let kpb = wallet.keypair().to_bytes();
        let ex  = executor.clone();
        let hm  = handler_map.clone();
        tokio::spawn(async move {
            let kp = Keypair::try_from(kpb.as_slice()).expect("keypair");
            pm.run(Arc::new(kp), hm, ex).await;
        });
    }

    // ── Filter ────────────────────────────────────────────────
    let filter = Filter::new(config.clone());

    gui::success("Bot is live — watching for new pools…");

    // ── Main event loop ───────────────────────────────────────
    while let Some(event) = pool_rx.recv().await {
        match event {
            BotEvent::Shutdown => break,
            BotEvent::NewPool(pool) => {
                let pool = *pool;

                gui::info(format!(
                    "[DETECT] {} pool={} base={} quote={}",
                    pool.dex,
                    &pool.pool.to_string()[..8],
                    &pool.base_mint.to_string()[..8],
                    &pool.quote_mint.to_string()[..8],
                ));

                if let Err(e) = filter.check(&pool).await {
                    gui::warn(format!("[FILTER] ✗ Rejected: {e}"));
                    continue;
                }
                gui::info(format!("[FILTER] ✓ Passed filters"));

                let handler = match handler_map.get(&format!("{:?}", pool.dex)) {
                    Some(h) => h.clone(),
                    None    => continue,
                };

                let executor_clone         = executor.clone();
                let position_manager_clone = position_manager.clone();
                let kpb                    = wallet.keypair().to_bytes();
                let sol_in                 = lamports_to_sol(config.buy_amount_lamports);
                let auto_sell              = config.auto_sell;
                let spam_enabled           = config.spam_enabled;

                tokio::spawn(async move {
                    let keypair = Keypair::try_from(kpb.as_slice()).expect("keypair");
                    let buy_result = if spam_enabled {
                        executor_clone.spam_buy(&pool, &handler, &keypair).await
                    } else {
                        executor_clone.buy(&pool, &handler, &keypair).await
                    };
                    match buy_result {
                        Ok((sig, base_amount)) => {
                            gui::success(format!(
                                "[BUY] ✓ {:.4} SOL  tx={}…",
                                sol_in,
                                &sig[..8]
                            ));
                            gui::position_opened(
                                &pool.pool.to_string(),
                                &pool.dex.to_string(),
                                sol_in,
                            );
                            if auto_sell {
                                let pos = Position {
                                    id:           uuid::Uuid::new_v4().to_string(),
                                    entry_price:  1.0,
                                    base_amount,
                                    sol_spent:    sol_in,
                                    opened_at:    now_ms(),
                                    buy_tx:       sig,
                                    status:       crate::types::PositionStatus::Open,
                                    pool,
                                };
                                position_manager_clone.add(pos).await;
                            }
                        }
                        Err(e) => {
                            gui::error(format!("[BUY] ✗ Failed: {e}"));
                        }
                    }
                });
            }
        }
    }

    crate::gui_event::clear_sender();
}
