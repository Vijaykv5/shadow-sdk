use anyhow::{Context, Result};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signature, Signer},
    transaction::Transaction,
};

pub use stealth_vault::constants::{
    INTENT_SEED, INTENT_STATUS_CANCELLED, INTENT_STATUS_EXECUTED, INTENT_STATUS_PENDING, VAULT_SEED,
};

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

#[derive(Debug, Clone)]
pub struct SubmitExecutionIntentResult {
    pub signature: Signature,
    pub vault: Pubkey,
    pub intent: Pubkey,
    pub owner: Pubkey,
    pub ephemeral_authority: Pubkey,
    pub nonce: u64,
    pub payload_hash: [u8; 32],
    pub bump: u8,
}

#[derive(Debug, Clone)]
pub struct CancelIntentResult {
    pub signature: Signature,
    pub vault: Pubkey,
    pub intent: Pubkey,
    pub owner: Pubkey,
    pub authority: Pubkey,
    pub nonce: u64,
}

#[derive(Debug, Clone)]
pub struct ExecuteIntentResult {
    pub signature: Signature,
    pub vault: Pubkey,
    pub intent: Pubkey,
    pub owner: Pubkey,
    pub executor: Pubkey,
    pub nonce: u64,
}

pub fn derive_vault_pda(owner: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[VAULT_SEED, owner.as_ref()], &stealth_vault::ID)
}

pub fn derive_intent_pda(vault: &Pubkey, nonce: u64) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[INTENT_SEED, vault.as_ref(), &nonce.to_le_bytes()],
        &stealth_vault::ID,
    )
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

pub fn submit_execution_intent_instruction(
    owner: Pubkey,
    ephemeral_authority: Pubkey,
    nonce: u64,
    payload_hash: [u8; 32],
) -> Instruction {
    let (vault, _) = derive_vault_pda(&owner);
    let (intent, _) = derive_intent_pda(&vault, nonce);

    Instruction {
        program_id: stealth_vault::ID,
        accounts: anchor_lang::ToAccountMetas::to_account_metas(
            &stealth_vault::accounts::SubmitExecutionIntent {
                ephemeral_authority,
                vault,
                intent,
                system_program: anchor_lang::system_program::ID,
            },
            None,
        ),
        data: anchor_lang::InstructionData::data(
            &stealth_vault::instruction::SubmitExecutionIntent {
                nonce,
                payload_hash,
            },
        ),
    }
}

pub fn cancel_intent_instruction(owner: Pubkey, authority: Pubkey, nonce: u64) -> Instruction {
    let (vault, _) = derive_vault_pda(&owner);
    let (intent, _) = derive_intent_pda(&vault, nonce);

    Instruction {
        program_id: stealth_vault::ID,
        accounts: anchor_lang::ToAccountMetas::to_account_metas(
            &stealth_vault::accounts::CancelIntent {
                authority,
                vault,
                intent,
            },
            None,
        ),
        data: anchor_lang::InstructionData::data(&stealth_vault::instruction::CancelIntent {
            nonce,
        }),
    }
}

