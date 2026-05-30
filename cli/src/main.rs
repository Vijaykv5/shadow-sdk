use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use shadow_stealth::{
    cancel_intent, create_vault, execute_intent, rotate_authority, stealth_vault_program_id,
    submit_execution_intent,
};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    pubkey::Pubkey,
    signature::{read_keypair_file, Keypair, Signer},
};

const DEVNET_DEPLOY_WALLET: &str = "2eDJJZydDTV4HQmbtX6YwhrdfCW7XU3zms9538HGqkuB";
const LAMPORTS_PER_SOL: u64 = 1_000_000_000;

#[derive(Debug, Parser)]
#[command(name = "shadow")]
#[command(about = "Shadow SDK operator and developer CLI")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Create an ephemeral stealth vault PDA for the configured owner wallet.
    CreateVault(CreateVaultArgs),
    /// Rotate the vault's temporary execution authority.
    RotateAuthority(RotateAuthorityArgs),
    /// Submit a payload hash as an execution intent using the current ephemeral authority.
    SubmitIntent(SubmitIntentArgs),
    /// Cancel a pending execution intent as the owner or current ephemeral authority.
    CancelIntent(CancelIntentArgs),
    /// Mark a pending execution intent as executed using the current ephemeral authority.
    ExecuteIntent(ExecuteIntentArgs),
    /// Print devnet deployment and CLI test readiness.
    DevnetStatus(DevnetStatusArgs),
}

#[derive(Debug, Parser)]
struct CreateVaultArgs {
    /// Cluster to submit the transaction to.
    #[arg(long, value_enum, default_value_t = Cluster::Localnet)]
    cluster: Cluster,

    /// Override the RPC URL. Takes precedence over --cluster.
    #[arg(long)]
    rpc_url: Option<String>,

    /// Owner and fee-payer keypair path.
    #[arg(long, default_value = "~/.config/solana/id.json")]
    keypair: String,

    /// Existing execution authority pubkey. If omitted, a fresh ephemeral keypair is generated.
    #[arg(long)]
    ephemeral_authority: Option<Pubkey>,

    /// Existing execution authority keypair path. Used only for its pubkey.
    #[arg(long)]
    ephemeral_authority_keypair: Option<String>,
}

#[derive(Debug, Parser)]
struct RotateAuthorityArgs {
    /// Cluster to submit the transaction to.
    #[arg(long, value_enum, default_value_t = Cluster::Localnet)]
    cluster: Cluster,

    /// Override the RPC URL. Takes precedence over --cluster.
    #[arg(long)]
    rpc_url: Option<String>,

    /// Owner and fee-payer keypair path.
    #[arg(long, default_value = "~/.config/solana/id.json")]
    keypair: String,

    /// New execution authority pubkey. If omitted, a fresh ephemeral keypair is generated.
    #[arg(long)]
    new_ephemeral_authority: Option<Pubkey>,

    /// New execution authority keypair path. Used only for its pubkey.
    #[arg(long)]
    new_ephemeral_authority_keypair: Option<String>,
}

#[derive(Debug, Parser)]
struct SubmitIntentArgs {
    /// Cluster to submit the transaction to.
    #[arg(long, value_enum, default_value_t = Cluster::Localnet)]
    cluster: Cluster,

    /// Override the RPC URL. Takes precedence over --cluster.
    #[arg(long)]
    rpc_url: Option<String>,

    /// Owner pubkey for the vault. If omitted, --owner-keypair is read and its pubkey is used.
    #[arg(long)]
    owner: Option<Pubkey>,

    /// Owner keypair path used only to infer the owner pubkey when --owner is omitted.
    #[arg(long, default_value = "~/.config/solana/id.json")]
    owner_keypair: String,

    /// Current ephemeral authority keypair path. This keypair signs and pays for the intent.
    #[arg(long)]
    ephemeral_authority_keypair: String,

    /// Unique nonce for this intent under the vault.
    #[arg(long)]
    nonce: u64,

    /// 32-byte payload hash as 64 hex characters.
    #[arg(long)]
    payload_hash: String,
}

#[derive(Debug, Parser)]
struct CancelIntentArgs {
    /// Cluster to submit the transaction to.
    #[arg(long, value_enum, default_value_t = Cluster::Localnet)]
    cluster: Cluster,

    /// Override the RPC URL. Takes precedence over --cluster.
    #[arg(long)]
    rpc_url: Option<String>,

    /// Owner pubkey for the vault. If omitted, --owner-keypair is read and its pubkey is used.
    #[arg(long)]
    owner: Option<Pubkey>,

