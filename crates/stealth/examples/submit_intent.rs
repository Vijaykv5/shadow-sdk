use shadow_stealth::{create_vault, submit_private_intent, PrivateIntent};
use solana_client::rpc_client::RpcClient;
use solana_sdk::signature::{read_keypair_file, Signer};

fn main() -> anyhow::Result<()> {
    let rpc = RpcClient::new("https://api.devnet.solana.com".to_string());
    let owner = read_keypair_file("~/.config/solana/id.json")
        .map_err(|err| anyhow::anyhow!("failed to read owner keypair: {err}"))?;
    let ephemeral = read_keypair_file("~/.config/solana/id.json")
        .map_err(|err| anyhow::anyhow!("failed to read ephemeral keypair: {err}"))?;
    let payload = br#"{"nonce":1,"kind":"mock_execution","payload":{"message":"hello shadow"},"expires_at":null}"#;
    let private_intent = PrivateIntent::from_bytes(1, payload.as_slice());

    let vault = create_vault(&rpc, &owner, ephemeral.pubkey())?;
    let intent = submit_private_intent(&rpc, owner.pubkey(), &ephemeral, &private_intent)?;

    println!("vault PDA: {}", vault.vault);
    println!("intent PDA: {}", intent.intent);
    println!("payload hash: {}", private_intent.hash_hex());

    Ok(())
}