pub fn execute_intent_instruction(owner: Pubkey, executor: Pubkey, nonce: u64) -> Instruction {
    let (vault, _) = derive_vault_pda(&owner);
    let (intent, _) = derive_intent_pda(&vault, nonce);

    Instruction {
        program_id: stealth_vault::ID,
        accounts: anchor_lang::ToAccountMetas::to_account_metas(
            &stealth_vault::accounts::ExecuteIntent {
                executor,
                vault,
                intent,
            },
            None,
        ),
        data: anchor_lang::InstructionData::data(&stealth_vault::instruction::ExecuteIntent {
            nonce,
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

pub fn submit_execution_intent(
    rpc_client: &RpcClient,
    owner: Pubkey,
    ephemeral_authority: &Keypair,
    nonce: u64,
    payload_hash: [u8; 32],
) -> Result<SubmitExecutionIntentResult> {
    let ephemeral_authority_pubkey = ephemeral_authority.pubkey();
    let (vault, _) = derive_vault_pda(&owner);
    let (intent, bump) = derive_intent_pda(&vault, nonce);
    let instruction =
        submit_execution_intent_instruction(owner, ephemeral_authority_pubkey, nonce, payload_hash);
    let recent_blockhash = rpc_client
        .get_latest_blockhash()
        .context("failed to fetch latest blockhash")?;
    let transaction = Transaction::new_signed_with_payer(
        &[instruction],
        Some(&ephemeral_authority_pubkey),
        &[ephemeral_authority],
        recent_blockhash,
    );

    let signature = rpc_client
        .send_and_confirm_transaction_with_spinner_and_commitment(
            &transaction,
            CommitmentConfig::confirmed(),
        )
        .context("failed to send and confirm submit execution intent transaction")?;

    Ok(SubmitExecutionIntentResult {
        signature,
        vault,
        intent,
        owner,
        ephemeral_authority: ephemeral_authority_pubkey,
        nonce,
        payload_hash,
        bump,
    })
}

pub fn cancel_intent(
    rpc_client: &RpcClient,
    owner: Pubkey,
    authority: &Keypair,
    nonce: u64,
) -> Result<CancelIntentResult> {
    let authority_pubkey = authority.pubkey();
    let (vault, _) = derive_vault_pda(&owner);
    let (intent, _) = derive_intent_pda(&vault, nonce);
    let instruction = cancel_intent_instruction(owner, authority_pubkey, nonce);
    let recent_blockhash = rpc_client
        .get_latest_blockhash()
        .context("failed to fetch latest blockhash")?;
    let transaction = Transaction::new_signed_with_payer(
        &[instruction],
        Some(&authority_pubkey),
        &[authority],
        recent_blockhash,
    );

    let signature = rpc_client
        .send_and_confirm_transaction_with_spinner_and_commitment(
            &transaction,
            CommitmentConfig::confirmed(),
        )
        .context("failed to send and confirm cancel intent transaction")?;

    Ok(CancelIntentResult {
        signature,
        vault,
        intent,
        owner,
        authority: authority_pubkey,
        nonce,
    })
}

pub fn execute_intent(
    rpc_client: &RpcClient,
    owner: Pubkey,
    executor: &Keypair,
    nonce: u64,
) -> Result<ExecuteIntentResult> {
    let executor_pubkey = executor.pubkey();
    let (vault, _) = derive_vault_pda(&owner);
    let (intent, _) = derive_intent_pda(&vault, nonce);
    let instruction = execute_intent_instruction(owner, executor_pubkey, nonce);
    let recent_blockhash = rpc_client
        .get_latest_blockhash()
        .context("failed to fetch latest blockhash")?;
    let transaction = Transaction::new_signed_with_payer(
        &[instruction],
        Some(&executor_pubkey),
        &[executor],
        recent_blockhash,
    );

    let signature = rpc_client
        .send_and_confirm_transaction_with_spinner_and_commitment(
            &transaction,
            CommitmentConfig::confirmed(),
        )
        .context("failed to send and confirm execute intent transaction")?;

    Ok(ExecuteIntentResult {
        signature,
        vault,
        intent,
        owner,
        executor: executor_pubkey,
        nonce,
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

    #[test]
    fn derives_same_intent_for_same_vault_and_nonce() {
        let vault = Pubkey::new_unique();
        let nonce = 42;

        let first = derive_intent_pda(&vault, nonce);
        let second = derive_intent_pda(&vault, nonce);

        assert_eq!(first, second);
    }

    #[test]
    fn submit_intent_instruction_targets_derived_accounts() {
        let owner = Pubkey::new_unique();
        let ephemeral_authority = Pubkey::new_unique();
        let nonce = 7;
        let payload_hash = [9; 32];
        let (vault, _) = derive_vault_pda(&owner);
        let (intent, _) = derive_intent_pda(&vault, nonce);

        let instruction =
            submit_execution_intent_instruction(owner, ephemeral_authority, nonce, payload_hash);

        assert_eq!(instruction.program_id, stealth_vault::ID);
        assert_eq!(instruction.accounts[0].pubkey, ephemeral_authority);
        assert_eq!(instruction.accounts[1].pubkey, vault);
        assert_eq!(instruction.accounts[2].pubkey, intent);
    }

    #[test]
    fn cancel_intent_instruction_targets_derived_accounts() {
        let owner = Pubkey::new_unique();
        let authority = Pubkey::new_unique();
        let nonce = 11;
        let (vault, _) = derive_vault_pda(&owner);
        let (intent, _) = derive_intent_pda(&vault, nonce);

        let instruction = cancel_intent_instruction(owner, authority, nonce);

        assert_eq!(instruction.program_id, stealth_vault::ID);
        assert_eq!(instruction.accounts[0].pubkey, authority);
        assert_eq!(instruction.accounts[1].pubkey, vault);
        assert_eq!(instruction.accounts[2].pubkey, intent);
    }

    #[test]
    fn execute_intent_instruction_targets_derived_accounts() {
        let owner = Pubkey::new_unique();
        let executor = Pubkey::new_unique();
        let nonce = 12;
        let (vault, _) = derive_vault_pda(&owner);
        let (intent, _) = derive_intent_pda(&vault, nonce);

        let instruction = execute_intent_instruction(owner, executor, nonce);

        assert_eq!(instruction.program_id, stealth_vault::ID);
        assert_eq!(instruction.accounts[0].pubkey, executor);
        assert_eq!(instruction.accounts[1].pubkey, vault);
        assert_eq!(instruction.accounts[2].pubkey, intent);
    }
}
