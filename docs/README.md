# Catalyst Docs

These docs are written against the current `catalyst-node-rust` implementation (CLI: `catalyst-cli`).

## Guides

- **Node operators**: [`node-operator-guide.md`](./node-operator-guide.md)
  - Build + run a local node
  - Build + run a 3-node public testnet (Vultr-style) with EU as boot + RPC
  - Firewall ports and common pitfalls

- **Users**: [`user-guide.md`](./user-guide.md)
  - Using the dev/test faucet
  - Sending a simple payment transaction
  - Deploying + calling an EVM contract (REVM-backed)

- **Testers**: [`tester-guide.md`](./tester-guide.md)
  - Health checks (status/head advancing)
  - RPC checks, peer checks, and basic troubleshooting

- **Builders**: [`builder-guide.md`](./builder-guide.md)
  - RPC methods currently implemented
  - Contract/runtime notes and current limitations

- **Wallets / Integrators**: [`wallet-interop.md`](./wallet-interop.md)
  - Canonical address format
  - v1 transaction signing payload + wire encoding

- **Explorer / Indexer handoff**: [`explorer-handoff.md`](./explorer-handoff.md)
  - Chain model + compatibility notes
  - Practical indexing loop (blocks-by-range + receipts)

- **Sync / Fast sync**: [`sync-guide.md`](./sync-guide.md)
  - Snapshot-based node restore workflow
  - Verifying chain identity + head

## Important implementation notes (current state)

- **Consensus traffic is P2P on `30333/tcp`**, not RPC. RPC is only for client interaction.
- **Faucet** is *not* an ERC20 contract. It’s a deterministic, pre-funded account used for dev/test flows.
- **Tokenomics / fees** are currently minimal scaffolding (see Builder guide).

