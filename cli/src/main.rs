use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use shadow_stealth::{create_vault, rotate_authority};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    pubkey::Pubkey,
    signature::{Keypair, Signer, read_keypair_file},
};

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
    }
}

fn handle_create_vault(args: CreateVaultArgs) -> Result<()> {
    let keypair_path = shellexpand::tilde(&args.keypair).into_owned();
    let owner = read_keypair_file(&keypair_path)
        .map_err(|err| anyhow::anyhow!("failed to read keypair at {keypair_path}: {err}"))?;
    let ephemeral_authority = args
        .ephemeral_authority
        .unwrap_or_else(|| Keypair::new().pubkey());
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
    let new_ephemeral_authority = args
        .new_ephemeral_authority
        .unwrap_or_else(|| Keypair::new().pubkey());
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
