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