    /// Owner keypair path. Used as the cancel signer when --authority-keypair is omitted.
    #[arg(long, default_value = "~/.config/solana/id.json")]
    owner_keypair: String,

    /// Authority keypair path. Must be the owner or current ephemeral authority.
    #[arg(long)]
    authority_keypair: Option<String>,

    /// Nonce of the intent to cancel.
    #[arg(long)]
    nonce: u64,
}

#[derive(Debug, Parser)]
struct ExecuteIntentArgs {
    /// Cluster to submit the transaction to.
    #[arg(long, value_enum, default_value_t = Cluster::Localnet)]
    cluster: Cluster,

    /// Override the RPC URL. Takes precedence over --cluster.
    #[arg(long)]
    rpc_url: Option<String>,

    /// Owner pubkey for the vault. If omitted, --owner-keypair is read and its pubkey is used.
    #[arg(long)]
    owner: Option<Pubkey>,

    /// Owner keypair path used only to infer the owner pubkey when --owner is omitted.
    #[arg(long, default_value = "~/.config/solana/id.json")]
    owner_keypair: String,

    /// Current ephemeral authority keypair path. This keypair signs and pays for execution.
    #[arg(long)]
    executor_keypair: String,

    /// Nonce of the intent to execute.
    #[arg(long)]
    nonce: u64,
}

#[derive(Debug, Parser)]
struct DevnetStatusArgs {
    /// Override the RPC URL.
    #[arg(long, default_value = "https://api.devnet.solana.com")]
    rpc_url: String,

    /// Owner and fee-payer keypair path used by CLI examples.
    #[arg(long, default_value = "~/.config/solana/id.json")]
    keypair: String,

    /// Ephemeral authority keypair path used by CLI examples.
    #[arg(long, default_value = "~/.config/solana/id.json")]
    ephemeral_authority_keypair: String,

    /// Nonce used in the printed submit/execute examples.
    #[arg(long, default_value_t = 1)]
    nonce: u64,

    /// Optional payload hash to include in the printed submit-intent command.
    #[arg(long)]
    payload_hash: Option<String>,
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

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::CreateVault(args) => handle_create_vault(args),
        Command::RotateAuthority(args) => handle_rotate_authority(args),
        Command::SubmitIntent(args) => handle_submit_intent(args),
        Command::CancelIntent(args) => handle_cancel_intent(args),
        Command::ExecuteIntent(args) => handle_execute_intent(args),
        Command::DevnetStatus(args) => handle_devnet_status(args),
    }
}

fn handle_create_vault(args: CreateVaultArgs) -> Result<()> {
    let keypair_path = shellexpand::tilde(&args.keypair).into_owned();
    let owner = read_keypair_file(&keypair_path)
        .map_err(|err| anyhow::anyhow!("failed to read keypair at {keypair_path}: {err}"))?;
    let ephemeral_authority = resolve_authority_pubkey(
        args.ephemeral_authority,
        args.ephemeral_authority_keypair.as_deref(),
        "ephemeral authority",
        "ephemeral-authority",
    )?;
    let rpc_url = args
        .rpc_url
        .unwrap_or_else(|| args.cluster.rpc_url().to_string());
    let rpc_client = RpcClient::new_with_commitment(rpc_url.clone(), CommitmentConfig::confirmed());

    let result = create_vault(&rpc_client, &owner, ephemeral_authority)
        .with_context(|| format!("failed to create vault on {rpc_url}"))?;

    println!("vault PDA: {}", result.vault);
    println!("owner: {}", result.owner);
    println!("ephemeral authority: {}", result.ephemeral_authority);
    println!("bump: {}", result.bump);
    println!("signature: {}", result.signature);

    Ok(())
}

fn handle_rotate_authority(args: RotateAuthorityArgs) -> Result<()> {
    let keypair_path = shellexpand::tilde(&args.keypair).into_owned();
    let owner = read_keypair_file(&keypair_path)
        .map_err(|err| anyhow::anyhow!("failed to read keypair at {keypair_path}: {err}"))?;
    let new_ephemeral_authority = resolve_authority_pubkey(
        args.new_ephemeral_authority,
        args.new_ephemeral_authority_keypair.as_deref(),
        "new ephemeral authority",
        "new-ephemeral-authority",
    )?;
    let rpc_url = args
        .rpc_url
        .unwrap_or_else(|| args.cluster.rpc_url().to_string());
    let rpc_client = RpcClient::new_with_commitment(rpc_url.clone(), CommitmentConfig::confirmed());

    let result = rotate_authority(&rpc_client, &owner, new_ephemeral_authority)
        .with_context(|| format!("failed to rotate authority on {rpc_url}"))?;

    println!("vault PDA: {}", result.vault);
    println!("owner: {}", result.owner);
    println!(
        "new ephemeral authority: {}",
        result.new_ephemeral_authority
    );
    println!("signature: {}", result.signature);

    Ok(())
}

