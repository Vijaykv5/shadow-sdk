use anchor_lang::prelude::*;

#[account]
#[derive(InitSpace)]
pub struct Vault {
    pub owner: Pubkey,
    pub ephemeral_authority: Pubkey,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct ExecutionIntent {
    pub vault: Pubkey,
    pub ephemeral_authority: Pubkey,
    pub executor: Pubkey,
    pub nonce: u64,
    pub payload_hash: [u8; 32],
    pub status: u8,
    pub created_at: i64,
    pub cancelled_at: i64,
    pub executed_at: i64,
    pub bump: u8,
}
