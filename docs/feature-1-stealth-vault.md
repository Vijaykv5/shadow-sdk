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

`programs/stealth-vault/src/constants.rs` defines shared PDA seeds so the program and SDK use the same derivation convention.

`programs/stealth-vault/src/instructions/initialize_vault.rs` defines the Anchor accounts context and writes the initial vault state.

`programs/stealth-vault/src/instructions/rotate_authority.rs` defines the authority rotation context. The owner must sign, and the vault PDA must match `[b"vault", owner]`.

`crates/stealth/` is the Rust SDK. It exposes `derive_vault_pda()` for deterministic address derivation, `create_vault()` for initialization, and `rotate_authority()` for replacing the execution authority.

`cli/` is the operator-facing binary. It parses command-line flags, loads the owner keypair, creates or accepts an ephemeral authority pubkey, calls the SDK, and prints the result.

## PDA And Bump

A Program Derived Address is a deterministic address controlled by a Solana program, not by a private key. Shadow derives vaults with:

```rust
[b"vault", owner.key().as_ref()]
```

That means each owner has one canonical stealth vault address for this feature. Solana also finds a `bump`, a one-byte value that makes the derived address valid as a PDA. The program stores the bump so future instructions can re-sign for the PDA with the same seeds.

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

Rotate the vault to a fresh generated temporary authority:

```bash
cargo run -p shadow-cli -- rotate-authority
```

Or provide the new authority explicitly:

```bash
cargo run -p shadow-cli -- rotate-authority --new-ephemeral-authority <PUBKEY>
```
