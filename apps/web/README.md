# Shadow SDK Web Console

Next.js operator console for composing private intent payloads on devnet,
submitting matching intent hashes on-chain, and handing the private payload to
the relayer API.

## Run

```bash
npm install
npm run dev
```

Open `http://localhost:3000`.

By default the console calls the local relayer API at `http://127.0.0.1:8787`.
Override it with:

```bash
NEXT_PUBLIC_RELAYER_URL=http://127.0.0.1:8787 npm run dev
```

## Flow

1. Connect a browser wallet.
2. Create or refresh the vault PDA for the owner wallet.
3. Compose a payload and generate its hash.
4. Submit the hash on-chain with the ephemeral authority wallet.
5. Start the Rust relayer API:

```bash
cargo run -p shadow-relayer -- serve \
  --cluster devnet \
  --executor-keypair ~/.config/solana/ephemeral.json \
  --bind 127.0.0.1:8787
```

6. Click **Queue in DB** to persist the private payload through the relayer.
7. Click **Check DB** to refresh the queued status.
8. Click **Execute queued** to verify the queued payload and mark the on-chain intent executed.

The web flow hashes canonical compact JSON for the HTTP relayer path. The older
queue/file flow still hashes exact file bytes.

For the quick synchronous demo path, use **Execute once** instead of queueing.

## Checks

```bash
npm run typecheck
npm run build
```
