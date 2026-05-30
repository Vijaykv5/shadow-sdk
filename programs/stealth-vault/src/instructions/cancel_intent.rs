use anchor_lang::prelude::*;

use crate::{
    constants::{INTENT_SEED, INTENT_STATUS_CANCELLED, INTENT_STATUS_PENDING, VAULT_SEED},
    errors::StealthVaultError,
    state::{ExecutionIntent, Vault},
};

#[derive(Accounts)]
#[instruction(nonce: u64)]
pub struct CancelIntent<'info> {
    pub authority: Signer<'info>,
    #[account(
        seeds = [VAULT_SEED, vault.owner.as_ref()],
        bump = vault.bump
    )]
    pub vault: Account<'info, Vault>,
    #[account(
        mut,
        seeds = [INTENT_SEED, vault.key().as_ref(), &nonce.to_le_bytes()],
        bump = intent.bump,
        constraint = intent.vault == vault.key()
    )]
    pub intent: Account<'info, ExecutionIntent>,
}

pub fn cancel_intent_handler(ctx: Context<CancelIntent>, _nonce: u64) -> Result<()> {
    let authority = ctx.accounts.authority.key();
    let vault = &ctx.accounts.vault;
    let intent = &mut ctx.accounts.intent;

    require!(
        authority == vault.owner || authority == vault.ephemeral_authority,
        StealthVaultError::UnauthorizedIntentCancellation
    );
    require!(
        intent.status == INTENT_STATUS_PENDING,
        StealthVaultError::IntentNotPending
    );

    intent.status = INTENT_STATUS_CANCELLED;
    intent.cancelled_at = Clock::get()?.unix_timestamp;

    Ok(())
}
