use std::{
    fs::{self, OpenOptions},
    io::ErrorKind,
    net::SocketAddr,
    path::{Path, PathBuf},
    str::FromStr,
    sync::Arc,
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anchor_lang::AccountDeserialize;
use anyhow::{Context, Result};
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use clap::{Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use shadow_stealth::{derive_intent_pda, derive_vault_pda, execute_intent};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    hash::hash,
    pubkey::Pubkey,
    signature::{read_keypair_file, Keypair, Signer},
};
use stealth_vault::{ExecutionIntent, INTENT_STATUS_PENDING};
use tower_http::cors::CorsLayer;

mod executor;
mod payload;

use executor::execute_payload_action;
use payload::{
    parse_execution_route, parse_payload_action, ExecutionRoute, IntentPayload, PayloadAction,
};

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
    /// Start a stateless HTTP relayer API.
    Serve(ServeArgs),
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
    /// TOML config file. CLI flags override values from this file.
    #[arg(long)]
    config: Option<PathBuf>,

    /// Cluster to read and submit transactions to.
    #[arg(long, value_enum)]
    cluster: Option<Cluster>,

    /// Override the RPC URL. Takes precedence over --cluster.
    #[arg(long)]
    rpc_url: Option<String>,

    /// Owner pubkey for the vault.
    #[arg(long)]
    owner: Option<Pubkey>,

    /// Current ephemeral authority keypair path. This signer marks the intent executed.
    #[arg(long)]
    executor_keypair: Option<String>,

    /// Private intent payload JSON file.
    #[arg(long)]
    payload: Option<PathBuf>,
}

#[derive(Debug, Parser)]
struct RunArgs {
    /// TOML config file. CLI flags override values from this file.
    #[arg(long)]
    config: Option<PathBuf>,

    /// Cluster to read and submit transactions to.
    #[arg(long, value_enum)]
    cluster: Option<Cluster>,

    /// Override the RPC URL. Takes precedence over --cluster.
    #[arg(long)]
    rpc_url: Option<String>,

    /// Owner pubkey for the vault.
    #[arg(long)]
    owner: Option<Pubkey>,

    /// Current ephemeral authority keypair path. This signer marks intents executed.
    #[arg(long)]
    executor_keypair: Option<String>,

    /// Queue root containing pending, executed, and failed payload directories.
    #[arg(long)]
    payload_dir: Option<PathBuf>,

    /// Poll forever instead of exiting after one scan.
    #[arg(long)]
    watch: bool,

    /// Seconds to wait between scans when --watch is set.
    #[arg(long)]
    poll_seconds: Option<u64>,

    /// Number of failed processing attempts before moving a payload to failed/.
    #[arg(long)]
    max_retries: Option<u8>,
}

#[derive(Debug, Parser)]
struct ServeArgs {
    /// TOML config file. CLI flags override values from this file.
    #[arg(long)]
    config: Option<PathBuf>,

    /// Cluster to read and submit transactions to.
    #[arg(long, value_enum)]
    cluster: Option<Cluster>,

    /// Override the RPC URL. Takes precedence over --cluster.
    #[arg(long)]
    rpc_url: Option<String>,

    /// Current ephemeral authority keypair path. This signer marks intents executed.
    #[arg(long)]
    executor_keypair: Option<String>,

    /// HTTP bind address.
    #[arg(long, default_value = "127.0.0.1:8787")]
    bind: SocketAddr,
}

#[derive(Debug, Clone, Copy, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
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

#[derive(Debug, Default, Deserialize)]
struct RelayerConfig {
    cluster: Option<Cluster>,
    rpc_url: Option<String>,
    owner: Option<String>,
    executor_keypair: Option<String>,
    payload: Option<PathBuf>,
    payload_dir: Option<PathBuf>,
    poll_seconds: Option<u64>,
    max_retries: Option<u8>,
}

struct ExecuteOnceConfig {
    rpc_url: String,
    owner: Pubkey,
    executor_keypair: String,
    payload: PathBuf,
}

struct RunConfig {
    rpc_url: String,
    owner: Pubkey,
    executor_keypair: String,
    payload_dir: PathBuf,
    watch: bool,
    poll_seconds: u64,
    max_retries: u8,
}

struct ServeConfig {
    rpc_url: String,
    executor_keypair: String,
    bind: SocketAddr,
}

