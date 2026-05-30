use anchor_lang::prelude::*;

use crate::{
    constants::{INTENT_SEED, INTENT_STATUS_PENDING, VAULT_SEED},
    errors::StealthVaultError,
    state::{ExecutionIntent, Vault},
};

#[derive(Accounts)]
#[instruction(nonce: u64)]
pub struct SubmitExecutionIntent<'info> {
    #[account(mut)]
    pub ephemeral_authority: Signer<'info>,
    #[account(
        seeds = [VAULT_SEED, vault.owner.as_ref()],
        bump = vault.bump,
        constraint = vault.ephemeral_authority == ephemeral_authority.key()
            @ StealthVaultError::InvalidEphemeralAuthority
    )]
    pub vault: Account<'info, Vault>,
    #[account(
        init,
        payer = ephemeral_authority,
        space = 8 + ExecutionIntent::INIT_SPACE,
        seeds = [INTENT_SEED, vault.key().as_ref(), &nonce.to_le_bytes()],
        bump
    )]
    pub intent: Account<'info, ExecutionIntent>,
    pub system_program: Program<'info, System>,
}

pub fn submit_execution_intent_handler(
    ctx: Context<SubmitExecutionIntent>,
    nonce: u64,
    payload_hash: [u8; 32],
) -> Result<()> {
    let intent = &mut ctx.accounts.intent;

    intent.vault = ctx.accounts.vault.key();
    intent.ephemeral_authority = ctx.accounts.ephemeral_authority.key();
    intent.executor = Pubkey::default();
    intent.nonce = nonce;
    intent.payload_hash = payload_hash;
    intent.status = INTENT_STATUS_PENDING;
    intent.created_at = Clock::get()?.unix_timestamp;
    intent.cancelled_at = 0;
    intent.executed_at = 0;
    intent.bump = ctx.bumps.intent;

    Ok(())
}
