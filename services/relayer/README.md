# Shadow Relayer

The relayer is the first off-chain worker for Shadow SDK. It verifies a private payload against an on-chain `ExecutionIntent` hash, runs the private action, then marks the intent executed.

This first version supports `mock_execution`, a real `system_transfer` action, and schema-validated `perps_order` intents. It has a route policy for public RPC vs private/bundle execution, but the production Drift/Jito adapters are not wired yet.

## Payload Format

```json
{
  "nonce": 1,
  "kind": "mock_execution",
  "payload": {
    "message": "hello shadow"
  },
  "expires_at": null
}
```

For a real lamport transfer from the executor keypair:

```json
{
  "nonce": 2,
  "kind": "system_transfer",
  "payload": {
    "to": "<RECIPIENT_PUBKEY>",
    "lamports": 1000000
  },
  "expires_at": null
}
```

For a perps order intent:

```json
{
  "nonce": 3,
  "kind": "perps_order",
  "payload": {
    "venue": "mock",
    "market": "SOL-PERP",
    "side": "long",
    "size_base_lots": 10,
    "limit_price": 150000000,
    "max_slippage_bps": 50,
    "reduce_only": false,
    "client_order_id": "shadow-demo-1"
  },
  "route": {
    "kind": "mock_private_bundle",
    "tip_lamports": 5000
  },
  "expires_at": null
}
```

`perps_order` intents intentionally cannot execute over `public_rpc`. Use `mock_private_bundle` for local development. `jito_bundle` is accepted by the schema and requires a positive `tip_lamports`, but production Jito block-engine submission still needs an adapter.

The relayer hashes the exact payload file bytes with Solana's hash function. Submit that hash on-chain as the intent `payload_hash`.

## Commands

You can pass repeated relayer settings with CLI flags or a TOML config file:

```toml
cluster = "localnet"
owner = "<OWNER_PUBKEY>"
executor_keypair = "~/.config/solana/ephemeral.json"
payload = "examples/mock-intent.json"
payload_dir = "payloads"
poll_seconds = 5
max_retries = 3
```

CLI flags override config values:

```bash
cargo run -p shadow-relayer -- run --config examples/relayer.localnet.toml --watch
```

Create the queue folders:

```bash
cargo run -p shadow-relayer -- init-queue --payload-dir payloads
```

Check queue counts:

```bash
cargo run -p shadow-relayer -- queue-status --payload-dir payloads
```

Print the hash for a payload:

```bash
cargo run -p shadow-relayer -- hash-payload --payload examples/mock-intent.json
```

Verify and execute one pending intent:

```bash
cargo run -p shadow-relayer -- execute-once \
  --config examples/relayer.localnet.toml \
  --owner <OWNER_PUBKEY> \
  --executor-keypair ~/.config/solana/ephemeral.json \
  --payload examples/mock-intent.json
```

The executor keypair must be the vault's current ephemeral authority.

## HTTP API

For app integrations, run the relayer as a stateless HTTP backend:

```bash
cargo run -p shadow-relayer -- serve \
  --config examples/relayer.localnet.toml \
  --executor-keypair ~/.config/solana/ephemeral.json \
  --bind 127.0.0.1:8787
```

Check health:

```bash
curl http://127.0.0.1:8787/health
```

Verify and execute one pending intent:

```bash
curl -X POST http://127.0.0.1:8787/execute-once \
  -H 'content-type: application/json' \
  -d '{
    "owner": "<OWNER_PUBKEY>",
    "payload": {
      "nonce": 1,
      "kind": "mock_execution",
      "payload": {
        "message": "hello shadow"
      },
      "expires_at": null
    }
  }'
```

The API hashes the canonical compact JSON inside `payload`, verifies that hash
against the on-chain intent, executes the private action, and then marks the
intent executed. There is no database in this mode; the on-chain intent account
is the source of truth.

## Queue Layout

The `run` command treats `--payload-dir` as a queue root and creates these folders if they do not exist:

```text
payloads/
  pending/
  executed/
  failed/
```

Place new private payload JSON files in `pending/`. Successful payloads move to `executed/`. Failed payloads move to `failed/` with a matching `.error` file that records the reason.

Run one directory scan and execute every matching pending intent:

```bash
cargo run -p shadow-relayer -- run \
  --config examples/relayer.localnet.toml \
  --owner <OWNER_PUBKEY> \
  --executor-keypair ~/.config/solana/ephemeral.json \
  --payload-dir payloads \
  --max-retries 3
```

Run continuously:

```bash
cargo run -p shadow-relayer -- run \
  --owner <OWNER_PUBKEY> \
  --executor-keypair ~/.config/solana/ephemeral.json \
  --payload-dir payloads \
  --watch \
  --poll-seconds 5
```

Each `.json` file in `pending/` is parsed, expiry-checked, hash-checked against the on-chain intent, and only then submitted as an `execute_intent` transaction.

The queue uses a sidecar `.lock` file while a payload is being processed so two relayers do not process the same file at once. Failed attempts stay in `pending/` until `--max-retries` is reached. The relayer records retry metadata in `.attempts` and `.error` sidecar files, then moves the payload to `failed/` with a final `.error` metadata file after the retry limit.