fn handle_submit_intent(args: SubmitIntentArgs) -> Result<()> {
    let owner = match args.owner {
        Some(owner) => owner,
        None => {
            let owner_keypair_path = shellexpand::tilde(&args.owner_keypair).into_owned();
            read_keypair_file(&owner_keypair_path)
                .map_err(|err| {
                    anyhow::anyhow!("failed to read owner keypair at {owner_keypair_path}: {err}")
                })?
                .pubkey()
        }
    };
    let ephemeral_authority_keypair_path =
        shellexpand::tilde(&args.ephemeral_authority_keypair).into_owned();
    let ephemeral_authority = read_keypair_file(&ephemeral_authority_keypair_path).map_err(
        |err| {
            anyhow::anyhow!(
                "failed to read ephemeral authority keypair at {ephemeral_authority_keypair_path}: {err}"
            )
        },
    )?;
    let payload_hash = parse_payload_hash(&args.payload_hash)?;
    let rpc_url = args
        .rpc_url
        .unwrap_or_else(|| args.cluster.rpc_url().to_string());
    let rpc_client = RpcClient::new_with_commitment(rpc_url.clone(), CommitmentConfig::confirmed());

    let result = submit_execution_intent(
        &rpc_client,
        owner,
        &ephemeral_authority,
        args.nonce,
        payload_hash,
    )
    .with_context(|| format!("failed to submit execution intent on {rpc_url}"))?;

    println!("intent PDA: {}", result.intent);
    println!("vault PDA: {}", result.vault);
    println!("owner: {}", result.owner);
    println!("ephemeral authority: {}", result.ephemeral_authority);
    println!("nonce: {}", result.nonce);
    println!(
        "payload hash: {}",
        format_payload_hash(&result.payload_hash)
    );
    println!("bump: {}", result.bump);
    println!("signature: {}", result.signature);

    Ok(())
}

fn handle_cancel_intent(args: CancelIntentArgs) -> Result<()> {
    let owner_keypair_path = shellexpand::tilde(&args.owner_keypair).into_owned();
    let owner_keypair = read_keypair_file(&owner_keypair_path).map_err(|err| {
        anyhow::anyhow!("failed to read owner keypair at {owner_keypair_path}: {err}")
    })?;
    let owner = args.owner.unwrap_or_else(|| owner_keypair.pubkey());
    let authority = match args.authority_keypair {
        Some(path) => {
            let authority_keypair_path = shellexpand::tilde(&path).into_owned();
            read_keypair_file(&authority_keypair_path).map_err(|err| {
                anyhow::anyhow!(
                    "failed to read authority keypair at {authority_keypair_path}: {err}"
                )
            })?
        }
        None => owner_keypair,
    };
    let rpc_url = args
        .rpc_url
        .unwrap_or_else(|| args.cluster.rpc_url().to_string());
    let rpc_client = RpcClient::new_with_commitment(rpc_url.clone(), CommitmentConfig::confirmed());

    let result = cancel_intent(&rpc_client, owner, &authority, args.nonce)
        .with_context(|| format!("failed to cancel intent on {rpc_url}"))?;

    println!("intent PDA: {}", result.intent);
    println!("vault PDA: {}", result.vault);
    println!("owner: {}", result.owner);
    println!("authority: {}", result.authority);
    println!("nonce: {}", result.nonce);
    println!("signature: {}", result.signature);

    Ok(())
}

fn handle_execute_intent(args: ExecuteIntentArgs) -> Result<()> {
    let owner = match args.owner {
        Some(owner) => owner,
        None => {
            let owner_keypair_path = shellexpand::tilde(&args.owner_keypair).into_owned();
            read_keypair_file(&owner_keypair_path)
                .map_err(|err| {
                    anyhow::anyhow!("failed to read owner keypair at {owner_keypair_path}: {err}")
                })?
                .pubkey()
        }
    };
    let executor_keypair_path = shellexpand::tilde(&args.executor_keypair).into_owned();
    let executor = read_keypair_file(&executor_keypair_path).map_err(|err| {
        anyhow::anyhow!("failed to read executor keypair at {executor_keypair_path}: {err}")
    })?;
    let rpc_url = args
        .rpc_url
        .unwrap_or_else(|| args.cluster.rpc_url().to_string());
    let rpc_client = RpcClient::new_with_commitment(rpc_url.clone(), CommitmentConfig::confirmed());

    let result = execute_intent(&rpc_client, owner, &executor, args.nonce)
        .with_context(|| format!("failed to execute intent on {rpc_url}"))?;

    println!("intent PDA: {}", result.intent);
    println!("vault PDA: {}", result.vault);
    println!("owner: {}", result.owner);
    println!("executor: {}", result.executor);
    println!("nonce: {}", result.nonce);
    println!("signature: {}", result.signature);

    Ok(())
}