#[derive(Clone)]
struct ApiState {
    rpc_client: Arc<RpcClient>,
    rpc_url: String,
    executor: Arc<Keypair>,
}

#[derive(Debug, Deserialize)]
struct ExecuteOnceRequest {
    owner: Pubkey,
    payload: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct ExecuteOnceResponse {
    intent: Pubkey,
    vault: Pubkey,
    owner: Pubkey,
    executor: Pubkey,
    nonce: u64,
    signature: String,
    payload_hash: String,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
}

#[derive(Debug)]
struct ApiError(anyhow::Error);

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::HashPayload(args) => handle_hash_payload(args),
        Command::InitQueue(args) => handle_init_queue(args),
        Command::QueueStatus(args) => handle_queue_status(args),
        Command::ExecuteOnce(args) => handle_execute_once(args),
        Command::Run(args) => handle_run(args),
        Command::Serve(args) => {
            let runtime =
                tokio::runtime::Runtime::new().context("failed to start tokio runtime")?;
            runtime.block_on(handle_serve(args))
        }
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
    let config = resolve_execute_once_config(args)?;
    let rpc_client =
        RpcClient::new_with_commitment(config.rpc_url.clone(), CommitmentConfig::confirmed());
    let executor = read_executor_keypair(&config.executor_keypair)?;
    let result = execute_payload_file(
        &rpc_client,
        &config.rpc_url,
        config.owner,
        &executor,
        &config.payload,
    )?;

    print_execution_result(&result);

    Ok(())
}

fn handle_run(args: RunArgs) -> Result<()> {
    let config = resolve_run_config(args)?;
    anyhow::ensure!(
        config.poll_seconds > 0,
        "--poll-seconds must be greater than zero"
    );

    let rpc_client =
        RpcClient::new_with_commitment(config.rpc_url.clone(), CommitmentConfig::confirmed());
    let executor = read_executor_keypair(&config.executor_keypair)?;
    let poll_interval = Duration::from_secs(config.poll_seconds);
    let queue_dirs = ensure_queue_dirs(&config.payload_dir)?;

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

            match execute_payload_file(&rpc_client, &config.rpc_url, config.owner, &executor, &path)
            {
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

                    if attempts >= config.max_retries {
                        let archived_path =
                            archive_failed_payload(&path, &queue_dirs.failed, &err, attempts)?;
                        println!("archived failed payload {}", archived_path.display());
                    } else {
                        println!(
                            "will retry payload {} ({attempts}/{})",
                            path.display(),
                            config.max_retries
                        );
                    }
                }
            }
        }

        if !config.watch {
            break;
        }

        thread::sleep(poll_interval);
    }

    Ok(())
}

