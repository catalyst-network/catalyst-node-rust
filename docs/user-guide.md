# User guide (faucet, payments, deploy + call)

This guide assumes there is a reachable RPC endpoint, e.g.:

- `RPC_URL=http://<EU_PUBLIC_IP>:8545`
- Example: `RPC_URL=http://45.32.177.248:8545`

All commands below are client-side; you can run them from your laptop/desktop.

## 0) Sanity check: you are on the intended network

Before sending transactions, confirm chain identity:

```bash
curl -s -X POST "$RPC_URL" -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"catalyst_getSyncInfo","params":[]}'
```

Check `network_id` and `chain_id` match what operators published (see [`network-identity.md`](./network-identity.md)).

## 1) Faucet (dev/test implementation)

In the current implementation, the faucet is **not** an ERC20 contract.

It is a deterministic, pre-funded account:
- faucet private key bytes are `[0xFA; 32]`
- on node startup, if missing from state, it is initialized with balance `1_000_000`

Create the faucet key file (exactly 64 hex chars; no `0x` prefix needed):

```bash
python3 -c 'print("fa"*32)' > faucet.key
```

Derive faucet pubkey:

```bash
FAUCET_PK="$(./target/release/catalyst-cli pubkey --key-file faucet.key | tail -n 1)"
echo "FAUCET_PK=$FAUCET_PK"
```

Confirm faucet is funded:

```bash
./target/release/catalyst-cli balance "$FAUCET_PK" --rpc-url "$RPC_URL"
```

## 2) Send a simple payment transaction

Recipient addresses in this scaffold are **32-byte hex pubkeys** (node IDs).

Example:

```bash
RECIP="<RECIPIENT_PUBKEY_HEX>"
./target/release/catalyst-cli send "$RECIP" 3 --key-file faucet.key --rpc-url "$RPC_URL"
sleep 60
./target/release/catalyst-cli balance "$RECIP" --rpc-url "$RPC_URL"
```

Notes:
- `send` returning a `tx_id` means the RPC accepted the transaction. Inclusion can take a few cycles.

## 3) Deploy + call an EVM contract (REVM-backed)

This repo ships a deterministic initcode fixture at:
- `testdata/evm/return_2a_initcode.hex`

It deploys a contract whose runtime bytecode returns `0x2a` (42) on empty calldata.

### Use a fresh wallet for EVM

If you used the faucet key to send payments first, EVM deploys from that same key can fail due to nonce interactions.
Recommended workflow:

1) Generate a fresh wallet key
2) Fund it from the faucet
3) Deploy + call using the fresh wallet key

Create a new wallet key:

```bash
python3 -c 'import os,binascii; print(binascii.hexlify(os.urandom(32)).decode())' > evm_wallet.key
EVM_PK="$(./target/release/catalyst-cli pubkey --key-file evm_wallet.key | tail -n 1)"
echo "EVM_PK=$EVM_PK"
```

Fund it:

```bash
./target/release/catalyst-cli send "$EVM_PK" 100 --key-file faucet.key --rpc-url "$RPC_URL"
sleep 60
./target/release/catalyst-cli balance "$EVM_PK" --rpc-url "$RPC_URL"
```

### Deploy

```bash
./target/release/catalyst-cli deploy testdata/evm/return_2a_initcode.hex \
  --key-file evm_wallet.key --rpc-url "$RPC_URL" --runtime evm
```

Copy the printed `contract_address` to `ADDR`.

Wait for code to appear:

```bash
ADDR="0x<PASTE_CONTRACT_ADDRESS>"

while true; do
  code=$(curl -s -X POST "$RPC_URL" -H 'content-type: application/json' \
    -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"catalyst_getCode\",\"params\":[\"$ADDR\"]}" \
    | python3 -c 'import sys,json; print(json.load(sys.stdin).get("result",""))')
  echo "code=$code"
  [ "$code" != "0x" ] && break
  sleep 5
done
```

### Call

Empty calldata:

```bash
./target/release/catalyst-cli call "$ADDR" 0x \
  --key-file evm_wallet.key --rpc-url "$RPC_URL" --value 0
```

Poll the persisted return value:

```bash
while true; do
  ret=$(curl -s -X POST "$RPC_URL" -H 'content-type: application/json' \
    -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"catalyst_getLastReturn\",\"params\":[\"$ADDR\"]}" \
    | python3 -c 'import sys,json; print(json.load(sys.stdin).get("result",""))')
  echo "ret=$ret"
  [ "$ret" != "0x" ] && break
  sleep 5
done
```

Expected:

`0x000000000000000000000000000000000000000000000000000000000000002a`

