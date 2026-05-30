use anchor_lang::{AccountDeserialize, InstructionData, ToAccountMetas};
use solana_program_test::{BanksClient, ProgramTest, ProgramTestContext};
use solana_sdk::{
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    system_instruction, system_program,
    transaction::Transaction,
};
use std::path::PathBuf;
use stealth_vault::{
    accounts,
    constants::{
        INTENT_SEED, INTENT_STATUS_CANCELLED, INTENT_STATUS_EXECUTED, INTENT_STATUS_PENDING,
        VAULT_SEED,
    },
    instruction, ExecutionIntent, Vault,
};

fn test_program() -> ProgramTest {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let deploy_dir = manifest_dir.join("../../target/deploy");
    std::env::set_var("BPF_OUT_DIR", deploy_dir);
    let mut program_test = ProgramTest::new("stealth_vault", stealth_vault::ID, None);
    program_test.prefer_bpf(true);
    program_test
}

fn derive_vault(owner: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[VAULT_SEED, owner.as_ref()], &stealth_vault::ID)
}

fn derive_intent(vault: &Pubkey, nonce: u64) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[INTENT_SEED, vault.as_ref(), &nonce.to_le_bytes()],
        &stealth_vault::ID,
    )
}

fn initialize_vault_instruction(owner: Pubkey, ephemeral_authority: Pubkey) -> Instruction {
    let (vault, _) = derive_vault(&owner);

    Instruction {
        program_id: stealth_vault::ID,
        accounts: accounts::InitializeVault {
            owner,
            vault,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
        data: instruction::InitializeVault {
            ephemeral_authority,
        }
        .data(),
    }
}

fn submit_intent_instruction(
    owner: Pubkey,
    ephemeral_authority: Pubkey,
    nonce: u64,
    payload_hash: [u8; 32],
) -> Instruction {
    let (vault, _) = derive_vault(&owner);
    let (intent, _) = derive_intent(&vault, nonce);

    Instruction {
        program_id: stealth_vault::ID,
        accounts: accounts::SubmitExecutionIntent {
            ephemeral_authority,
            vault,
            intent,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
        data: instruction::SubmitExecutionIntent {
            nonce,
            payload_hash,
        }
        .data(),
    }
}

fn execute_intent_instruction(owner: Pubkey, executor: Pubkey, nonce: u64) -> Instruction {
    let (vault, _) = derive_vault(&owner);
    let (intent, _) = derive_intent(&vault, nonce);

    Instruction {
        program_id: stealth_vault::ID,
        accounts: accounts::ExecuteIntent {
            executor,
            vault,
            intent,
        }
        .to_account_metas(None),
        data: instruction::ExecuteIntent { nonce }.data(),
    }
}

fn cancel_intent_instruction(owner: Pubkey, authority: Pubkey, nonce: u64) -> Instruction {
    let (vault, _) = derive_vault(&owner);
    let (intent, _) = derive_intent(&vault, nonce);

    Instruction {
        program_id: stealth_vault::ID,
        accounts: accounts::CancelIntent {
            authority,
            vault,
            intent,
        }
        .to_account_metas(None),
        data: instruction::CancelIntent { nonce }.data(),
    }
}

async fn process_instruction(
    context: &mut ProgramTestContext,
    payer: &Keypair,
    instruction: Instruction,
    signers: &[&Keypair],
) -> Result<(), solana_program_test::BanksClientError> {
    let mut all_signers = Vec::with_capacity(signers.len() + 1);
    all_signers.push(payer);
    all_signers.extend_from_slice(signers);

    let transaction = Transaction::new_signed_with_payer(
        &[instruction],
        Some(&payer.pubkey()),
        &all_signers,
        context.last_blockhash,
    );

    context.banks_client.process_transaction(transaction).await
}

async fn fund_keypair(context: &mut ProgramTestContext, recipient: &Keypair) {
    let payer = clone_keypair(&context.payer);
    let payer_pubkey = payer.pubkey();

    process_instruction(
        context,
        &payer,
        system_instruction::transfer(&payer_pubkey, &recipient.pubkey(), 2_000_000_000),
        &[],
    )
    .await
    .unwrap();
}

async fn fetch_vault(banks_client: &mut BanksClient, vault: Pubkey) -> Vault {
    let account = banks_client.get_account(vault).await.unwrap().unwrap();
    let mut data = account.data.as_slice();
    Vault::try_deserialize(&mut data).unwrap()
}

async fn fetch_intent(banks_client: &mut BanksClient, intent: Pubkey) -> ExecutionIntent {
    let account = banks_client.get_account(intent).await.unwrap().unwrap();
    let mut data = account.data.as_slice();
    ExecutionIntent::try_deserialize(&mut data).unwrap()
}

async fn create_vault(context: &mut ProgramTestContext, ephemeral_authority: Pubkey) -> Pubkey {
    let payer = clone_keypair(&context.payer);
    let owner = payer.pubkey();
    let (vault, _) = derive_vault(&owner);
    process_instruction(
        context,
        &payer,
        initialize_vault_instruction(owner, ephemeral_authority),
        &[],
    )
    .await
    .unwrap();
    vault
}

fn clone_keypair(keypair: &Keypair) -> Keypair {
    Keypair::from_bytes(&keypair.to_bytes()).unwrap()
}

async fn submit_intent(
    context: &mut ProgramTestContext,
    owner: Pubkey,
    ephemeral_authority: &Keypair,
    nonce: u64,
) -> Pubkey {
    let (vault, _) = derive_vault(&owner);
    let (intent, _) = derive_intent(&vault, nonce);
    process_instruction(
        context,
        ephemeral_authority,
        submit_intent_instruction(
            owner,
            ephemeral_authority.pubkey(),
            nonce,
            [nonce as u8; 32],
        ),
        &[],
    )
    .await
    .unwrap();
    intent
}

#[tokio::test]
async fn initialize_vault_stores_owner_and_ephemeral_authority() {
    let mut context = test_program().start_with_context().await;
    let ephemeral_authority = Keypair::new();
    let vault = create_vault(&mut context, ephemeral_authority.pubkey()).await;

    let vault_account = fetch_vault(&mut context.banks_client, vault).await;

    assert_eq!(vault_account.owner, context.payer.pubkey());
    assert_eq!(
        vault_account.ephemeral_authority,
        ephemeral_authority.pubkey()
    );
}

#[tokio::test]
async fn submit_intent_creates_pending_intent() {
    let mut context = test_program().start_with_context().await;
    let ephemeral_authority = Keypair::new();
    fund_keypair(&mut context, &ephemeral_authority).await;
    let owner = context.payer.pubkey();
    create_vault(&mut context, ephemeral_authority.pubkey()).await;

    let intent = submit_intent(&mut context, owner, &ephemeral_authority, 1).await;
    let intent_account = fetch_intent(&mut context.banks_client, intent).await;

    assert_eq!(
        intent_account.ephemeral_authority,
        ephemeral_authority.pubkey()
    );
    assert_eq!(intent_account.nonce, 1);
    assert_eq!(intent_account.payload_hash, [1; 32]);
    assert_eq!(intent_account.status, INTENT_STATUS_PENDING);
    assert_eq!(intent_account.cancelled_at, 0);
    assert_eq!(intent_account.executed_at, 0);
}

#[tokio::test]
async fn execute_intent_marks_pending_intent_executed() {
    let mut context = test_program().start_with_context().await;
    let ephemeral_authority = Keypair::new();
    fund_keypair(&mut context, &ephemeral_authority).await;
    let owner = context.payer.pubkey();
    create_vault(&mut context, ephemeral_authority.pubkey()).await;
    let intent = submit_intent(&mut context, owner, &ephemeral_authority, 2).await;

    process_instruction(
        &mut context,
        &ephemeral_authority,
        execute_intent_instruction(owner, ephemeral_authority.pubkey(), 2),
        &[],
    )
    .await
    .unwrap();

    let intent_account = fetch_intent(&mut context.banks_client, intent).await;
    assert_eq!(intent_account.status, INTENT_STATUS_EXECUTED);
    assert_eq!(intent_account.executor, ephemeral_authority.pubkey());
    assert!(intent_account.executed_at > 0);
    assert_eq!(intent_account.cancelled_at, 0);
}

#[tokio::test]
async fn cancel_intent_marks_pending_intent_cancelled() {
    let mut context = test_program().start_with_context().await;
    let ephemeral_authority = Keypair::new();
    fund_keypair(&mut context, &ephemeral_authority).await;
    let owner = context.payer.pubkey();
    create_vault(&mut context, ephemeral_authority.pubkey()).await;
    let intent = submit_intent(&mut context, owner, &ephemeral_authority, 3).await;
    let payer = clone_keypair(&context.payer);
    let payer_pubkey = payer.pubkey();

    process_instruction(
        &mut context,
        &payer,
        cancel_intent_instruction(owner, payer_pubkey, 3),
        &[],
    )
    .await
    .unwrap();

    let intent_account = fetch_intent(&mut context.banks_client, intent).await;
    assert_eq!(intent_account.status, INTENT_STATUS_CANCELLED);
    assert!(intent_account.cancelled_at > 0);
    assert_eq!(intent_account.executed_at, 0);
}

#[tokio::test]
async fn wrong_authority_cannot_submit_or_execute_intent() {
    let mut context = test_program().start_with_context().await;
    let ephemeral_authority = Keypair::new();
    let wrong_authority = Keypair::new();
    fund_keypair(&mut context, &ephemeral_authority).await;
    fund_keypair(&mut context, &wrong_authority).await;
    let owner = context.payer.pubkey();
    create_vault(&mut context, ephemeral_authority.pubkey()).await;

    let submit_result = process_instruction(
        &mut context,
        &wrong_authority,
        submit_intent_instruction(owner, wrong_authority.pubkey(), 4, [4; 32]),
        &[],
    )
    .await;
    assert!(submit_result.is_err());

    submit_intent(&mut context, owner, &ephemeral_authority, 5).await;
    let execute_result = process_instruction(
        &mut context,
        &wrong_authority,
        execute_intent_instruction(owner, wrong_authority.pubkey(), 5),
        &[],
    )
    .await;
    assert!(execute_result.is_err());
}

#[tokio::test]
async fn terminal_intents_cannot_change_state_again() {
    let mut context = test_program().start_with_context().await;
    let ephemeral_authority = Keypair::new();
    fund_keypair(&mut context, &ephemeral_authority).await;
    let owner = context.payer.pubkey();
    create_vault(&mut context, ephemeral_authority.pubkey()).await;

    submit_intent(&mut context, owner, &ephemeral_authority, 6).await;
    let payer = clone_keypair(&context.payer);
    let payer_pubkey = payer.pubkey();
    process_instruction(
        &mut context,
        &payer,
        cancel_intent_instruction(owner, payer_pubkey, 6),
        &[],
    )
    .await
    .unwrap();
    let execute_cancelled_result = process_instruction(
        &mut context,
        &ephemeral_authority,
        execute_intent_instruction(owner, ephemeral_authority.pubkey(), 6),
        &[],
    )
    .await;
    assert!(execute_cancelled_result.is_err());

    submit_intent(&mut context, owner, &ephemeral_authority, 7).await;
    process_instruction(
        &mut context,
        &ephemeral_authority,
        execute_intent_instruction(owner, ephemeral_authority.pubkey(), 7),
        &[],
    )
    .await
    .unwrap();
    let payer = clone_keypair(&context.payer);
    let payer_pubkey = payer.pubkey();
    let cancel_executed_result = process_instruction(
        &mut context,
        &payer,
        cancel_intent_instruction(owner, payer_pubkey, 7),
        &[],
    )
    .await;
    assert!(cancel_executed_result.is_err());
}