async fn handle_serve(args: ServeArgs) -> Result<()> {
    let config = resolve_serve_config(args)?;
    let rpc_client =
        RpcClient::new_with_commitment(config.rpc_url.clone(), CommitmentConfig::confirmed());
    let executor = read_executor_keypair(&config.executor_keypair)?;
    let state = ApiState {
        rpc_client: Arc::new(rpc_client),
        rpc_url: config.rpc_url,
        executor: Arc::new(executor),
    };
    let app = Router::new()
        .route("/health", get(health))
        .route("/execute-once", post(execute_once_http))
        .layer(CorsLayer::permissive())
        .with_state(state);
    let listener = tokio::net::TcpListener::bind(config.bind)
        .await
        .with_context(|| format!("failed to bind relayer API to {}", config.bind))?;

    println!("shadow relayer API listening on http://{}", config.bind);
    axum::serve(listener, app)
        .await
        .context("relayer API server failed")
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

async fn execute_once_http(
    State(state): State<ApiState>,
    Json(request): Json<ExecuteOnceRequest>,
) -> Result<Json<ExecuteOnceResponse>, ApiError> {
    let rpc_client = Arc::clone(&state.rpc_client);
    let rpc_url = state.rpc_url.clone();
    let executor = Arc::clone(&state.executor);
    let payload = request.payload;
    let owner = request.owner;

    let response = tokio::task::spawn_blocking(move || {
        let loaded = load_payload_value(payload)?;
        ensure_not_expired(&loaded.intent)?;
        let intent_account = fetch_intent(&rpc_client, owner, loaded.intent.nonce)?;
        validate_intent_for_execution(&intent_account, loaded.hash, executor.pubkey())?;
        execute_payload_action(
            &rpc_client,
            &loaded.action,
            &loaded.route,
            executor.as_ref(),
        )
        .context("failed to execute private payload action")?;
        let result = execute_intent(&rpc_client, owner, executor.as_ref(), loaded.intent.nonce)
            .with_context(|| format!("failed to execute intent on {rpc_url}"))?;

        Ok::<_, anyhow::Error>(ExecuteOnceResponse {
            intent: result.intent,
            vault: result.vault,
            owner: result.owner,
            executor: result.executor,
            nonce: result.nonce,
            signature: result.signature.to_string(),
            payload_hash: format_hash(&loaded.hash),
        })
    })
    .await
    .map_err(|err| ApiError(anyhow::anyhow!("relayer worker task failed: {err}")))?
    .map_err(ApiError)?;

    Ok(Json(response))
}

fn resolve_execute_once_config(args: ExecuteOnceArgs) -> Result<ExecuteOnceConfig> {
    let file_config = read_relayer_config(args.config.as_deref())?;
    let cluster = args
        .cluster
        .or(file_config.cluster)
        .unwrap_or(Cluster::Localnet);
    let rpc_url = args
        .rpc_url
        .or(file_config.rpc_url)
        .unwrap_or_else(|| cluster.rpc_url().to_string());

    Ok(ExecuteOnceConfig {
        rpc_url,
        owner: resolve_owner(args.owner, file_config.owner)?,
        executor_keypair: required_arg(
            args.executor_keypair.or(file_config.executor_keypair),
            "executor_keypair",
        )?,
        payload: required_arg(args.payload.or(file_config.payload), "payload")?,
    })
}

fn resolve_run_config(args: RunArgs) -> Result<RunConfig> {
    let file_config = read_relayer_config(args.config.as_deref())?;
    let cluster = args
        .cluster
        .or(file_config.cluster)
        .unwrap_or(Cluster::Localnet);
    let rpc_url = args
        .rpc_url
        .or(file_config.rpc_url)
        .unwrap_or_else(|| cluster.rpc_url().to_string());

    Ok(RunConfig {
        rpc_url,
        owner: resolve_owner(args.owner, file_config.owner)?,
        executor_keypair: required_arg(
            args.executor_keypair.or(file_config.executor_keypair),
            "executor_keypair",
        )?,
        payload_dir: required_arg(args.payload_dir.or(file_config.payload_dir), "payload_dir")?,
        poll_seconds: args.poll_seconds.or(file_config.poll_seconds).unwrap_or(5),
        max_retries: args
            .max_retries
            .or(file_config.max_retries)
            .unwrap_or(DEFAULT_MAX_RETRIES),
        watch: args.watch,
    })
}

fn resolve_serve_config(args: ServeArgs) -> Result<ServeConfig> {
    let file_config = read_relayer_config(args.config.as_deref())?;
    let cluster = args
        .cluster
        .or(file_config.cluster)
        .unwrap_or(Cluster::Localnet);
    let rpc_url = args
        .rpc_url
        .or(file_config.rpc_url)
        .unwrap_or_else(|| cluster.rpc_url().to_string());

    Ok(ServeConfig {
        rpc_url,
        executor_keypair: required_arg(
            args.executor_keypair.or(file_config.executor_keypair),
            "executor_keypair",
        )?,
        bind: args.bind,
    })
}

fn read_relayer_config(path: Option<&Path>) -> Result<RelayerConfig> {
    let Some(path) = path else {
        return Ok(RelayerConfig::default());
    };

    let contents = fs::read_to_string(path)
        .with_context(|| format!("failed to read relayer config {}", path.display()))?;
    toml::from_str(&contents)
        .with_context(|| format!("failed to parse relayer config {}", path.display()))
}

fn required_arg<T>(value: Option<T>, name: &str) -> Result<T> {
    value.with_context(|| format!("missing required relayer setting `{name}`"))
}

fn resolve_owner(cli_owner: Option<Pubkey>, config_owner: Option<String>) -> Result<Pubkey> {
    if let Some(owner) = cli_owner {
        return Ok(owner);
    }

    let owner = required_arg(config_owner, "owner")?;
    Pubkey::from_str(&owner).with_context(|| format!("invalid owner pubkey `{owner}`"))
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
    route: ExecutionRoute,
}

fn read_payload_file(path: &Path) -> Result<LoadedPayload> {
    let bytes = fs::read(path)
        .with_context(|| format!("failed to read payload file {}", path.display()))?;
    let intent = serde_json::from_slice::<IntentPayload>(&bytes)
        .with_context(|| format!("failed to parse payload JSON {}", path.display()))?;
    let action = parse_payload_action(&intent)
        .with_context(|| format!("failed to validate payload schema {}", path.display()))?;
    let route = parse_execution_route(&intent)
        .with_context(|| format!("failed to validate execution route {}", path.display()))?;
    let hash = hash(&bytes).to_bytes();

    Ok(LoadedPayload {
        intent,
        hash,
        action,
        route,
    })
}

fn load_payload_value(value: serde_json::Value) -> Result<LoadedPayload> {
    let bytes = serde_json::to_vec(&value).context("failed to serialize payload JSON")?;
    let intent =
        serde_json::from_value::<IntentPayload>(value).context("failed to parse payload JSON")?;
    let action = parse_payload_action(&intent).context("failed to validate payload schema")?;
    let route = parse_execution_route(&intent).context("failed to validate execution route")?;
    let hash = hash(&bytes).to_bytes();

    Ok(LoadedPayload {
        intent,
        hash,
        action,
        route,
    })
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
    let error_path = error_path(&destination);
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
    execute_payload_action(rpc_client, &payload.action, &payload.route, executor).with_context(
        || {
            format!(
                "failed to execute private payload action for {}",
                path.display()
            )
        },
    )?;

    execute_intent(rpc_client, owner, executor, payload.intent.nonce)
        .with_context(|| format!("failed to execute intent on {rpc_url}"))
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

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = Json(serde_json::json!({
            "error": format!("{:#}", self.0),
        }));

        (StatusCode::BAD_REQUEST, body).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        executor::validate_route_policy,
        payload::{OrderSide, PerpsOrder, PerpsVenue},
    };
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
    fn run_config_loads_from_toml_file() {
        let dir = temp_test_dir("run-config");
        let config_path = dir.join("relayer.toml");
        fs::write(
            &config_path,
            r#"
cluster = "devnet"
owner = "11111111111111111111111111111111"
executor_keypair = "~/.config/solana/ephemeral.json"
payload_dir = "payloads"
poll_seconds = 7
max_retries = 4
"#,
        )
        .expect("failed to write config");

        let config = resolve_run_config(RunArgs {
            config: Some(config_path),
            cluster: None,
            rpc_url: None,
            owner: None,
            executor_keypair: None,
            payload_dir: None,
            watch: true,
            poll_seconds: None,
            max_retries: None,
        })
        .expect("config should resolve");

        assert_eq!(config.rpc_url, Cluster::Devnet.rpc_url());
        assert_eq!(config.owner, Pubkey::default());
        assert_eq!(config.executor_keypair, "~/.config/solana/ephemeral.json");
        assert_eq!(config.payload_dir, PathBuf::from("payloads"));
        assert!(config.watch);
        assert_eq!(config.poll_seconds, 7);
        assert_eq!(config.max_retries, 4);
    }

    #[test]
    fn execute_once_cli_args_override_config_file() {
        let dir = temp_test_dir("execute-config");
        let config_path = dir.join("relayer.toml");
        let cli_owner = Pubkey::new_unique();
        let config_payload = dir.join("config.json");
        let cli_payload = dir.join("cli.json");
        fs::write(
            &config_path,
            format!(
                r#"
cluster = "devnet"
owner = "11111111111111111111111111111111"
executor_keypair = "config-keypair.json"
payload = "{}"
"#,
                config_payload.display()
            ),
        )
        .expect("failed to write config");

        let config = resolve_execute_once_config(ExecuteOnceArgs {
            config: Some(config_path),
            cluster: Some(Cluster::Localnet),
            rpc_url: Some("http://override.local".to_string()),
            owner: Some(cli_owner),
            executor_keypair: Some("cli-keypair.json".to_string()),
            payload: Some(cli_payload.clone()),
        })
        .expect("config should resolve");

        assert_eq!(config.rpc_url, "http://override.local");
        assert_eq!(config.owner, cli_owner);
        assert_eq!(config.executor_keypair, "cli-keypair.json");
        assert_eq!(config.payload, cli_payload);
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
        assert_eq!(loaded.route, ExecutionRoute::PublicRpc);
        assert_eq!(loaded.hash, hash(bytes.as_bytes()).to_bytes());
    }

    #[test]
    fn loads_http_payload_and_hashes_canonical_json() {
        let value = serde_json::json!({
            "nonce": 7,
            "kind": "mock_execution",
            "payload": {
                "message": "hello shadow"
            },
            "expires_at": null
        });
        let canonical_bytes =
            serde_json::to_vec(&value).expect("payload should serialize canonically");

        let loaded = load_payload_value(value).expect("http payload should load");

        assert_eq!(loaded.intent.nonce, 7);
        assert_eq!(
            loaded.action,
            PayloadAction::MockExecution {
                message: "hello shadow".to_string()
            }
        );
        assert_eq!(loaded.hash, hash(&canonical_bytes).to_bytes());
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
            route: None,
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
            kind: "unknown_action".to_string(),
            payload: serde_json::json!({}),
            route: None,
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
            route: None,
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
    fn validates_perps_order_payload_schema() {
        let intent = IntentPayload {
            nonce: 9,
            kind: "perps_order".to_string(),
            payload: serde_json::json!({
                "venue": "mock",
                "market": "SOL-PERP",
                "side": "long",
                "size_base_lots": 10,
                "limit_price": 150_000_000,
                "max_slippage_bps": 50,
                "reduce_only": false,
                "client_order_id": "shadow-test-1",
            }),
            route: None,
            expires_at: None,
        };

        let action = parse_payload_action(&intent).expect("perps payload should validate");

        assert_eq!(
            action,
            PayloadAction::PerpsOrder(PerpsOrder {
                venue: PerpsVenue::Mock,
                market: "SOL-PERP".to_string(),
                side: OrderSide::Long,
                size_base_lots: 10,
                limit_price: 150_000_000,
                max_slippage_bps: 50,
                reduce_only: false,
                client_order_id: "shadow-test-1".to_string(),
            })
        );
    }

    #[test]
    fn rejects_perps_order_with_unsafe_slippage() {
        let intent = IntentPayload {
            nonce: 9,
            kind: "perps_order".to_string(),
            payload: serde_json::json!({
                "venue": "mock",
                "market": "SOL-PERP",
                "side": "short",
                "size_base_lots": 10,
                "limit_price": 150_000_000,
                "max_slippage_bps": 1_001,
                "client_order_id": "shadow-test-2",
            }),
            route: None,
            expires_at: None,
        };

        let err = parse_payload_action(&intent).expect_err("unsafe slippage should fail");

        assert!(
            err.to_string()
                .contains("perps_order max_slippage_bps must be <= 1000"),
            "unexpected error: {err:#}"
        );
    }

    #[test]
    fn parses_mock_private_bundle_route() {
        let intent = IntentPayload {
            nonce: 9,
            kind: "perps_order".to_string(),
            payload: serde_json::json!({}),
            route: Some(serde_json::json!({
                "kind": "mock_private_bundle",
                "tip_lamports": 5_000,
            })),
            expires_at: None,
        };

        let route = parse_execution_route(&intent).expect("route should parse");

        assert_eq!(
            route,
            ExecutionRoute::MockPrivateBundle {
                tip_lamports: 5_000
            }
        );
    }

    #[test]
    fn perps_orders_require_private_or_bundle_route() {
        let action = PayloadAction::PerpsOrder(PerpsOrder {
            venue: PerpsVenue::Mock,
            market: "SOL-PERP".to_string(),
            side: OrderSide::Long,
            size_base_lots: 10,
            limit_price: 150_000_000,
            max_slippage_bps: 50,
            reduce_only: false,
            client_order_id: "shadow-test-3".to_string(),
        });

        let err = validate_route_policy(&action, &ExecutionRoute::PublicRpc)
            .expect_err("public route should be rejected");

        assert!(
            err.to_string()
                .contains("perps_order requires a private/bundle route"),
            "unexpected error: {err:#}"
        );
        validate_route_policy(
            &action,
            &ExecutionRoute::MockPrivateBundle { tip_lamports: 0 },
        )
        .expect("mock private route should pass");
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
        let error_path = error_path(&archived);

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
            route: None,
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
            route: None,
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
