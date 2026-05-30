# Shadow Relayer

The relayer is the first off-chain worker for Shadow SDK. It verifies a private payload against an on-chain `ExecutionIntent` hash, runs the private action, then marks the intent executed.

This first version supports `mock_execution` and a real `system_transfer` action. It does not place swaps, perps orders, Jito bundles, or MagicBlock actions yet.

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

The relayer hashes the exact payload file bytes with Solana's hash function. Submit that hash on-chain as the intent `payload_hash`.

## Commands

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
  --owner <OWNER_PUBKEY> \
  --executor-keypair ~/.config/solana/ephemeral.json \
  --payload examples/mock-intent.json
```

The executor keypair must be the vault's current ephemeral authority.

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
