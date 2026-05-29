use anyhow::{Context, Result};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signature, Signer},
    transaction::Transaction,
};

pub use stealth_vault::constants::VAULT_SEED;

#[derive(Debug, Clone)]
pub struct CreateVaultResult {
    pub signature: Signature,
    pub vault: Pubkey,
    pub owner: Pubkey,
    pub ephemeral_authority: Pubkey,
    pub bump: u8,
}

#[derive(Debug, Clone)]
pub struct RotateAuthorityResult {
    pub signature: Signature,
    pub vault: Pubkey,
    pub owner: Pubkey,
    pub new_ephemeral_authority: Pubkey,
}

pub fn derive_vault_pda(owner: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[VAULT_SEED, owner.as_ref()], &stealth_vault::ID)
}

pub fn initialize_vault_instruction(owner: Pubkey, ephemeral_authority: Pubkey) -> Instruction {
    let (vault, _) = derive_vault_pda(&owner);

    Instruction {
        program_id: stealth_vault::ID,
        accounts: anchor_lang::ToAccountMetas::to_account_metas(
            &stealth_vault::accounts::InitializeVault {
                owner,
                vault,
                system_program: anchor_lang::system_program::ID,
            },
            None,
        ),
        data: anchor_lang::InstructionData::data(&stealth_vault::instruction::InitializeVault {
            ephemeral_authority,
        }),
    }
}

pub fn rotate_authority_instruction(owner: Pubkey, new_ephemeral_authority: Pubkey) -> Instruction {
    let (vault, _) = derive_vault_pda(&owner);

    Instruction {
        program_id: stealth_vault::ID,
        accounts: anchor_lang::ToAccountMetas::to_account_metas(
            &stealth_vault::accounts::RotateAuthority { owner, vault },
            None,
        ),
        data: anchor_lang::InstructionData::data(&stealth_vault::instruction::RotateAuthority {
            new_ephemeral_authority,
        }),
    }
}

pub fn create_vault(
    rpc_client: &RpcClient,
    owner: &Keypair,
    ephemeral_authority: Pubkey,
) -> Result<CreateVaultResult> {
    let owner_pubkey = owner.pubkey();
    let (vault, bump) = derive_vault_pda(&owner_pubkey);
    let instruction = initialize_vault_instruction(owner_pubkey, ephemeral_authority);
    let recent_blockhash = rpc_client
        .get_latest_blockhash()
        .context("failed to fetch latest blockhash")?;
    let transaction = Transaction::new_signed_with_payer(
        &[instruction],
        Some(&owner_pubkey),
        &[owner],
        recent_blockhash,
    );

    let signature = rpc_client
        .send_and_confirm_transaction_with_spinner_and_commitment(
            &transaction,
            CommitmentConfig::confirmed(),
        )
        .context("failed to send and confirm create vault transaction")?;

    Ok(CreateVaultResult {
        signature,
        vault,
        owner: owner_pubkey,
        ephemeral_authority,
        bump,
    })
}

pub fn rotate_authority(
    rpc_client: &RpcClient,
    owner: &Keypair,
    new_ephemeral_authority: Pubkey,
) -> Result<RotateAuthorityResult> {
    let owner_pubkey = owner.pubkey();
    let (vault, _) = derive_vault_pda(&owner_pubkey);
    let instruction = rotate_authority_instruction(owner_pubkey, new_ephemeral_authority);
    let recent_blockhash = rpc_client
        .get_latest_blockhash()
        .context("failed to fetch latest blockhash")?;
    let transaction = Transaction::new_signed_with_payer(
        &[instruction],
        Some(&owner_pubkey),
        &[owner],
        recent_blockhash,
    );

    let signature = rpc_client
        .send_and_confirm_transaction_with_spinner_and_commitment(
            &transaction,
            CommitmentConfig::confirmed(),
        )
        .context("failed to send and confirm rotate authority transaction")?;

    Ok(RotateAuthorityResult {
        signature,
        vault,
        owner: owner_pubkey,
        new_ephemeral_authority,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_same_vault_for_same_owner() {
        let owner = Pubkey::new_unique();

        let first = derive_vault_pda(&owner);
        let second = derive_vault_pda(&owner);

        assert_eq!(first, second);
    }

    #[test]
    fn initialize_instruction_targets_derived_vault() {
        let owner = Pubkey::new_unique();
        let ephemeral_authority = Pubkey::new_unique();
        let (vault, _) = derive_vault_pda(&owner);

        let instruction = initialize_vault_instruction(owner, ephemeral_authority);

        assert_eq!(instruction.program_id, stealth_vault::ID);
        assert_eq!(instruction.accounts[0].pubkey, owner);
        assert_eq!(instruction.accounts[1].pubkey, vault);
    }

    #[test]
    fn rotate_instruction_targets_derived_vault() {
        let owner = Pubkey::new_unique();
        let new_ephemeral_authority = Pubkey::new_unique();
        let (vault, _) = derive_vault_pda(&owner);

        let instruction = rotate_authority_instruction(owner, new_ephemeral_authority);

        assert_eq!(instruction.program_id, stealth_vault::ID);
        assert_eq!(instruction.accounts[0].pubkey, owner);
        assert_eq!(instruction.accounts[1].pubkey, vault);
    }
}
