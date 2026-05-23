use crate::error::{BotError, Result};
use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signer},
};

pub struct Wallet {
    keypair: Keypair,
}

impl Wallet {
    pub fn from_base58(private_key: &str) -> Result<Self> {
        let bytes = bs58::decode(private_key)
            .into_vec()
            .map_err(|e| BotError::Wallet(format!("base58 decode: {e}")))?;
        let keypair =
            Keypair::try_from(bytes.as_slice()).map_err(|e| BotError::Wallet(format!("keypair: {e}")))?;
        Ok(Self { keypair })
    }

    pub fn pubkey(&self) -> Pubkey {
        self.keypair.pubkey()
    }

    pub fn keypair(&self) -> &Keypair {
        &self.keypair
    }
}
