# Shadow SDK Web Console

Next.js operator console for composing private intent payloads, submitting matching
intent hashes on-chain, and handing the private payload to the relayer queue.

## Run

```bash
npm install
npm run dev
```

Open `http://localhost:3000`.

## Flow

1. Connect a browser wallet.
2. Create or refresh the vault PDA for the owner wallet.
3. Compose a payload and generate its hash.
4. Submit the hash on-chain with the ephemeral authority wallet.
5. Save the private payload into `payloads/pending`.
6. Run the Rust relayer to verify, execute, and archive the payload.

## Checks

```bash
npm run typecheck
npm run build
```
