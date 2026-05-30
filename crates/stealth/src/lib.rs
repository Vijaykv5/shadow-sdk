//! Rust SDK for Shadow SDK stealth vaults and hashed execution intents.
//!
//! This crate gives Solana clients a small API for deriving Shadow PDAs,
//! building instructions, hashing private payload bytes, and submitting intent
//! transactions. The private payload should move off-chain to a relayer; only
//! its 32-byte hash is committed on-chain.
//!
//! ```no_run
//! use shadow_stealth::{create_vault, submit_private_intent, PrivateIntent};
//! use solana_client::rpc_client::RpcClient;
//! use solana_sdk::signature::{read_keypair_file, Signer};
//!
//! # fn main() -> anyhow::Result<()> {
//! let rpc = RpcClient::new("https://api.devnet.solana.com".to_string());
//! let owner = read_keypair_file("~/.config/solana/id.json")
//!     .map_err(|err| anyhow::anyhow!("failed to read owner keypair: {err}"))?;
//! let ephemeral = read_keypair_file("~/.config/solana/id.json")
//!     .map_err(|err| anyhow::anyhow!("failed to read ephemeral keypair: {err}"))?;
//! let intent = PrivateIntent::from_bytes(1, br#"{"nonce":1,"kind":"mock_execution"}"#);
//!
//! create_vault(&rpc, &owner, ephemeral.pubkey())?;
//! submit_private_intent(&rpc, owner.pubkey(), &ephemeral, &intent)?;
//! # Ok(())
//! # }
//! ```

use anyhow::{Context, Result};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    hash::hash,
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::{Keypair, Signature, Signer},
    system_program,
    transaction::Transaction,
};

pub const STEALTH_VAULT_PROGRAM_ID_STRING: &str = "4XmHzu3kxf3oyD2bchUmkDKoq2QADHkP13Zcv1hsS5X5";
pub const VAULT_SEED: &[u8] = b"vault";
pub const INTENT_SEED: &[u8] = b"intent";
pub const INTENT_STATUS_PENDING: u8 = 0;
pub const INTENT_STATUS_CANCELLED: u8 = 1;
pub const INTENT_STATUS_EXECUTED: u8 = 2;

const INITIALIZE_VAULT_DISCRIMINATOR: [u8; 8] = [48, 191, 163, 44, 71, 129, 63, 164];
const ROTATE_AUTHORITY_DISCRIMINATOR: [u8; 8] = [248, 225, 151, 35, 28, 15, 85, 12];
const SUBMIT_EXECUTION_INTENT_DISCRIMINATOR: [u8; 8] = [144, 64, 85, 209, 247, 3, 129, 47];
const CANCEL_INTENT_DISCRIMINATOR: [u8; 8] = [67, 73, 238, 244, 208, 89, 225, 59];
const EXECUTE_INTENT_DISCRIMINATOR: [u8; 8] = [53, 130, 47, 154, 227, 220, 122, 212];

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

/// Private off-chain payload plus the hash committed on-chain.
///
/// The payload bytes are intentionally retained so applications can hand the
/// same exact bytes to a relayer, while only [`payload_hash`](Self::payload_hash)
/// is submitted to the Shadow vault program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivateIntent {
    pub nonce: u64,
    pub payload_bytes: Vec<u8>,
    pub payload_hash: [u8; 32],
}

impl PrivateIntent {
    /// Build a private intent from exact payload bytes.
    pub fn from_bytes(nonce: u64, payload_bytes: impl Into<Vec<u8>>) -> Self {
        let payload_bytes = payload_bytes.into();
        let payload_hash = hash_payload_bytes(&payload_bytes);

        Self {
            nonce,
            payload_bytes,
            payload_hash,
        }
    }

    /// Build a private intent from compact JSON bytes.
    ///
    /// If exact byte formatting matters, prefer [`from_bytes`](Self::from_bytes)
    /// with the bytes your frontend or payload file produced.
    pub fn from_json(nonce: u64, value: &serde_json::Value) -> Result<Self> {
        let (payload_hash, payload_bytes) = hash_payload_json(value)?;

        Ok(Self {
            nonce,
            payload_bytes,
            payload_hash,
        })
    }

    /// Return the payload hash as lowercase hex.
    pub fn hash_hex(&self) -> String {
        payload_hash_to_hex(&self.payload_hash)
    }
}

/// Return the Shadow vault program id.
pub fn stealth_vault_program_id() -> Pubkey {
    STEALTH_VAULT_PROGRAM_ID_STRING
        .parse()
        .expect("static Shadow program id must be valid")
}

