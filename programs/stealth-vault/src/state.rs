use anchor_lang::prelude::*;

#[account]
#[derive(InitSpace)]
pub struct Vault {
    pub owner: Pubkey,
    pub ephemeral_authority: Pubkey,
    pub bump: u8,
}
