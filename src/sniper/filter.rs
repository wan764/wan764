//! Pool and token filters — gates each detected pool before we snipe it.

use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::commitment_config::CommitmentConfig;
use tracing::debug;

use crate::{
    config::Config,
    error::{BotError, Result},
    types::PoolInfo,
    utils::parse_mint,
};

pub struct Filter {
    config: Config,
    rpc: RpcClient,
}

impl Filter {
    pub fn new(config: Config) -> Self {
        let rpc =
            RpcClient::new_with_commitment(config.rpc_url.clone(), CommitmentConfig::confirmed());
        Self { config, rpc }
    }

    pub async fn check(&self, pool: &PoolInfo) -> Result<()> {
        self.check_target_token(pool)?;
        self.check_quote_token(pool)?;
        if self.config.reject_mint_authority || self.config.reject_freeze_authority {
            self.check_token_authority(pool).await?;
        }
        Ok(())
    }

    /// If the user configured a target token, reject every pool that
    /// does not contain that exact mint on either side.
    fn check_target_token(&self, pool: &PoolInfo) -> Result<()> {
        if let Some(target) = &self.config.target_token_mint {
            if pool.base_mint != *target && pool.quote_mint != *target {
                return Err(BotError::FilterRejected(format!(
                    "pool does not contain target token {}",
                    target
                )));
            }
        }
        Ok(())
    }

    fn check_quote_token(&self, pool: &PoolInfo) -> Result<()> {
        let has_known_quote = self.config.quote_mints.contains(&pool.quote_mint)
            || self.config.quote_mints.contains(&pool.base_mint);
        if !has_known_quote {
            return Err(BotError::FilterRejected(format!(
                "neither {} nor {} is in the quote-mint allowlist",
                pool.base_mint, pool.quote_mint
            )));
        }
        Ok(())
    }

    async fn check_token_authority(&self, pool: &PoolInfo) -> Result<()> {
        // The "new" token is whichever mint is NOT in our known-quote list
        let new_mint = if self.config.quote_mints.contains(&pool.quote_mint) {
            &pool.base_mint
        } else {
            &pool.quote_mint
        };

        let account = self
            .rpc
            .get_account(new_mint)
            .await
            .map_err(BotError::Rpc)?;

        let mint = parse_mint(&account.data)
            .ok_or_else(|| BotError::Parse(format!("cannot parse mint {new_mint}")))?;

        if self.config.reject_mint_authority && mint.mint_authority.is_some() {
            return Err(BotError::FilterRejected(format!(
                "token {new_mint} has active mint authority"
            )));
        }
        if self.config.reject_freeze_authority && mint.freeze_authority.is_some() {
            return Err(BotError::FilterRejected(format!(
                "token {new_mint} has freeze authority"
            )));
        }

        debug!("token {new_mint} passed authority checks");
        Ok(())
    }
}
