use anchor_lang::prelude::*;

use crate::{
    constants::{INTENT_SEED, INTENT_STATUS_EXECUTED, INTENT_STATUS_PENDING, VAULT_SEED},
    errors::StealthVaultError,
    state::{ExecutionIntent, Vault},
};

#[derive(Accounts)]
#[instruction(nonce: u64)]
pub struct ExecuteIntent<'info> {
    pub executor: Signer<'info>,
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

pub fn execute_intent_handler(ctx: Context<ExecuteIntent>, _nonce: u64) -> Result<()> {
    let executor = ctx.accounts.executor.key();
    let vault = &ctx.accounts.vault;
    let intent = &mut ctx.accounts.intent;

    require!(
        executor == vault.ephemeral_authority,
        StealthVaultError::UnauthorizedIntentExecution
    );
    require!(
        intent.status == INTENT_STATUS_PENDING,
        StealthVaultError::IntentNotPending
    );

    intent.status = INTENT_STATUS_EXECUTED;
    intent.executor = executor;
    intent.executed_at = Clock::get()?.unix_timestamp;

    Ok(())
}
