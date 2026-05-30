use std::{
    fs::{self, OpenOptions},
    io::ErrorKind,
    path::{Path, PathBuf},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anchor_lang::AccountDeserialize;
use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use shadow_stealth::{derive_intent_pda, derive_vault_pda, execute_intent};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    hash::hash,
    pubkey::Pubkey,
    signature::{read_keypair_file, Signer},
    system_instruction,
    transaction::Transaction,
};
use stealth_vault::{ExecutionIntent, INTENT_STATUS_PENDING};

const PENDING_DIR: &str = "pending";
const EXECUTED_DIR: &str = "executed";
const FAILED_DIR: &str = "failed";
const DEFAULT_MAX_RETRIES: u8 = 3;

#[derive(Debug, Parser)]
#[command(name = "shadow-relayer")]
#[command(about = "Shadow SDK intent relayer")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Print the payload hash a user should submit on-chain.
    HashPayload(PayloadArgs),
    /// Create the relayer queue directory layout.
    InitQueue(QueueArgs),
    /// Print queue counts for pending, executed, and failed payloads.
    QueueStatus(QueueArgs),
    /// Verify one private payload against one pending on-chain intent and mark it executed.
    ExecuteOnce(ExecuteOnceArgs),
    /// Scan a payload directory and execute every matching pending intent.
    Run(RunArgs),
}

#[derive(Debug, Parser)]
struct PayloadArgs {
    /// Private intent payload JSON file.
    #[arg(long)]
    payload: PathBuf,
}

#[derive(Debug, Parser)]
struct QueueArgs {
    /// Queue root containing pending, executed, and failed payload directories.
    #[arg(long)]
    payload_dir: PathBuf,
}

#[derive(Debug, Parser)]
struct ExecuteOnceArgs {
    /// Cluster to read and submit transactions to.
    #[arg(long, value_enum, default_value_t = Cluster::Localnet)]
    cluster: Cluster,

    /// Override the RPC URL. Takes precedence over --cluster.
    #[arg(long)]
    rpc_url: Option<String>,

    /// Owner pubkey for the vault.
    #[arg(long)]
    owner: Pubkey,

    /// Current ephemeral authority keypair path. This signer marks the intent executed.
    #[arg(long)]
    executor_keypair: String,

    /// Private intent payload JSON file.
    #[arg(long)]
    payload: PathBuf,
}

#[derive(Debug, Parser)]
struct RunArgs {
    /// Cluster to read and submit transactions to.
    #[arg(long, value_enum, default_value_t = Cluster::Localnet)]
    cluster: Cluster,

    /// Override the RPC URL. Takes precedence over --cluster.
    #[arg(long)]
    rpc_url: Option<String>,

    /// Owner pubkey for the vault.
    #[arg(long)]
    owner: Pubkey,

    /// Current ephemeral authority keypair path. This signer marks intents executed.
    #[arg(long)]
    executor_keypair: String,

    /// Queue root containing pending, executed, and failed payload directories.
    #[arg(long)]
    payload_dir: PathBuf,

    /// Poll forever instead of exiting after one scan.
    #[arg(long)]
    watch: bool,

    /// Seconds to wait between scans when --watch is set.
    #[arg(long, default_value_t = 5)]
    poll_seconds: u64,

