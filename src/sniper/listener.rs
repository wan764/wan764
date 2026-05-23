//! WebSocket log subscriber — detects new pool-creation transactions on every
//! enabled DEX and forwards them over an mpsc channel.

use std::sync::Arc;

use futures_util::StreamExt;
use solana_client::{
    nonblocking::{pubsub_client::PubsubClient, rpc_client::RpcClient},
    rpc_config::{RpcTransactionLogsConfig, RpcTransactionLogsFilter},
    rpc_response::RpcLogsResponse,
};
use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey, signature::Signature};
use solana_transaction_status::{EncodedTransaction, UiMessage, UiTransactionEncoding};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::{
    config::Config,
    dex::DexHandler,
    error::{BotError, Result},
    types::{BotEvent, Dex, PoolInfo},
};

// ── Public entry point ─────────────────────────────────────────

/// Runs forever, reconnecting on errors.
pub async fn run_listener(
    dex: Dex,
    handler: Arc<dyn DexHandler>,
    config: Arc<Config>,
    tx: mpsc::Sender<BotEvent>,
) {
    loop {
        info!("{dex} listener starting…");
        match listen_once(dex, handler.clone(), config.clone(), tx.clone()).await {
            Ok(()) => {}
            Err(e) => {
                error!("{dex} listener error: {e}. Reconnecting in 3s…");
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            }
        }
    }
}

// ── Internal ────────────────────────────────────────────────────

async fn listen_once(
    dex: Dex,
    handler: Arc<dyn DexHandler>,
    config: Arc<Config>,
    tx: mpsc::Sender<BotEvent>,
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

    info!("{dex} listener connected — watching {}", dex.program_id());

    while let Some(response) = stream.next().await {
        let log_resp: RpcLogsResponse = response.value;

        if log_resp.err.is_some() {
            continue;
        }

        let init_log = handler.pool_init_log();
        if !log_resp.logs.iter().any(|l| l.contains(init_log)) {
            continue;
        }

        debug!("{dex} pool-init tx: {}", log_resp.signature);

        let handler_clone = handler.clone();
        let config_clone = config.clone();
        let tx_clone = tx.clone();
        let sig = log_resp.signature.clone();

        tokio::spawn(async move {
            match fetch_and_parse(dex, handler_clone.as_ref(), &config_clone, &sig).await {
                Ok(pool) => {
                    info!(
                        "{} new pool  pool={}  base={}  quote={}  tx={}",
                        pool.dex, pool.pool, pool.base_mint, pool.quote_mint, pool.tx_signature
                    );
                    let _ = tx_clone.send(BotEvent::NewPool(Box::new(pool))).await;
                }
                Err(e) => warn!("{dex} parse failed for {sig}: {e}"),
            }
        });
    }

    Ok(())
}

/// Fetch the confirmed transaction and extract pool info.
async fn fetch_and_parse(
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

    let tx_with_meta = rpc
        .get_transaction(&sig, UiTransactionEncoding::Json)
        .await
        .map_err(BotError::Rpc)?;

    let program_id = dex.program_id();
    let (account_keys, ix_accounts) =
        parse_encoded_tx(tx_with_meta.transaction.transaction, &program_id)?;

    handler.parse_pool(&account_keys, &ix_accounts, signature)
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
                "unsupported tx encoding (use UiTransactionEncoding::Json)".to_string(),
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

    // Parse the flat account-key list
    let account_keys: Vec<Pubkey> = raw_msg
        .account_keys
        .iter()
        .map(|k| k.parse::<Pubkey>().map_err(|e| BotError::Parse(e.to_string())))
        .collect::<Result<Vec<_>>>()?;

    // Find the first instruction belonging to our DEX program
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
            BotError::Parse(format!("no instruction found for program {program_id}"))
        })?;

    Ok((account_keys, ix_accounts))
}
