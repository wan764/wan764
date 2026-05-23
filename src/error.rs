use thiserror::Error;

#[derive(Debug, Error)]
pub enum BotError {
    #[error("RPC error: {0}")]
    Rpc(#[from] solana_client::client_error::ClientError),

    #[error("SDK error: {0}")]
    Sdk(String),

    #[error("Websocket error: {0}")]
    Websocket(String),

    #[error("Config error: {0}")]
    Config(String),

    #[error("Wallet error: {0}")]
    Wallet(String),

    #[error("Filter rejected: {0}")]
    FilterRejected(String),

    #[error("Swap build error: {0}")]
    SwapBuild(String),

    #[error("Transaction send error: {0}")]
    Send(String),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, BotError>;
