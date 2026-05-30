# Feature 1: Ephemeral Stealth Vault Creation

This feature is the execution identity layer for future Shadow SDK perpetual trading infrastructure. It creates a program-owned vault account and lets the owner rotate the temporary execution authority. It does not implement a perp exchange, relayer, liquidation engine, copy trading, or MagicBlock integration.

## Flow

```bash
shadow create-vault
```

The command derives a vault PDA from the owner wallet, initializes the account on-chain, stores the owner, stores an ephemeral authority pubkey, stores the PDA bump, and prints the vault address.

```bash
shadow rotate-authority
```

The command derives the same vault PDA, verifies the owner signer through the program constraints, and replaces the vault's temporary execution authority.

```bash
shadow submit-intent
```

The command derives the vault PDA and an intent PDA, verifies that the signer is the vault's current ephemeral authority, and stores a pending 32-byte payload hash with a nonce. This is the first handoff point for future relayers, keepers, perps engines, or private execution workers. The payload itself stays off-chain; the program records only the commitment hash.

```bash
shadow cancel-intent
```

The command cancels a pending intent. The signer must be either the vault owner or the vault's current ephemeral authority.

```bash
shadow execute-intent
```

The command marks a pending intent as executed. The signer must be the vault's current ephemeral authority. This is still a lifecycle marker only; future relayer work will verify the off-chain payload and perform the real external action before calling this instruction.

## File Ownership

`programs/stealth-vault/` contains the Anchor program. It owns the on-chain account layout plus the `initialize_vault` and `rotate_authority` instructions.

`programs/stealth-vault/src/state.rs` defines the `Vault` account:

```rust
pub struct Vault {
    pub owner: Pubkey,
    pub ephemeral_authority: Pubkey,
    pub bump: u8,
}
```

It also defines the `ExecutionIntent` account:

```rust
pub struct ExecutionIntent {
    pub vault: Pubkey,
    pub ephemeral_authority: Pubkey,
    pub executor: Pubkey,
    pub nonce: u64,
    pub payload_hash: [u8; 32],
    pub status: u8,
    pub created_at: i64,
    pub cancelled_at: i64,
    pub executed_at: i64,
    pub bump: u8,
}
```

Intent status values are:

- `0`: pending
- `1`: cancelled
- `2`: executed

`programs/stealth-vault/src/constants.rs` defines shared PDA seeds so the program and SDK use the same derivation convention.

`programs/stealth-vault/src/instructions/initialize_vault.rs` defines the Anchor accounts context and writes the initial vault state.

`programs/stealth-vault/src/instructions/rotate_authority.rs` defines the authority rotation context. The owner must sign, and the vault PDA must match `[b"vault", owner]`.

`programs/stealth-vault/src/instructions/submit_execution_intent.rs` defines the execution intent context. The current ephemeral authority must sign, and the intent PDA must match `[b"intent", vault, nonce]`.

`programs/stealth-vault/src/instructions/cancel_intent.rs` defines the cancellation context. The owner or current ephemeral authority must sign, and the intent must still be pending.

`programs/stealth-vault/src/instructions/execute_intent.rs` defines the execution marker context. The current ephemeral authority must sign, and the intent must still be pending.

`crates/stealth/` is the Rust SDK. It exposes `derive_vault_pda()` for deterministic vault address derivation, `derive_intent_pda()` for deterministic intent address derivation, `create_vault()` for initialization, `rotate_authority()` for replacing the execution authority, `submit_execution_intent()` for storing a hashed execution intent, `execute_intent()` for marking an intent executed, and `cancel_intent()` for cancelling a pending intent.

`cli/` is the operator-facing binary. It parses command-line flags, loads the owner keypair, creates or accepts an ephemeral authority pubkey, calls the SDK, and prints the result.

## PDA And Bump

A Program Derived Address is a deterministic address controlled by a Solana program, not by a private key. Shadow derives vaults with:

```rust
[b"vault", owner.key().as_ref()]
```

That means each owner has one canonical stealth vault address for this feature. Solana also finds a `bump`, a one-byte value that makes the derived address valid as a PDA. The program stores the bump so future instructions can re-sign for the PDA with the same seeds.

Execution intents are derived with:

```rust
[b"intent", vault.key().as_ref(), nonce.to_le_bytes().as_ref()]
```

That means each vault can have one intent per nonce. Reusing a nonce will fail because the intent PDA already exists.

## Localnet Usage

Build and deploy the Anchor program first:

```bash
anchor build
anchor deploy
```

Then create a vault:

```bash
cargo run -p shadow-cli -- create-vault
```

For devnet:

```bash
cargo run -p shadow-cli -- create-vault --cluster devnet
```

You can provide an existing execution authority:

```bash
cargo run -p shadow-cli -- create-vault --ephemeral-authority <PUBKEY>
```

Or provide an execution authority keypair and use its pubkey:

```bash
cargo run -p shadow-cli -- create-vault --ephemeral-authority-keypair ~/.config/solana/ephemeral.json
```

Rotate the vault to a fresh generated temporary authority:

```bash
cargo run -p shadow-cli -- rotate-authority
```

Or provide the new authority explicitly:

```bash
cargo run -p shadow-cli -- rotate-authority --new-ephemeral-authority <PUBKEY>
```

Or rotate to a saved keypair's pubkey:

```bash
cargo run -p shadow-cli -- rotate-authority --new-ephemeral-authority-keypair ~/.config/solana/ephemeral.json
```

Submit a hashed execution intent:

```bash
cargo run -p shadow-cli -- submit-intent \
  --ephemeral-authority-keypair ~/.config/solana/ephemeral.json \
  --nonce 1 \
  --payload-hash 0000000000000000000000000000000000000000000000000000000000000001
```

The `--payload-hash` value must be 32 bytes encoded as 64 hex characters. In production, hash a structured off-chain intent payload and submit only the hash on-chain.

Mark a pending intent as executed:

```bash
cargo run -p shadow-cli -- execute-intent \
  --owner <OWNER_PUBKEY> \
  --executor-keypair ~/.config/solana/ephemeral.json \
  --nonce 1
```

Cancel a pending intent as the owner:

```bash
cargo run -p shadow-cli -- cancel-intent --nonce 1
```

Cancel as the current ephemeral authority:

```bash
cargo run -p shadow-cli -- cancel-intent \
  --owner <OWNER_PUBKEY> \
  --authority-keypair ~/.config/solana/ephemeral.json \
  --nonce 1
```