    /// Number of failed processing attempts before moving a payload to failed/.
    #[arg(long, default_value_t = DEFAULT_MAX_RETRIES)]
    max_retries: u8,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Cluster {
    Localnet,
    Devnet,
}

impl Cluster {
    fn rpc_url(self) -> &'static str {
        match self {
            Self::Localnet => "http://127.0.0.1:8899",
            Self::Devnet => "https://api.devnet.solana.com",
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct IntentPayload {
    nonce: u64,
    kind: String,
    payload: serde_json::Value,
    expires_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PayloadAction {
    MockExecution { message: String },
    SystemTransfer { to: Pubkey, lamports: u64 },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::HashPayload(args) => handle_hash_payload(args),
        Command::InitQueue(args) => handle_init_queue(args),
        Command::QueueStatus(args) => handle_queue_status(args),
        Command::ExecuteOnce(args) => handle_execute_once(args),
        Command::Run(args) => handle_run(args),
    }
}

fn handle_hash_payload(args: PayloadArgs) -> Result<()> {
    let payload = read_payload_file(&args.payload)?;

    println!("nonce: {}", payload.intent.nonce);
    println!("kind: {}", payload.intent.kind);
    println!("payload hash: {}", format_hash(&payload.hash));

    Ok(())
}

fn handle_init_queue(args: QueueArgs) -> Result<()> {
    let queue_dirs = ensure_queue_dirs(&args.payload_dir)?;

    println!("queue root: {}", args.payload_dir.display());
    println!("pending: {}", queue_dirs.pending.display());
    println!("executed: {}", queue_dirs.executed.display());
    println!("failed: {}", queue_dirs.failed.display());

    Ok(())
}

fn handle_queue_status(args: QueueArgs) -> Result<()> {
    let queue_dirs = ensure_queue_dirs(&args.payload_dir)?;
    let status = queue_status(&queue_dirs)?;

    println!("queue root: {}", args.payload_dir.display());
    println!("pending json: {}", status.pending_json);
    println!("executed json: {}", status.executed_json);
    println!("failed json: {}", status.failed_json);
    println!("failed errors: {}", status.failed_errors);

    Ok(())
}

fn handle_execute_once(args: ExecuteOnceArgs) -> Result<()> {
    let rpc_url = args
        .rpc_url
        .unwrap_or_else(|| args.cluster.rpc_url().to_string());
    let rpc_client = RpcClient::new_with_commitment(rpc_url.clone(), CommitmentConfig::confirmed());
    let executor = read_executor_keypair(&args.executor_keypair)?;
    let result = execute_payload_file(&rpc_client, &rpc_url, args.owner, &executor, &args.payload)?;

    print_execution_result(&result);

    Ok(())
}

fn handle_run(args: RunArgs) -> Result<()> {
    anyhow::ensure!(
        args.poll_seconds > 0,
        "--poll-seconds must be greater than zero"
    );

    let rpc_url = args
        .rpc_url
        .unwrap_or_else(|| args.cluster.rpc_url().to_string());
    let rpc_client = RpcClient::new_with_commitment(rpc_url.clone(), CommitmentConfig::confirmed());
    let executor = read_executor_keypair(&args.executor_keypair)?;
    let poll_interval = Duration::from_secs(args.poll_seconds);
    let queue_dirs = ensure_queue_dirs(&args.payload_dir)?;

    loop {
        let payload_files = list_payload_files(&queue_dirs.pending)?;
        println!(
            "scanning {} payload file(s) in {}",
            payload_files.len(),
            queue_dirs.pending.display()
        );

        for path in payload_files {
            let _lock = match acquire_payload_lock(&path) {
                Ok(lock) => lock,
                Err(err) => {
                    println!("skipping locked payload {}: {err:#}", path.display());
                    continue;
                }
            };

            match execute_payload_file(&rpc_client, &rpc_url, args.owner, &executor, &path) {
                Ok(result) => {
                    println!("executed payload {}", path.display());
                    print_execution_result(&result);
                    let archived_path = archive_payload(&path, &queue_dirs.executed)?;
                    remove_attempt_metadata(&path)?;
                    println!("archived executed payload {}", archived_path.display());
                }
                Err(err) => {
                    println!("failed payload {}: {err:#}", path.display());
                    let attempts = record_failed_attempt(&path, &err)?;

                    if attempts >= args.max_retries {
                        let archived_path =
                            archive_failed_payload(&path, &queue_dirs.failed, &err, attempts)?;
                        println!("archived failed payload {}", archived_path.display());
                    } else {
                        println!(
                            "will retry payload {} ({attempts}/{})",
                            path.display(),
                            args.max_retries
                        );
                    }
                }
            }
        }

        if !args.watch {
            break;
        }

        thread::sleep(poll_interval);
    }

    Ok(())
}

struct QueueDirs {
    pending: PathBuf,
    executed: PathBuf,
    failed: PathBuf,
}

#[derive(Debug, PartialEq, Eq)]
struct QueueStatus {
    pending_json: usize,
    executed_json: usize,
    failed_json: usize,
    failed_errors: usize,
}

#[derive(Debug)]
struct LoadedPayload {
    intent: IntentPayload,
    hash: [u8; 32],
    action: PayloadAction,
}

fn read_payload_file(path: &Path) -> Result<LoadedPayload> {
    let bytes = fs::read(path)
        .with_context(|| format!("failed to read payload file {}", path.display()))?;
    let intent = serde_json::from_slice::<IntentPayload>(&bytes)
        .with_context(|| format!("failed to parse payload JSON {}", path.display()))?;
    let action = parse_payload_action(&intent)
        .with_context(|| format!("failed to validate payload schema {}", path.display()))?;
    let hash = hash(&bytes).to_bytes();

    Ok(LoadedPayload {
        intent,
        hash,
        action,
    })
}

fn parse_payload_action(intent: &IntentPayload) -> Result<PayloadAction> {
    match intent.kind.as_str() {
        "mock_execution" => {
            #[derive(Deserialize)]
            struct MockPayload {
                message: String,
            }

            let payload = serde_json::from_value::<MockPayload>(intent.payload.clone())
                .context("mock_execution payload must contain string field `message`")?;
            anyhow::ensure!(
                !payload.message.trim().is_empty(),
                "mock_execution message must not be empty"
            );

            Ok(PayloadAction::MockExecution {
                message: payload.message,
            })
        }
        "system_transfer" => {
            #[derive(Deserialize)]
            struct TransferPayload {
                to: Pubkey,
                lamports: u64,
            }

            let payload = serde_json::from_value::<TransferPayload>(intent.payload.clone())
                .context("system_transfer payload must contain `to` and `lamports`")?;
            anyhow::ensure!(
                payload.to != Pubkey::default(),
                "system_transfer recipient must not be the default pubkey"
            );
            anyhow::ensure!(
                payload.lamports > 0,
                "system_transfer lamports must be greater than zero"
            );

            Ok(PayloadAction::SystemTransfer {
                to: payload.to,
                lamports: payload.lamports,
            })
        }
        other => anyhow::bail!(
            "unsupported payload kind `{other}`; supported kinds are `mock_execution` and `system_transfer`"
        ),
    }
}

fn ensure_queue_dirs(payload_dir: &Path) -> Result<QueueDirs> {
    let pending = payload_dir.join(PENDING_DIR);
    let executed = payload_dir.join(EXECUTED_DIR);
    let failed = payload_dir.join(FAILED_DIR);

    for dir in [&pending, &executed, &failed] {
        fs::create_dir_all(dir)
            .with_context(|| format!("failed to create queue directory {}", dir.display()))?;
    }

    Ok(QueueDirs {
        pending,
        executed,
        failed,
    })
}

fn queue_status(queue_dirs: &QueueDirs) -> Result<QueueStatus> {
    Ok(QueueStatus {
        pending_json: count_files_with_extension(&queue_dirs.pending, "json")?,
        executed_json: count_files_with_extension(&queue_dirs.executed, "json")?,
        failed_json: count_files_with_extension(&queue_dirs.failed, "json")?,
        failed_errors: count_files_with_extension(&queue_dirs.failed, "error")?,
    })
}

fn list_payload_files(payload_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    for entry in fs::read_dir(payload_dir)
        .with_context(|| format!("failed to read payload directory {}", payload_dir.display()))?
    {
        let entry = entry.with_context(|| {
            format!(
                "failed to read an entry in payload directory {}",
                payload_dir.display()
            )
        })?;
        let path = entry.path();

        if path
            .extension()
            .is_some_and(|extension| extension == "json")
            && !lock_path(&path).exists()
        {
            files.push(path);
        }
    }

    files.sort();
    Ok(files)
}

fn count_files_with_extension(dir: &Path, extension: &str) -> Result<usize> {
    let mut count = 0;

    for entry in fs::read_dir(dir)
        .with_context(|| format!("failed to read queue directory {}", dir.display()))?
    {
        let path = entry
            .with_context(|| {
                format!(
                    "failed to read an entry in queue directory {}",
                    dir.display()
                )
            })?
            .path();

        if path
            .extension()
            .is_some_and(|candidate| candidate == extension)
        {
            count += 1;
        }
    }

    Ok(count)
}

fn archive_payload(path: &Path, destination_dir: &Path) -> Result<PathBuf> {
    let destination = next_archive_path(path, destination_dir)?;
    fs::rename(path, &destination).with_context(|| {
        format!(
            "failed to move payload {} to {}",
            path.display(),
            destination.display()
        )
    })?;

    Ok(destination)
}

fn archive_failed_payload(
    path: &Path,
    destination_dir: &Path,
    err: &anyhow::Error,
    attempts: u8,
) -> Result<PathBuf> {
    let destination = archive_payload(path, destination_dir)?;
    let error_path = destination.with_extension("json.error");
    fs::write(&error_path, failure_metadata(err, attempts)).with_context(|| {
        format!(
            "failed to write payload error file {}",
            error_path.display()
        )
    })?;
    remove_attempt_metadata(path)?;

    Ok(destination)
}

fn failure_metadata(err: &anyhow::Error, attempts: u8) -> String {
    serde_json::json!({
        "attempts": attempts,
        "error": format!("{err:#}"),
    })
    .to_string()
}

fn attempt_path(path: &Path) -> PathBuf {
    sidecar_path(path, "attempts")
}

fn error_path(path: &Path) -> PathBuf {
    sidecar_path(path, "error")
}

fn lock_path(path: &Path) -> PathBuf {
    sidecar_path(path, "lock")
}

fn sidecar_path(path: &Path, suffix: &str) -> PathBuf {
    let mut sidecar = path.as_os_str().to_os_string();
    sidecar.push(".");
    sidecar.push(suffix);
    PathBuf::from(sidecar)
}

fn read_attempts(path: &Path) -> Result<u8> {
    let path = attempt_path(path);

    match fs::read_to_string(&path) {
        Ok(contents) => contents
            .trim()
            .parse::<u8>()
            .with_context(|| format!("failed to parse attempt count {}", path.display())),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(0),
        Err(err) => Err(err).with_context(|| format!("failed to read {}", path.display())),
    }
}

fn record_failed_attempt(path: &Path, err: &anyhow::Error) -> Result<u8> {
    let attempts = read_attempts(path)?.saturating_add(1);
    fs::write(attempt_path(path), attempts.to_string())
        .with_context(|| format!("failed to write attempt count for {}", path.display()))?;
    fs::write(error_path(path), failure_metadata(err, attempts))
        .with_context(|| format!("failed to write retry error for {}", path.display()))?;

    Ok(attempts)
}

fn remove_attempt_metadata(path: &Path) -> Result<()> {
    for metadata_path in [attempt_path(path), error_path(path)] {
        match fs::remove_file(&metadata_path) {
            Ok(()) => {}
            Err(err) if err.kind() == ErrorKind::NotFound => {}
            Err(err) => {
                return Err(err)
                    .with_context(|| format!("failed to remove {}", metadata_path.display()))
            }
        }
    }

    Ok(())
}

struct PayloadLock {
    path: PathBuf,
}

impl Drop for PayloadLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn acquire_payload_lock(path: &Path) -> Result<PayloadLock> {
    let path = lock_path(path);
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .with_context(|| format!("failed to acquire payload lock {}", path.display()))?;

    Ok(PayloadLock { path })
}

fn next_archive_path(path: &Path, destination_dir: &Path) -> Result<PathBuf> {
    let file_name = path
        .file_name()
        .context("payload path does not have a file name")?;
    let candidate = destination_dir.join(file_name);

    if !candidate.exists() {
        return Ok(candidate);
    }

    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .context("payload file name is not valid UTF-8")?;
    let extension = path.extension().and_then(|extension| extension.to_str());

    for attempt in 1.. {
        let file_name = match extension {
            Some(extension) => format!("{stem}.{attempt}.{extension}"),
            None => format!("{stem}.{attempt}"),
        };
        let candidate = destination_dir.join(file_name);

        if !candidate.exists() {
            return Ok(candidate);
        }
    }

    unreachable!("archive path search is unbounded")
}

fn read_executor_keypair(path: &str) -> Result<solana_sdk::signature::Keypair> {
    let expanded_path = shellexpand::tilde(path).into_owned();
    read_keypair_file(&expanded_path)
        .map_err(|err| anyhow::anyhow!("failed to read executor keypair at {expanded_path}: {err}"))
}

fn execute_payload_file(
    rpc_client: &RpcClient,
    rpc_url: &str,
    owner: Pubkey,
    executor: &solana_sdk::signature::Keypair,
    path: &Path,
) -> Result<shadow_stealth::ExecuteIntentResult> {
    let payload = read_payload_file(path)?;
    ensure_not_expired(&payload.intent)?;
    let intent_account = fetch_intent(rpc_client, owner, payload.intent.nonce)?;
    validate_intent_for_execution(&intent_account, payload.hash, executor.pubkey())?;
    execute_payload_action(rpc_client, &payload.action, executor).with_context(|| {
        format!(
            "failed to execute private payload action for {}",
            path.display()
        )
    })?;

    execute_intent(rpc_client, owner, executor, payload.intent.nonce)
        .with_context(|| format!("failed to execute intent on {rpc_url}"))
}

fn execute_payload_action(
    rpc_client: &RpcClient,
    action: &PayloadAction,
    executor: &solana_sdk::signature::Keypair,
) -> Result<()> {
    match action {
        PayloadAction::MockExecution { message } => {
            println!("mock execution: {message}");
            Ok(())
        }
        PayloadAction::SystemTransfer { to, lamports } => {
            let from = executor.pubkey();
            let instruction = system_instruction::transfer(&from, to, *lamports);
            let recent_blockhash = rpc_client
                .get_latest_blockhash()
                .context("failed to fetch latest blockhash for system transfer")?;
            let transaction = Transaction::new_signed_with_payer(
                &[instruction],
                Some(&from),
                &[executor],
                recent_blockhash,
            );

            rpc_client
                .send_and_confirm_transaction_with_spinner_and_commitment(
                    &transaction,
                    CommitmentConfig::confirmed(),
                )
                .context("failed to send and confirm system transfer")?;

            Ok(())
        }
    }
}

fn validate_intent_for_execution(
    intent_account: &ExecutionIntent,
    payload_hash: [u8; 32],
    executor: Pubkey,
) -> Result<()> {
    anyhow::ensure!(
        intent_account.status == INTENT_STATUS_PENDING,
        "intent is not pending; current status is {}",
        intent_account.status
    );
    anyhow::ensure!(
        intent_account.payload_hash == payload_hash,
        "payload hash does not match on-chain intent"
    );
    anyhow::ensure!(
        intent_account.ephemeral_authority == executor,
        "executor keypair is not the intent's ephemeral authority"
    );

    Ok(())
}

fn fetch_intent(rpc_client: &RpcClient, owner: Pubkey, nonce: u64) -> Result<ExecutionIntent> {
    let (vault, _) = derive_vault_pda(&owner);
    let (intent, _) = derive_intent_pda(&vault, nonce);
    let account = rpc_client
        .get_account(&intent)
        .with_context(|| format!("failed to fetch intent account {intent}"))?;

    anyhow::ensure!(
        account.owner == stealth_vault::ID,
        "intent account is not owned by the stealth vault program"
    );

    let mut data = account.data.as_slice();
    ExecutionIntent::try_deserialize(&mut data)
        .with_context(|| format!("failed to deserialize intent account {intent}"))
}

fn ensure_not_expired(payload: &IntentPayload) -> Result<()> {
    if let Some(expires_at) = payload.expires_at {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("system clock is before unix epoch")?
            .as_secs() as i64;
        anyhow::ensure!(
            expires_at > now,
            "payload expired at {expires_at}; current unix timestamp is {now}"
        );
    }

    Ok(())
}

fn format_hash(hash: &[u8; 32]) -> String {
    hash.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn print_execution_result(result: &shadow_stealth::ExecuteIntentResult) {
    println!("intent PDA: {}", result.intent);
    println!("vault PDA: {}", result.vault);
    println!("owner: {}", result.owner);
    println!("executor: {}", result.executor);
    println!("nonce: {}", result.nonce);
    println!("signature: {}", result.signature);
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_sdk::pubkey::Pubkey;
    use stealth_vault::{INTENT_STATUS_CANCELLED, INTENT_STATUS_EXECUTED};

    fn valid_payload_json(nonce: u64, expires_at: Option<i64>) -> String {
        let expires_at = expires_at
            .map(|timestamp| timestamp.to_string())
            .unwrap_or_else(|| "null".to_string());

        format!(
            r#"{{"nonce":{nonce},"kind":"mock_execution","payload":{{"message":"hello shadow"}},"expires_at":{expires_at}}}"#
        )
    }

    fn temp_test_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("test clock should be after unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "shadow-relayer-{name}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("failed to create temp test dir");
        path
    }

    fn sample_intent(payload_hash: [u8; 32], executor: Pubkey, status: u8) -> ExecutionIntent {
        ExecutionIntent {
            vault: Pubkey::new_unique(),
            ephemeral_authority: executor,
            executor: Pubkey::default(),
            nonce: 1,
            payload_hash,
            status,
            created_at: 0,
            cancelled_at: 0,
            executed_at: 0,
            bump: 255,
        }
    }

    #[test]
    fn reads_payload_and_hashes_exact_file_bytes() {
        let dir = temp_test_dir("hash");
        let path = dir.join("intent.json");
        let bytes = valid_payload_json(7, None);
        fs::write(&path, &bytes).expect("failed to write payload");

        let loaded = read_payload_file(&path).expect("payload should load");

        assert_eq!(loaded.intent.nonce, 7);
        assert_eq!(loaded.intent.kind, "mock_execution");
        assert_eq!(loaded.intent.expires_at, None);
        assert_eq!(
            loaded.action,
            PayloadAction::MockExecution {
                message: "hello shadow".to_string()
            }
        );
        assert_eq!(loaded.hash, hash(bytes.as_bytes()).to_bytes());
    }

    #[test]
    fn validates_system_transfer_payload_schema() {
        let recipient = Pubkey::new_unique();
        let intent = IntentPayload {
            nonce: 8,
            kind: "system_transfer".to_string(),
            payload: serde_json::json!({
                "to": recipient,
                "lamports": 10,
            }),
            expires_at: None,
        };

        let action = parse_payload_action(&intent).expect("transfer payload should validate");

        assert_eq!(
            action,
            PayloadAction::SystemTransfer {
                to: recipient,
                lamports: 10,
            }
        );
    }

    #[test]
    fn rejects_unsupported_payload_kind() {
        let intent = IntentPayload {
            nonce: 8,
            kind: "perps_order".to_string(),
            payload: serde_json::json!({}),
            expires_at: None,
        };

        let err = parse_payload_action(&intent).expect_err("unsupported kind should fail");

        assert!(
            err.to_string().contains("unsupported payload kind"),
            "unexpected error: {err:#}"
        );
    }

    #[test]
    fn rejects_invalid_payload_schema() {
        let intent = IntentPayload {
            nonce: 8,
            kind: "system_transfer".to_string(),
            payload: serde_json::json!({
                "to": Pubkey::new_unique(),
                "lamports": 0,
            }),
            expires_at: None,
        };

        let err = parse_payload_action(&intent).expect_err("invalid transfer should fail");

        assert!(
            err.to_string()
                .contains("system_transfer lamports must be greater than zero"),
            "unexpected error: {err:#}"
        );
    }

    #[test]
    fn rejects_invalid_payload_json() {
        let dir = temp_test_dir("invalid-json");
        let path = dir.join("intent.json");
        fs::write(&path, "{not json").expect("failed to write payload");

        let err = read_payload_file(&path).expect_err("invalid JSON should fail");

        assert!(
            err.to_string().contains("failed to parse payload JSON"),
            "unexpected error: {err:#}"
        );
    }

    #[test]
    fn lists_only_json_payload_files_in_sorted_order() {
        let dir = temp_test_dir("list");
        fs::write(dir.join("b.json"), valid_payload_json(2, None)).expect("failed to write b");
        fs::write(dir.join("notes.txt"), "ignore me").expect("failed to write notes");
        fs::write(dir.join("a.json"), valid_payload_json(1, None)).expect("failed to write a");
        fs::write(dir.join("locked.json"), valid_payload_json(3, None))
            .expect("failed to write locked");
        fs::write(dir.join("locked.json.lock"), "").expect("failed to write lock");

        let files = list_payload_files(&dir).expect("payload dir should list");
        let names = files
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().to_string())
            .collect::<Vec<_>>();

        assert_eq!(names, vec!["a.json", "b.json"]);
    }

    #[test]
    fn creates_payload_queue_directories() {
        let dir = temp_test_dir("queue-dirs");

        let queue_dirs = ensure_queue_dirs(&dir).expect("queue dirs should be created");

        assert!(queue_dirs.pending.is_dir());
        assert!(queue_dirs.executed.is_dir());
        assert!(queue_dirs.failed.is_dir());
        assert_eq!(queue_dirs.pending, dir.join(PENDING_DIR));
        assert_eq!(queue_dirs.executed, dir.join(EXECUTED_DIR));
        assert_eq!(queue_dirs.failed, dir.join(FAILED_DIR));
    }

    #[test]
    fn run_queue_scans_pending_directory_only() {
        let dir = temp_test_dir("pending-only");
        let queue_dirs = ensure_queue_dirs(&dir).expect("queue dirs should be created");
        fs::write(
            queue_dirs.pending.join("pending.json"),
            valid_payload_json(1, None),
        )
        .expect("failed to write pending");
        fs::write(
            queue_dirs.executed.join("executed.json"),
            valid_payload_json(2, None),
        )
        .expect("failed to write executed");
        fs::write(
            queue_dirs.failed.join("failed.json"),
            valid_payload_json(3, None),
        )
        .expect("failed to write failed");

        let files = list_payload_files(&queue_dirs.pending).expect("pending dir should list");
        let names = files
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().to_string())
            .collect::<Vec<_>>();

        assert_eq!(names, vec!["pending.json"]);
    }

    #[test]
    fn archive_payload_moves_file_and_preserves_collisions() {
        let dir = temp_test_dir("archive");
        let pending = dir.join(PENDING_DIR);
        let executed = dir.join(EXECUTED_DIR);
        fs::create_dir_all(&pending).expect("failed to create pending");
        fs::create_dir_all(&executed).expect("failed to create executed");
        fs::write(executed.join("intent.json"), "already archived")
            .expect("failed to write collision");
        let path = pending.join("intent.json");
        fs::write(&path, "new payload").expect("failed to write payload");

        let archived = archive_payload(&path, &executed).expect("payload should archive");

        assert_eq!(archived, executed.join("intent.1.json"));
        assert!(!path.exists());
        assert_eq!(
            fs::read_to_string(archived).expect("failed to read archive"),
            "new payload"
        );
        assert_eq!(
            fs::read_to_string(executed.join("intent.json")).expect("failed to read original"),
            "already archived"
        );
    }

    #[test]
    fn archive_failed_payload_moves_file_and_writes_error() {
        let dir = temp_test_dir("failed-archive");
        let pending = dir.join(PENDING_DIR);
        let failed = dir.join(FAILED_DIR);
        fs::create_dir_all(&pending).expect("failed to create pending");
        fs::create_dir_all(&failed).expect("failed to create failed");
        let path = pending.join("bad.json");
        fs::write(&path, "bad payload").expect("failed to write payload");
        let err = anyhow::anyhow!("payload hash does not match on-chain intent");

        let archived =
            archive_failed_payload(&path, &failed, &err, 3).expect("failed payload should archive");
        let error_path = archived.with_extension("json.error");

        assert_eq!(archived, failed.join("bad.json"));
        assert!(!path.exists());
        assert_eq!(
            fs::read_to_string(&archived).expect("failed to read archive"),
            "bad payload"
        );
        assert!(fs::read_to_string(error_path)
            .expect("failed to read error file")
            .contains("payload hash does not match"),);
    }

    #[test]
    fn retry_metadata_tracks_attempts_before_archiving() {
        let dir = temp_test_dir("retry-metadata");
        let path = dir.join("intent.json");
        fs::write(&path, valid_payload_json(1, None)).expect("failed to write payload");
        let err = anyhow::anyhow!("temporary rpc error");

        let first = record_failed_attempt(&path, &err).expect("first attempt should record");
        let second = record_failed_attempt(&path, &err).expect("second attempt should record");

        assert_eq!(first, 1);
        assert_eq!(second, 2);
        assert_eq!(read_attempts(&path).expect("attempt count should read"), 2);
        assert!(fs::read_to_string(error_path(&path))
            .expect("failed to read retry error")
            .contains("temporary rpc error"));
    }

    #[test]
    fn payload_lock_blocks_second_processor_and_cleans_up_on_drop() {
        let dir = temp_test_dir("lock");
        let path = dir.join("intent.json");
        fs::write(&path, valid_payload_json(1, None)).expect("failed to write payload");

        let lock = acquire_payload_lock(&path).expect("first lock should succeed");
        let second_lock = acquire_payload_lock(&path);

        assert!(second_lock.is_err());
        assert!(lock_path(&path).exists());

        drop(lock);

        assert!(!lock_path(&path).exists());
    }

    #[test]
    fn queue_status_counts_payloads_and_error_files() {
        let dir = temp_test_dir("queue-status");
        let queue_dirs = ensure_queue_dirs(&dir).expect("queue dirs should be created");
        fs::write(
            queue_dirs.pending.join("pending.json"),
            valid_payload_json(1, None),
        )
        .expect("failed to write pending");
        fs::write(queue_dirs.pending.join("pending.txt"), "ignore")
            .expect("failed to write pending txt");
        fs::write(
            queue_dirs.executed.join("executed.json"),
            valid_payload_json(2, None),
        )
        .expect("failed to write executed");
        fs::write(
            queue_dirs.failed.join("failed.json"),
            valid_payload_json(3, None),
        )
        .expect("failed to write failed");
        fs::write(queue_dirs.failed.join("failed.json.error"), "bad")
            .expect("failed to write error");

        let status = queue_status(&queue_dirs).expect("queue status should load");

        assert_eq!(
            status,
            QueueStatus {
                pending_json: 1,
                executed_json: 1,
                failed_json: 1,
                failed_errors: 1,
            }
        );
    }

    #[test]
    fn expiry_check_accepts_null_and_future_expiry() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("test clock should be after unix epoch")
            .as_secs() as i64;

        let no_expiry = IntentPayload {
            nonce: 1,
            kind: "mock_execution".to_string(),
            payload: serde_json::json!({}),
            expires_at: None,
        };
        let future_expiry = IntentPayload {
            expires_at: Some(now + 60),
            ..no_expiry
        };

        ensure_not_expired(&future_expiry).expect("future expiry should pass");
    }

