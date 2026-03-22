# Tokenomics testnet validation

This guide validates that tokenomics v1 behavior matches the docs after a clean testnet reset.

Use this when you want to confirm:

- `0 KAT` default genesis policy is preserved in runtime defaults
- issuance/reward flow is active after genesis
- basic network behavior is still healthy

## 1) Build

```bash
CC=gcc-13 CXX=g++-13 make build
```

## 2) Reset and start fresh local testnet

```bash
make testnet-down
CC=gcc-13 CXX=g++-13 make testnet-up-clean
```

Notes:

- `testnet-up-clean` wipes testnet data and starts fresh state.
- Local harnesses intentionally enable a deterministic faucet for dev UX.
- Protocol defaults remain `faucet_mode=disabled`, `faucet_balance=0` outside this testnet convenience path.

## 3) Capture validator identities

```bash
PK1=$(./target/release/catalyst-cli pubkey --key-file testnet/node1/node.key | tail -n 1)
PK2=$(./target/release/catalyst-cli pubkey --key-file testnet/node2/node.key | tail -n 1)
echo "PK1=$PK1"
echo "PK2=$PK2"
```

## 4) Check chain progression on fresh reset

Confirm head is advancing:

```bash
./target/release/catalyst-cli status --rpc-url http://127.0.0.1:8545
sleep 30
./target/release/catalyst-cli status --rpc-url http://127.0.0.1:8545
```

Expected outcome:

- `applied_cycle` increases between checks
- this confirms post-genesis cycle production is active on the reset testnet

## 5) Validate issuance observability

Query tokenomics summary:

```bash
curl -s -X POST http://127.0.0.1:8545 -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"catalyst_getTokenomicsInfo","params":[]}'
```

Expected outcome:

- `applied_cycle` is non-zero and increasing over time
- `block_reward_atoms` matches configured fixed reward (`1000000000`)
- `estimated_issued_atoms` increases as `applied_cycle` increases
- `fee_burn_bps = 7000`, `fee_to_reward_pool_bps = 3000`, `fee_to_treasury_bps = 0`
- `producer_set_reward_bps = 7000`, `waiting_pool_reward_bps = 3000`

## 6) Validate issuance logic path with tests

Run consensus tests (includes compensation/reward logic paths):

```bash
CC=gcc-13 CXX=g++-13 cargo test -p catalyst-consensus
```

Expected outcome:

- tests pass, confirming deterministic consensus/reward logic behavior in code paths

## 7) Regression checks

Run baseline functional tests against the running testnet:

```bash
CC=gcc-13 CXX=g++-13 make testnet-basic-test
CC=gcc-13 CXX=g++-13 make testnet-contract-test
```

If either command fails, treat it as a regression signal and capture logs before rollout.

**`testnet-contract-test` / “evm wallet funding not observed”:** The contract harness funds a fresh wallet via `send`, then checks balance. `send` submits a tx and returns immediately; the script waits for `catalyst_getTransactionReceipt` to show `applied` before asserting balance. If this still fails, check `catalyst_getTransactionReceipt` for the printed `tx_id`, faucet balance, and node logs.

## 8) Teardown

```bash
make testnet-down
```
