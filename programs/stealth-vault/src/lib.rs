use anchor_lang::prelude::*;

pub mod constants;
pub mod instructions;
pub mod state;

pub use constants::*;
pub use instructions::*;
pub use state::*;

declare_id!("4BrQYfhKFkKnCZAtEakMhqMgEVzevoq9j7wUEEPqSEaA");

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
}