fn handle_devnet_status(args: DevnetStatusArgs) -> Result<()> {
    let keypair_path = shellexpand::tilde(&args.keypair).into_owned();
    let owner = read_keypair_file(&keypair_path)
        .map_err(|err| anyhow::anyhow!("failed to read keypair at {keypair_path}: {err}"))?;
    let ephemeral_keypair_path = shellexpand::tilde(&args.ephemeral_authority_keypair).into_owned();
    let ephemeral_authority = read_keypair_file(&ephemeral_keypair_path).map_err(|err| {
        anyhow::anyhow!(
            "failed to read ephemeral authority keypair at {ephemeral_keypair_path}: {err}"
        )
    })?;
    let rpc_client =
        RpcClient::new_with_commitment(args.rpc_url.clone(), CommitmentConfig::confirmed());
    let owner_balance = rpc_client
        .get_balance(&owner.pubkey())
        .with_context(|| format!("failed to fetch owner balance from {}", args.rpc_url))?;
    let deploy_wallet = DEVNET_DEPLOY_WALLET
        .parse::<Pubkey>()
        .context("devnet deploy wallet constant is not a valid pubkey")?;
    let deploy_balance = rpc_client.get_balance(&deploy_wallet).unwrap_or(0);
    let payload_hash = args
        .payload_hash
        .as_deref()
        .unwrap_or("<PAYLOAD_HASH_FROM_WEB_OR_RELAYER_HASH_PAYLOAD>");

    println!("Shadow SDK devnet status");
    println!("rpc url: {}", args.rpc_url);
    let program_id = stealth_vault_program_id();
    println!("program id: {}", program_id);
    println!(
        "deploy wallet: {} ({} SOL)",
        deploy_wallet,
        format_sol(deploy_balance)
    );
    println!(
        "owner wallet: {} ({} SOL)",
        owner.pubkey(),
        format_sol(owner_balance)
    );
    println!("ephemeral authority: {}", ephemeral_authority.pubkey());
    println!();
    println!("fund deploy wallet:");
    println!("  solana airdrop 2 {deploy_wallet} --url devnet");
    println!();
    println!("deploy program:");
    println!("  anchor build");
    println!("  anchor deploy --provider.cluster devnet");
    println!("  solana program show {} --url devnet", program_id);
    println!();
    println!("test with CLI:");
    println!("  cargo run -p shadow-cli -- create-vault --cluster devnet \\");
    println!("    --keypair {} \\", args.keypair);
    println!(
        "    --ephemeral-authority-keypair {}",
        args.ephemeral_authority_keypair
    );
    println!("  cargo run -p shadow-cli -- submit-intent --cluster devnet \\");
    println!("    --owner {} \\", owner.pubkey());
    println!(
        "    --ephemeral-authority-keypair {} \\",
        args.ephemeral_authority_keypair
    );
    println!("    --nonce {} \\", args.nonce);
    println!("    --payload-hash {payload_hash}");

    Ok(())
}

fn parse_payload_hash(value: &str) -> Result<[u8; 32]> {
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

fn resolve_authority_pubkey(
    explicit_pubkey: Option<Pubkey>,
    keypair_path: Option<&str>,
    label: &str,
    flag_stem: &str,
) -> Result<Pubkey> {
    match (explicit_pubkey, keypair_path) {
        (Some(_), Some(_)) => {
            anyhow::bail!("provide either --{flag_stem} or --{flag_stem}-keypair, not both")
        }
        (Some(pubkey), None) => Ok(pubkey),
        (None, Some(path)) => {
            let expanded_path = shellexpand::tilde(path).into_owned();
            Ok(read_keypair_file(&expanded_path)
                .map_err(|err| {
                    anyhow::anyhow!("failed to read {label} keypair at {expanded_path}: {err}")
                })?
                .pubkey())
        }
        (None, None) => Ok(Keypair::new().pubkey()),
    }
}

fn format_sol(lamports: u64) -> String {
    let whole = lamports / LAMPORTS_PER_SOL;
    let fractional = lamports % LAMPORTS_PER_SOL;
    format!("{whole}.{fractional:09}")
}

fn format_payload_hash(payload_hash: &[u8; 32]) -> String {
    payload_hash
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
