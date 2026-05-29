use anchor_lang::prelude::*;

use crate::{constants::VAULT_SEED, state::Vault};

#[derive(Accounts)]
pub struct InitializeVault<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,
    #[account(
        init,
        payer = owner,
        space = 8 + Vault::INIT_SPACE,
        seeds = [VAULT_SEED, owner.key().as_ref()],
        bump
    )]
    pub vault: Account<'info, Vault>,
    pub system_program: Program<'info, System>,
}

pub fn initialize_vault_handler(
    ctx: Context<InitializeVault>,
    ephemeral_authority: Pubkey,
) -> Result<()> {
    let vault = &mut ctx.accounts.vault;

    vault.owner = ctx.accounts.owner.key();
    vault.ephemeral_authority = ephemeral_authority;
    vault.bump = ctx.bumps.vault;

    Ok(())
}
