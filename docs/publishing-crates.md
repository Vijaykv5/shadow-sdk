# Publishing Shadow SDK Crates

Shadow SDK publishes two Rust crates:

1. `stealth-vault`: Anchor program interface crate
2. `shadow-stealth`: user-facing SDK crate

Normal users should install `shadow-stealth`. `stealth-vault` is available for
advanced Anchor/program-interface users.

## Prerequisites

Create a crates.io API token and export it:

```bash
export CARGO_REGISTRY_TOKEN="<YOUR_CRATES_IO_TOKEN>"
```

## Verify

```bash
cargo test -p shadow-stealth
cargo package -p stealth-vault
cargo package -p shadow-stealth
```

## Publish

```bash
cargo publish -p stealth-vault --token "$CARGO_REGISTRY_TOKEN"
cargo publish -p shadow-stealth --token "$CARGO_REGISTRY_TOKEN"
```

## Install

After publishing:

```toml
[dependencies]
shadow-stealth = "0.1.0"
```

During early development:

```toml
shadow-stealth = { git = "https://github.com/Vijaykv5/shadow-sdk" }
```
