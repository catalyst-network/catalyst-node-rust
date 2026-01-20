# Smoke tests

## `smoke_testnet.sh`

Runs a short end-to-end test against a local 3-node testnet:
- starts node1/node2/node3
- waits for node1 RPC
- sends a faucet-funded tx to node1
- restarts node1
- verifies the tx is still included (mempool persistence + rehydrate + rebroadcast)
- stops the testnet

Run via make:

```bash
make smoke-testnet
```

Or directly:

```bash
bash scripts/smoke_testnet.sh
```

Environment variables:
- `AMOUNT` (default `3`)
- `TIMEOUT_SECS` (default `90`)
- `RPC_URL` (default `http://127.0.0.1:8545`)
- `RPC_HOSTPORT` (default `127.0.0.1:8545`)

