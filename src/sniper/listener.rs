//! WebSocket log subscriber — detects new pool-creation transactions on every
//! enabled DEX and forwards them over an mpsc channel.

use std::{
    collections::HashSet,
    sync::Arc,
    time::Duration,
};

use futures_util::StreamExt;
use solana_client::{
    nonblocking::{pubsub_client::PubsubClient, rpc_client::RpcClient},
    rpc_config::{RpcTransactionLogsConfig, RpcTransactionLogsFilter},
    rpc_response::RpcLogsResponse,
};
use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey, signature::Signature};
use solana_transaction_status::{EncodedTransaction, UiMessage, UiTransactionEncoding};
use tokio::sync::{mpsc, Mutex as TokioMutex};
use tracing::debug;

use crate::{
    config::Config,
    dex::DexHandler,
    error::{BotError, Result},
    gui_event as gui,
    types::{BotEvent, Dex, PoolInfo},
};

// ── Public entry point ───────────────────────────────────────────────

/// Runs forever, reconnecting on errors. All status output goes to the GUI.
pub async fn run_listener(
    dex: Dex,
    handler: Arc<dyn DexHandler>,
    config: Arc<Config>,
    tx: mpsc::Sender<BotEvent>,
) {
    // Shared dedup set — prevents processing the same signature twice
    // (Solana WS sometimes delivers duplicate notifications)
    let seen: Arc<TokioMutex<HashSet<String>>> = Arc::new(TokioMutex::new(HashSet::new()));

    loop {
        match listen_once(dex, handler.clone(), config.clone(), tx.clone(), seen.clone()).await {
            Ok(()) => {
                gui::warn(format!("[{dex}] WS stream ended — reconnecting…"));
            }
            Err(e) => {
                gui::error(format!("[{dex}] WS error: {e} — reconnecting in 3 s…"));
                tokio::time::sleep(Duration::from_secs(3)).await;
            }
        }
    }
}

// ── Internal ────────────────────────────────────────────────────────

async fn listen_once(
    dex: Dex,
    handler: Arc<dyn DexHandler>,
    config: Arc<Config>,
    tx: mpsc::Sender<BotEvent>,
    seen: Arc<TokioMutex<HashSet<String>>>,
) -> anyhow::Result<()> {
    use anyhow::Context;

    let client = PubsubClient::new(&config.ws_url)
        .await
        .context("PubsubClient::new")?;

    let (mut stream, _unsub) = client
        .logs_subscribe(
            RpcTransactionLogsFilter::Mentions(vec![dex.program_id().to_string()]),
            RpcTransactionLogsConfig {
                commitment: Some(CommitmentConfig::confirmed()),
            },
        )
        .await
        .context("logs_subscribe")?;

    gui::success(format!(
        "[{dex}] ✓ WebSocket connected ({}…{})",
        &dex.program_id().to_string()[..6],
        &dex.program_id().to_string()[38..],
    ));

    while let Some(response) = stream.next().await {
        let log_resp: RpcLogsResponse = response.value;

        // Skip failed transactions
        if log_resp.err.is_some() {
            continue;
        }

        // Filter to pool-init transactions only (case-insensitive)
        let init_kw = handler.pool_init_log().to_lowercase();
        if !log_resp.logs.iter().any(|l| l.to_lowercase().contains(&init_kw)) {
            continue;
        }

        let sig = log_resp.signature.clone();

        // ── Deduplication ───────────────────────────────────────────
        {
            let mut guard = seen.lock().await;
            if guard.contains(&sig) {
                debug!("[{dex}] duplicate sig skipped: {sig}");
                continue;
            }
            guard.insert(sig.clone());
            // Keep memory bounded — clear when set grows large
            if guard.len() > 2_000 {
                guard.clear();
            }
        }

        let sig_short = if sig.len() >= 8 { sig[..8].to_string() } else { sig.clone() };
        gui::info(format!("[{dex}] 🔍 Pool-init tx: {sig_short}…"));

        let handler_clone = handler.clone();
        let config_clone  = config.clone();
        let tx_clone      = tx.clone();

        tokio::spawn(async move {
            match fetch_and_parse_with_retry(dex, handler_clone.as_ref(), &config_clone, &sig).await {
                Ok(pool) => {
                    gui::info(format!(
                        "[{dex}] 📦 Pool parsed  pool={}…  base={}…  quote={}…",
                        &pool.pool.to_string()[..8],
                        &pool.base_mint.to_string()[..8],
                        &pool.quote_mint.to_string()[..8],
                    ));
                    let _ = tx_clone.send(BotEvent::NewPool(Box::new(pool))).await;
                }
                Err(e) => {
                    gui::warn(format!("[{dex}] ✗ Parse failed ({sig_short}…): {e}"));
                }
            }
        });
    }

    Ok(())
}

