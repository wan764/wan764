//! Trade executor — builds, signs and sends buy/sell transactions.

use std::sync::Arc;

use reqwest::Client as HttpClient;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    compute_budget::ComputeBudgetInstruction,
    instruction::Instruction,
    message::Message,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::Transaction,
};
use tracing::{info, warn};

use crate::{
    config::Config,
    constants::JITO_TIP_ACCOUNTS,
    dex::DexHandler,
    error::{BotError, Result},
    types::PoolInfo,
    utils::{ata_address, create_ata_idempotent_ix, lamports_to_sol},
};

pub struct Executor {
    pub config: Arc<Config>,
    rpc: RpcClient,
    http: HttpClient,
}

impl Executor {
    pub fn new(config: Arc<Config>) -> Self {
        let rpc = RpcClient::new_with_commitment(
            config.rpc_url.clone(),
            CommitmentConfig::confirmed(),
        );
        Self {
            config,
            rpc,
            http: HttpClient::new(),
        }
    }

    /// Execute a buy. Returns (tx_signature, base_token_amount_received).
    pub async fn buy(
        &self,
        pool: &PoolInfo,
        handler: &Arc<dyn DexHandler>,
        keypair: &Keypair,
    ) -> Result<(String, u64)> {
        let user = keypair.pubkey();
        let amount_in = self.config.buy_amount_lamports;

        let base_mint = if self.config.quote_mints.contains(&pool.quote_mint) {
            pool.base_mint
        } else {
            pool.quote_mint
        };

        let mut ixs: Vec<Instruction> = vec![
            ComputeBudgetInstruction::set_compute_unit_price(self.config.compute_unit_price),
            ComputeBudgetInstruction::set_compute_unit_limit(self.config.compute_unit_limit),
            create_ata_idempotent_ix(&user, &user, &base_mint),
        ];

        let swap_ixs = handler.build_swap_ix(pool, &user, amount_in, 0, true)?;
        ixs.extend(swap_ixs);

        if self.config.use_jito {
            ixs.push(Self::jito_tip_ix(&user, self.config.jito_tip_lamports));
        }

        let sig = self.send_transaction(&ixs, keypair).await?;
        info!("BUY  pool={} sig={}", pool.pool, sig);
        Ok((sig, 0))
    }

    /// Execute a sell of `amount` base tokens.
    pub async fn sell(
        &self,
        pool: &PoolInfo,
        handler: &Arc<dyn DexHandler>,
        keypair: &Keypair,
        amount: u64,
    ) -> Result<String> {
        let user = keypair.pubkey();

        let mut ixs: Vec<Instruction> = vec![
            ComputeBudgetInstruction::set_compute_unit_price(self.config.compute_unit_price),
            ComputeBudgetInstruction::set_compute_unit_limit(self.config.compute_unit_limit),
        ];

        let swap_ixs = handler.build_swap_ix(pool, &user, amount, 0, false)?;
        ixs.extend(swap_ixs);

        if self.config.use_jito {
            ixs.push(Self::jito_tip_ix(&user, self.config.jito_tip_lamports));
        }

        let sig = self.send_transaction(&ixs, keypair).await?;
        info!("SELL pool={} sig={}", pool.pool, sig);
        Ok(sig)
    }

    // ── Internals ─────────────────────────────────────────────

    async fn send_transaction(
        &self,
        instructions: &[Instruction],
        keypair: &Keypair,
    ) -> Result<String> {
        let blockhash = self
            .rpc
            .get_latest_blockhash()
            .await
            .map_err(BotError::Rpc)?;

        let message = Message::new(instructions, Some(&keypair.pubkey()));
        let mut tx = Transaction::new_unsigned(message);
        tx.sign(&[keypair], blockhash);

        if self.config.use_jito {
            self.send_jito_bundle(&tx).await
        } else {
            self.rpc
                .send_and_confirm_transaction(&tx)
                .await
                .map(|s| s.to_string())
                .map_err(BotError::Rpc)
        }
    }

    async fn send_jito_bundle(&self, tx: &Transaction) -> Result<String> {
        use base64::{engine::general_purpose::STANDARD, Engine};

        let serialized = bincode::serialize(tx)
            .map_err(|e| BotError::Send(format!("serialize: {e}")))?;
        let encoded = STANDARD.encode(&serialized);

        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "sendBundle",
            "params": [[encoded]]
        });

        let url = format!("{}/api/v1/bundles", self.config.jito_block_engine_url);
        let resp = self
            .http
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| BotError::Send(format!("jito http: {e}")))?;

        let json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| BotError::Send(format!("jito json: {e}")))?;

        if let Some(err) = json.get("error") {
            warn!("Jito bundle error: {err} — falling back to RPC");
            return self
                .rpc
                .send_and_confirm_transaction(tx)
                .await
                .map(|s| s.to_string())
                .map_err(BotError::Rpc);
        }

        json["result"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| BotError::Send("missing result in Jito response".to_string()))
    }

    fn jito_tip_ix(user: &Pubkey, tip_lamports: u64) -> Instruction {
        use std::time::{SystemTime, UNIX_EPOCH};
        let idx = (SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as usize)
            % JITO_TIP_ACCOUNTS.len();
        let tip_account: Pubkey = JITO_TIP_ACCOUNTS[idx].parse().unwrap();
        solana_sdk::system_instruction::transfer(user, &tip_account, tip_lamports)
    }
}
