# Shadow SDK Repository Structure

Keep Shadow SDK minimal at the root, but leave room to grow.

## Current Structure

```text
shadow-sdk/
├── Anchor.toml
├── Cargo.toml
├── README.md
├── cli/
├── crates/
│   └── stealth/
├── docs/
├── examples/
├── idl/
│   └── stealth_vault.json
├── programs/
│   └── stealth-vault/
└── services/
    └── relayer/
```

## Why These Folders Exist

`programs/` is for on-chain Anchor programs. Add one folder per program, for example `programs/shadow-execution`.

`crates/stealth/` is the reusable Rust SDK for vault and intent account derivation plus transaction builders. Add more crates only when there is real shared logic to extract.

`cli/` is the operator/developer command-line tool. Keep it thin; real logic should live in `crates/`.

`services/relayer/` is for the first infrastructure worker. The current relayer verifies a private payload file against an on-chain intent hash and marks the intent executed. Add more service folders only when they become real deployable binaries.

`idl/` is for stable, checked-in Anchor IDLs that SDK users can consume. Anchor build output still goes to `target/idl`; copy stable IDLs here when they are part of the public interface.

`examples/` is for small usage examples. Do not put core protocol logic here.

`docs/` is for architecture notes and runbooks.

## Anchor vs Cargo

Anchor owns:

- `Anchor.toml`
- `programs/*`
- generated `target/idl/*`
- generated `target/deploy/*`

Cargo owns:

- root `Cargo.toml`
- `Cargo.lock`
- `cli/`
- `crates/*`
- `services/*`
- `programs/*` when added as workspace members

Use exactly one `Anchor.toml` and exactly one root Cargo workspace. Do not run `anchor init` inside subfolders.

## Add Later, Not Now

Only add these when needed:

```text
packages/ts-sdk/          # future TypeScript SDK
services/keeper/          # future keeper bot
services/jito-searcher/   # future Jito bundle/searcher worker
config/                   # real cluster config templates
ops/                      # Docker, Helm, deployment manifests
tests/integration/        # cross-program integration tests
scripts/                  # release/build automation
```

That keeps the repo clean today while still giving Shadow SDK a path to become a serious Solana infrastructure project.