/// Fetch + parse with up to 6 retries (initial 800 ms delay, then 500 ms each).
/// The RPC often doesn't have the tx indexed yet at the moment the WS fires.
async fn fetch_and_parse_with_retry(
    dex: Dex,
    handler: &dyn DexHandler,
    config: &Config,
    signature: &str,
) -> Result<PoolInfo> {
    let rpc = RpcClient::new_with_commitment(
        config.rpc_url.clone(),
        CommitmentConfig::confirmed(),
    );

    let sig: Signature = signature
        .parse()
        .map_err(|e| BotError::Parse(format!("bad signature: {e}")))?;

    // Wait a moment before the first attempt — tx needs time to be indexed
    tokio::time::sleep(Duration::from_millis(800)).await;

    const MAX_ATTEMPTS: u32 = 6;
    let mut last_err = BotError::Parse("no attempts made".to_string());

    for attempt in 1..=MAX_ATTEMPTS {
        match rpc.get_transaction(&sig, UiTransactionEncoding::Json).await {
            Ok(tx_with_meta) => {
                let program_id = dex.program_id();
                let (account_keys, ix_accounts) =
                    parse_encoded_tx(tx_with_meta.transaction.transaction, &program_id)?;
                return handler.parse_pool(&account_keys, &ix_accounts, signature);
            }
            Err(e) => {
                last_err = BotError::Rpc(e);
                if attempt < MAX_ATTEMPTS {
                    // Exponential-ish back-off: 500 ms, 700 ms, 900 ms …
                    let wait_ms = 500 + (attempt - 1) as u64 * 200;
                    tokio::time::sleep(Duration::from_millis(wait_ms)).await;
                }
            }
        }
    }

    Err(last_err)
}

/// Parse account keys and find the DEX instruction's account-index list in one pass.
fn parse_encoded_tx(
    encoded: EncodedTransaction,
    program_id: &Pubkey,
) -> Result<(Vec<Pubkey>, Vec<u8>)> {
    let ui_tx = match encoded {
        EncodedTransaction::Json(t) => t,
        _ => {
            return Err(BotError::Parse(
                "unsupported encoding (expected Json)".to_string(),
            ))
        }
    };

    let raw_msg = match ui_tx.message {
        UiMessage::Raw(msg) => msg,
        UiMessage::Parsed(_) => {
            return Err(BotError::Parse(
                "parsed message format not supported".to_string(),
            ))
        }
    };

    let account_keys: Vec<Pubkey> = raw_msg
        .account_keys
        .iter()
        .map(|k| k.parse::<Pubkey>().map_err(|e| BotError::Parse(e.to_string())))
        .collect::<Result<Vec<_>>>()?;

    let ix_accounts = raw_msg
        .instructions
        .iter()
        .find(|ix| {
            account_keys
                .get(ix.program_id_index as usize)
                .map_or(false, |p| p == program_id)
        })
        .map(|ix| ix.accounts.clone())
        .ok_or_else(|| {
            BotError::Parse(format!("no instruction for program {program_id}"))
        })?;

    Ok((account_keys, ix_accounts))
}
