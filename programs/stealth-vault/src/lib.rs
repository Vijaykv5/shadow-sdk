use anchor_lang::prelude::*;

pub mod constants;
pub mod errors;
pub mod instructions;
pub mod state;

pub use constants::*;
pub use errors::*;
pub use instructions::*;
pub use state::*;

declare_id!("4XmHzu3kxf3oyD2bchUmkDKoq2QADHkP13Zcv1hsS5X5");

#[program]
pub mod stealth_vault {
    use super::*;

    pub fn initialize_vault(
        ctx: Context<InitializeVault>,
        ephemeral_authority: Pubkey,
    ) -> Result<()> {
        instructions::initialize_vault::initialize_vault_handler(ctx, ephemeral_authority)
    }

    pub fn rotate_authority(
        ctx: Context<RotateAuthority>,
        new_ephemeral_authority: Pubkey,
    ) -> Result<()> {
        instructions::rotate_authority::rotate_authority_handler(ctx, new_ephemeral_authority)
    }

    pub fn submit_execution_intent(
        ctx: Context<SubmitExecutionIntent>,
        nonce: u64,
        payload_hash: [u8; 32],
    ) -> Result<()> {
        instructions::submit_execution_intent::submit_execution_intent_handler(
            ctx,
            nonce,
            payload_hash,
        )
    }

    pub fn cancel_intent(ctx: Context<CancelIntent>, nonce: u64) -> Result<()> {
        instructions::cancel_intent::cancel_intent_handler(ctx, nonce)
    }

    pub fn execute_intent(ctx: Context<ExecuteIntent>, nonce: u64) -> Result<()> {
        instructions::execute_intent::execute_intent_handler(ctx, nonce)
    }
}