    #[test]
    fn expiry_check_rejects_expired_payload() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("test clock should be after unix epoch")
            .as_secs() as i64;
        let payload = IntentPayload {
            nonce: 1,
            kind: "mock_execution".to_string(),
            payload: serde_json::json!({}),
            expires_at: Some(now - 1),
        };

        let err = ensure_not_expired(&payload).expect_err("expired payload should fail");

        assert!(
            err.to_string().contains("payload expired"),
            "unexpected error: {err:#}"
        );
    }

    #[test]
    fn validates_matching_pending_intent() {
        let payload_hash = [7; 32];
        let executor = Pubkey::new_unique();
        let intent = sample_intent(payload_hash, executor, INTENT_STATUS_PENDING);

        validate_intent_for_execution(&intent, payload_hash, executor)
            .expect("matching pending intent should validate");
    }

    #[test]
    fn rejects_non_pending_intent() {
        let payload_hash = [7; 32];
        let executor = Pubkey::new_unique();

        for status in [INTENT_STATUS_CANCELLED, INTENT_STATUS_EXECUTED] {
            let intent = sample_intent(payload_hash, executor, status);
            let err = validate_intent_for_execution(&intent, payload_hash, executor)
                .expect_err("terminal intent should fail");

            assert!(
                err.to_string().contains("intent is not pending"),
                "unexpected error: {err:#}"
            );
        }
    }

    #[test]
    fn rejects_payload_hash_mismatch() {
        let executor = Pubkey::new_unique();
        let intent = sample_intent([7; 32], executor, INTENT_STATUS_PENDING);
        let err = validate_intent_for_execution(&intent, [8; 32], executor)
            .expect_err("hash mismatch should fail");

        assert!(
            err.to_string().contains("payload hash does not match"),
            "unexpected error: {err:#}"
        );
    }

    #[test]
    fn rejects_executor_mismatch() {
        let payload_hash = [7; 32];
        let intent = sample_intent(payload_hash, Pubkey::new_unique(), INTENT_STATUS_PENDING);
        let err = validate_intent_for_execution(&intent, payload_hash, Pubkey::new_unique())
            .expect_err("executor mismatch should fail");

        assert!(
            err.to_string()
                .contains("executor keypair is not the intent's ephemeral authority"),
            "unexpected error: {err:#}"
        );
    }
}
