# Shadow SDK

Shadow SDK is a Solana infrastructure monorepo for private and MEV-resistant execution.

The current implementation focuses on stealth vaults, hashed off-chain execution intents, and a relayer that verifies private payloads before marking intents executed.

The first implemented path is the stealth vault layer:

- create a per-owner vault PDA
- rotate a temporary execution authority
- submit hashed execution intents from the current ephemeral authority
- verify and execute private payloads through the relayer
- validate perps-shaped intents and route policy for future private bundle execution

## Repository Map

See [docs/architecture/repository-structure.md](docs/architecture/repository-structure.md) for the production folder architecture and ownership rules.

## Rust Crates

Shadow SDK is structured so downstream Solana apps can depend on the reusable
SDK crate directly:

```toml
[dependencies]
shadow-stealth = "0.1.0"
```

During early development:

```toml
shadow-stealth = { git = "https://github.com/Vijaykv5/shadow-sdk" }
```

The published crates are:

- `shadow-stealth`: user-facing Rust SDK
- `stealth-vault`: lower-level Anchor program interface crate

See [docs/publishing-crates.md](docs/publishing-crates.md) for the crates.io
publish flow.