/// Hash private payload bytes exactly as they should be committed on-chain.
///
/// Relayers should hash the exact bytes they received and compare the result
/// with the intent's on-chain payload hash before executing anything.
pub fn hash_payload_bytes(payload: &[u8]) -> [u8; 32] {
    hash(payload).to_bytes()
}

/// Serialize JSON compactly and return the matching payload hash and bytes.
///
/// Use this helper when callers own structured JSON values and want a stable
/// compact representation. If a frontend or file already produced exact JSON
/// bytes, call [`hash_payload_bytes`] on those bytes instead.
pub fn hash_payload_json(value: &serde_json::Value) -> Result<([u8; 32], Vec<u8>)> {
    let bytes = serde_json::to_vec(value).context("failed to serialize payload JSON")?;
    Ok((hash_payload_bytes(&bytes), bytes))
}

/// Format a 32-byte payload hash as lowercase hex.
pub fn payload_hash_to_hex(payload_hash: &[u8; 32]) -> String {
    payload_hash
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Parse a lowercase or uppercase hex payload hash, with optional `0x` prefix.
pub fn payload_hash_from_hex(value: &str) -> Result<[u8; 32]> {
    let trimmed = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .unwrap_or(value);

    anyhow::ensure!(
        trimmed.len() == 64,
        "payload hash must be 32 bytes encoded as 64 hex characters"
    );

    let mut bytes = [0u8; 32];
    for (index, chunk) in trimmed.as_bytes().chunks_exact(2).enumerate() {
        let hex = std::str::from_utf8(chunk).context("payload hash contains invalid utf-8")?;
        bytes[index] = u8::from_str_radix(hex, 16)
            .with_context(|| format!("payload hash contains invalid hex at byte {index}"))?;
    }

    Ok(bytes)
}

pub fn derive_vault_pda(owner: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[VAULT_SEED, owner.as_ref()], &stealth_vault_program_id())
}

pub fn derive_intent_pda(vault: &Pubkey, nonce: u64) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[INTENT_SEED, vault.as_ref(), &nonce.to_le_bytes()],
        &stealth_vault_program_id(),
    )
}

