use solana_sdk::{pubkey, pubkey::Pubkey};

// ── DEX Program IDs ────────────────────────────────────────────
pub const RAYDIUM_AMM_V4: Pubkey  = pubkey!("675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8");
pub const RAYDIUM_CPMM: Pubkey   = pubkey!("CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C");
pub const RAYDIUM_CLMM: Pubkey   = pubkey!("CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK");
pub const ORCA_WHIRLPOOL: Pubkey = pubkey!("whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc");
pub const METEORA_DLMM: Pubkey   = pubkey!("LBUZKhRxPF3XUpBCjp4YzTKgLe4ofs65zpnUXALkcqN");
pub const METEORA_DAMMV2: Pubkey = pubkey!("cpamdpZCGKUy5JxQXB4dcpGPiikHawvSWAd6mEn1sGG");

// ── Common token mints ─────────────────────────────────────────
pub const WSOL_MINT: Pubkey = pubkey!("So11111111111111111111111111111111111111112");
pub const USDC_MINT: Pubkey = pubkey!("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");
pub const USDT_MINT: Pubkey = pubkey!("Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB");

// ── System programs ────────────────────────────────────────────
pub const SPL_TOKEN_PROGRAM: Pubkey     = pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
pub const SPL_TOKEN_2022: Pubkey        = pubkey!("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");
pub const ASSOCIATED_TOKEN_PROGRAM: Pubkey = pubkey!("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJe1bD");
pub const OPENBOOK_PROGRAM: Pubkey      = pubkey!("srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX");

// ── Raydium AMM V4 – account indices inside initialize2 ix ────
pub const AMM_V4_INIT_POOL_IDX:  usize = 4;
pub const AMM_V4_INIT_COIN_MINT: usize = 8;
pub const AMM_V4_INIT_PC_MINT:   usize = 9;
// instruction type byte
pub const AMM_V4_SWAP_BASE_IN: u8 = 9;

// ── Raydium CPMM – account indices inside initialize ix ────────
pub const CPMM_INIT_POOL_IDX:   usize = 3;
pub const CPMM_INIT_MINT0_IDX:  usize = 4;
pub const CPMM_INIT_MINT1_IDX:  usize = 5;

// ── Orca Whirlpool – account indices inside initialize_pool ix ─
pub const ORCA_INIT_POOL_IDX:   usize = 4;
pub const ORCA_INIT_MINT_A_IDX: usize = 1;
pub const ORCA_INIT_MINT_B_IDX: usize = 2;
pub const ORCA_TICKS_PER_ARRAY: i32   = 88;

// ── Meteora DLMM – account indices inside initialize_lb_pair ix ─
pub const DLMM_INIT_POOL_IDX:   usize = 0;
pub const DLMM_INIT_MINT_X_IDX: usize = 2;
pub const DLMM_INIT_MINT_Y_IDX: usize = 3;

// ── Meteora DAMMv2 – account indices inside initialize ix ──────
pub const DAMMV2_INIT_POOL_IDX:  usize = 0;
pub const DAMMV2_INIT_MINT_A_IDX: usize = 3;
pub const DAMMV2_INIT_MINT_B_IDX: usize = 4;

// ── Jito tip accounts (rotate for load balancing) ─────────────
pub const JITO_TIP_ACCOUNTS: [&str; 4] = [
    "96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5",
    "HFqU5x63VTqvQss8hp11i4wVV8bD44PvwucfZ2bU7gRe",
    "Cw8CFyM9FkoMi7K7Crf6HNQqf4uEMzpKw6QNghXLvLkY",
    "ADaUMid9yfUytqMBgopwjb2DTLSokTSzL1zt13X5ta1R",
];

// ── Anchor discriminator helper ────────────────────────────────
/// Returns the first 8 bytes of SHA-256("global:<name>") — the Anchor instruction discriminator.
pub fn anchor_discriminator(name: &str) -> [u8; 8] {
    use sha2::{Digest, Sha256};
    let preimage = format!("global:{name}");
    let hash = Sha256::digest(preimage.as_bytes());
    hash[..8].try_into().expect("slice too short")
}
