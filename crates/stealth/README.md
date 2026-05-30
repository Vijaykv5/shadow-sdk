# shadow-stealth

Rust SDK for Shadow SDK stealth vaults, private payload hashes, and execution
intents on Solana.

Shadow keeps private execution payloads off-chain. Users or applications submit
only a 32-byte payload hash to the `stealth-vault` program, then a relayer
verifies the private payload bytes against that hash before execution.

## Install

```toml
[dependencies]
shadow-stealth = "0.1.0"
```

During early development, you can depend on the Git repo:

```toml
shadow-stealth = { git = "https://github.com/Vijaykv5/shadow-sdk" }
```

## Quick Start

```rust
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
    println!("vault: {}", vault.vault);
    println!("payload hash: {}", private_intent.hash_hex());

    let intent = submit_private_intent(&rpc, owner.pubkey(), &ephemeral, &private_intent)?;
    println!("intent: {}", intent.intent);

    Ok(())
}
```

## What This Crate Exposes

- PDA helpers: `derive_vault_pda`, `derive_intent_pda`
- Intent wrapper: `PrivateIntent`
- Payload helpers: `hash_payload_bytes`, `hash_payload_json`,
  `payload_hash_to_hex`, `payload_hash_from_hex`
- Instruction builders for vault and intent operations
- Transaction helpers for create, rotate, submit, cancel, and execute flows

## Publishing Order

`shadow-stealth` is the user-facing SDK crate. The separate `stealth-vault`
crate is useful for developers who need the lower-level Anchor program
interface, but normal users should install `shadow-stealth`.
