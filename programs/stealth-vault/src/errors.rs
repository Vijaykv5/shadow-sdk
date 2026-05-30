use anchor_lang::prelude::*;

#[error_code]
pub enum StealthVaultError {
    #[msg("Signer is not the vault's current ephemeral authority")]
    InvalidEphemeralAuthority,
    #[msg("Signer is not allowed to cancel this intent")]
    UnauthorizedIntentCancellation,
    #[msg("Signer is not allowed to execute this intent")]
    UnauthorizedIntentExecution,
    #[msg("Intent is not pending")]
    IntentNotPending,
}
