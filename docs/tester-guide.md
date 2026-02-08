# Tester guide (health checks + troubleshooting)

This guide focuses on black-box testing a running network via RPC and basic node logs.

## Health checks

### 1) Head advances

Run twice (a few cycles apart):

```bash
./target/release/catalyst-cli status --rpc-url http://<RPC_HOST>:8545
sleep 30
./target/release/catalyst-cli status --rpc-url http://<RPC_HOST>:8545
```

Expected:
- `applied_cycle` increases
- `applied_lsu_hash` / `last_lsu_cid` often change

### 2) Peer count (best-effort)

```bash
./target/release/catalyst-cli peers --rpc-url http://<RPC_HOST>:8545
```

Notes:
- `peer_count` is useful for diagnosing connectivity, but it is not the primary liveness signal.
- A network can be live even if the RPC node’s `peer_count` is low (depending on topology).

### 3) Basic payment test

See [`user-guide.md`](./user-guide.md) for the exact faucet/payment steps.

## Common failure modes

### “Insufficient data collected: got 1, required 2”

Meaning: a validator did not collect enough peer inputs to reach required majority for a cycle.

Checklist:
- **Firewall**: ensure `30333/tcp` is open inbound on all nodes.
- **Process**: ensure node is listening on `0.0.0.0:30333`.
- **Bootstrap peers**: ensure nodes list each other under `[network].bootstrap_peers`.
- **Clock drift**: cycles are epoch-aligned; ensure NTP/chrony is active.

### Transactions accepted but not applied (balance doesn’t change)

Meaning: the RPC accepted the tx, but it didn’t make it into an applied LSU yet.

Checklist:
- Wait a few cycles (propagation + leader selection).
- Ensure the RPC node is connected to the validator mesh (`peers`).
- Validate the sender is funded (faucet balance > 0).

## Firewall / network debugging commands

### UFW

```bash
sudo ufw status numbered
sudo journalctl -k --since "10 minutes ago" | grep -i "UFW BLOCK" | tail -n 50 || true
```

### Listening ports

```bash
sudo ss -ltnp | egrep ':30333|:8545' || true
```

### Connectivity

```bash
nc -vz <NODE_PUBLIC_IP> 30333
```

