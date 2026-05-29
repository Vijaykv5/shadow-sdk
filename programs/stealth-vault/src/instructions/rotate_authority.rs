use anchor_lang::prelude::*;

use crate::{constants::VAULT_SEED, state::Vault};

#[derive(Accounts)]
pub struct RotateAuthority<'info> {
    pub owner: Signer<'info>,
    #[account(
        mut,
        seeds = [VAULT_SEED, owner.key().as_ref()],
        bump = vault.bump,
        has_one = owner
    )]
    pub vault: Account<'info, Vault>,
}

pub fn rotate_authority_handler(
    ctx: Context<RotateAuthority>,
    new_ephemeral_authority: Pubkey,
) -> Result<()> {
    ctx.accounts.vault.ephemeral_authority = new_ephemeral_authority;

    Ok(())
}
