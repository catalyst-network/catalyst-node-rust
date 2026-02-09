# Network identity (chain_id / network_id) and numbering

Catalyst nodes and clients use a **chain identity** to prevent cross-network replay and to make tooling deterministic.

There are two identifiers:

- **`protocol.chain_id`** (u64): the **signing / execution domain**. It is used by:
  - wallet v1 signing payload domain separation
  - EVM `chainId` domain separation
- **`protocol.network_id`** (string): a human-readable network name for operators and tooling.

## Your numbering scheme

You chose the following scheme:

- **chain_id** = `Bitcoin Whitepaper date` + `Bitcoin genesis year` + `network index`
- **network_id** = a stable, public string name

Proposed assignments:

| chain_id | network_id | Meaning |
|---:|---|---|
| `200820091` | `catalyst-mainnet` | Mainnet |
| `200820092` | `catalyst-testnet` | Public testnet |
| `200820093` | `catalyst-devnet` | Developer network |
| `200820094` | `catalyst-staging` | Staging / pre-release |

## Rules (important)

- **Never change** `chain_id` for a running network. Changing it creates a **new chain** from the perspective of clients and signatures.
- **Never change** `genesis_hash` for a running network. It is part of the wallet v1 signing domain and the node’s chain identity.
- If you want a “new epoch” / restart of the public network, treat it as a **new network**:
  - bump `chain_id` and/or `network_id`
  - start from a fresh database (or a fresh snapshot made from that new chain)

## Where it is configured

Node config requires:

```toml
[protocol]
chain_id = 200820092
network_id = "catalyst-testnet"
```

Notes:
- Config uses **decimal** `chain_id`.
- RPC returns `chain_id` as a **hex string** (e.g. `"0xbf3c59c"`), so client-side comparisons should normalize.

## How to verify via RPC

From any machine that can reach RPC:

```bash
curl -s -X POST http://<RPC_HOST>:8545 -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"catalyst_getSyncInfo","params":[]}'
```

Check:
- `chain_id`
- `network_id`
- `genesis_hash`