pub fn initialize_vault_instruction(owner: Pubkey, ephemeral_authority: Pubkey) -> Instruction {
    let (vault, _) = derive_vault_pda(&owner);
    let mut data = INITIALIZE_VAULT_DISCRIMINATOR.to_vec();
    data.extend_from_slice(ephemeral_authority.as_ref());

    Instruction {
        program_id: stealth_vault_program_id(),
        accounts: vec![
            AccountMeta::new(owner, true),
            AccountMeta::new(vault, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
        data,
    }
}

pub fn rotate_authority_instruction(owner: Pubkey, new_ephemeral_authority: Pubkey) -> Instruction {
    let (vault, _) = derive_vault_pda(&owner);
    let mut data = ROTATE_AUTHORITY_DISCRIMINATOR.to_vec();
    data.extend_from_slice(new_ephemeral_authority.as_ref());

    Instruction {
        program_id: stealth_vault_program_id(),
        accounts: vec![
            AccountMeta::new(owner, true),
            AccountMeta::new(vault, false),
        ],
        data,
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
    let mut data = SUBMIT_EXECUTION_INTENT_DISCRIMINATOR.to_vec();
    data.extend_from_slice(&nonce.to_le_bytes());
    data.extend_from_slice(&payload_hash);

    Instruction {
        program_id: stealth_vault_program_id(),
        accounts: vec![
            AccountMeta::new(ephemeral_authority, true),
            AccountMeta::new_readonly(vault, false),
            AccountMeta::new(intent, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
        data,
    }
}

pub fn cancel_intent_instruction(owner: Pubkey, authority: Pubkey, nonce: u64) -> Instruction {
    let (vault, _) = derive_vault_pda(&owner);
    let (intent, _) = derive_intent_pda(&vault, nonce);
    let mut data = CANCEL_INTENT_DISCRIMINATOR.to_vec();
    data.extend_from_slice(&nonce.to_le_bytes());

    Instruction {
        program_id: stealth_vault_program_id(),
        accounts: vec![
            AccountMeta::new(authority, true),
            AccountMeta::new(vault, false),
            AccountMeta::new(intent, false),
        ],
        data,
    }
}

pub fn execute_intent_instruction(owner: Pubkey, executor: Pubkey, nonce: u64) -> Instruction {
    let (vault, _) = derive_vault_pda(&owner);
    let (intent, _) = derive_intent_pda(&vault, nonce);
    let mut data = EXECUTE_INTENT_DISCRIMINATOR.to_vec();
    data.extend_from_slice(&nonce.to_le_bytes());

    Instruction {
        program_id: stealth_vault_program_id(),
        accounts: vec![
            AccountMeta::new(executor, true),
            AccountMeta::new(vault, false),
            AccountMeta::new(intent, false),
        ],
        data,
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

/// Submit a [`PrivateIntent`]'s hash on-chain while keeping payload bytes off-chain.
pub fn submit_private_intent(
    rpc_client: &RpcClient,
    owner: Pubkey,
    ephemeral_authority: &Keypair,
    intent: &PrivateIntent,
) -> Result<SubmitExecutionIntentResult> {
    submit_execution_intent(
        rpc_client,
        owner,
        ephemeral_authority,
        intent.nonce,
        intent.payload_hash,
    )
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
    fn hashes_payload_bytes_and_formats_hex() {
        let payload = br#"{"nonce":1,"kind":"mock_execution"}"#;
        let payload_hash = hash_payload_bytes(payload);
        let hex = payload_hash_to_hex(&payload_hash);

        assert_eq!(hex.len(), 64);
        assert_eq!(
            payload_hash_from_hex(&hex).expect("hex hash should parse"),
            payload_hash
        );
        assert_eq!(
            payload_hash_from_hex(&format!("0x{hex}")).expect("prefixed hash should parse"),
            payload_hash
        );
    }

    #[test]
    fn builds_private_intent_from_exact_payload_bytes() {
        let payload = br#"{"nonce":1,"kind":"mock_execution"}"#;

        let intent = PrivateIntent::from_bytes(1, payload.as_slice());

        assert_eq!(intent.nonce, 1);
        assert_eq!(intent.payload_bytes, payload);
        assert_eq!(intent.payload_hash, hash_payload_bytes(payload));
        assert_eq!(intent.hash_hex(), payload_hash_to_hex(&intent.payload_hash));
    }

    #[test]
    fn hashes_compact_json_payloads() {
        let value = serde_json::json!({
            "nonce": 1,
            "kind": "mock_execution",
            "payload": {
                "message": "hello shadow"
            },
            "expires_at": null
        });

        let (payload_hash, bytes) =
            hash_payload_json(&value).expect("json payload should serialize");

        assert_eq!(payload_hash, hash_payload_bytes(&bytes));
        assert_eq!(
            std::str::from_utf8(&bytes).expect("json should be utf-8"),
            r#"{"expires_at":null,"kind":"mock_execution","nonce":1,"payload":{"message":"hello shadow"}}"#
        );
    }

    #[test]
    fn builds_private_intent_from_compact_json() {
        let value = serde_json::json!({
            "nonce": 2,
            "kind": "mock_execution",
            "payload": {
                "message": "hello shadow"
            },
            "expires_at": null
        });

        let intent = PrivateIntent::from_json(2, &value).expect("json intent should build");

        assert_eq!(intent.nonce, 2);
        assert_eq!(
            intent.payload_hash,
            hash_payload_bytes(&intent.payload_bytes)
        );
    }

    #[test]
    fn initialize_instruction_targets_derived_vault() {
        let owner = Pubkey::new_unique();
        let ephemeral_authority = Pubkey::new_unique();
        let (vault, _) = derive_vault_pda(&owner);

        let instruction = initialize_vault_instruction(owner, ephemeral_authority);

        assert_eq!(instruction.program_id, stealth_vault_program_id());
        assert_eq!(instruction.accounts[0].pubkey, owner);
        assert_eq!(instruction.accounts[1].pubkey, vault);
    }

    #[test]
    fn rotate_instruction_targets_derived_vault() {
        let owner = Pubkey::new_unique();
        let new_ephemeral_authority = Pubkey::new_unique();
        let (vault, _) = derive_vault_pda(&owner);

        let instruction = rotate_authority_instruction(owner, new_ephemeral_authority);

        assert_eq!(instruction.program_id, stealth_vault_program_id());
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

        assert_eq!(instruction.program_id, stealth_vault_program_id());
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

        assert_eq!(instruction.program_id, stealth_vault_program_id());
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

        assert_eq!(instruction.program_id, stealth_vault_program_id());
        assert_eq!(instruction.accounts[0].pubkey, executor);
        assert_eq!(instruction.accounts[1].pubkey, vault);
        assert_eq!(instruction.accounts[2].pubkey, intent);
    }
}
