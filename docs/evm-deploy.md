## EVM contract deployment (Catalyst)

Catalyst supports EVM smart contracts, but deployments are submitted as **Catalyst CTX2 transactions** via `catalyst_sendRawTransaction` (not Ethereum `eth_sendRawTransaction`).

### Quickstart (CLI)

Deploy from a raw hex file (or raw bytes) containing initcode:

```bash
./target/release/catalyst-cli deploy \
  --runtime evm \
  --key-file wallet.key \
  --rpc-url https://YOUR-RPC \
  --wait --verify-code \
  path/to/bytecode.hex
```

Deploy from a Foundry or Hardhat artifact JSON:

```bash
./target/release/catalyst-cli deploy \
  --runtime evm \
  --key-file wallet.key \
  --rpc-url https://YOUR-RPC \
  --wait --verify-code \
  out/MyContract.sol/MyContract.json
```

### Constructor args

`catalyst-cli deploy --args` expects **hex bytes** to append to the contract initcode.

Example:

```bash
./target/release/catalyst-cli deploy \
  --runtime evm \
  --key-file wallet.key \
  --rpc-url https://YOUR-RPC \
  --args 0x<abi-encoded-constructor-args> \
  --wait --verify-code \
  out/MyContract.sol/MyContract.json
```

### How the deploy address is derived

Catalyst uses the Ethereum `CREATE` derivation:

- `evm_sender = last20(sender_pubkey32)`
- `evm_nonce = protocol_nonce - 1` (protocol nonces start at 1; EVM nonce starts at 0)
- `contract_address = keccak256(rlp([evm_sender, evm_nonce]))[12..]`

The CLI prints the computed address as `contract_address: 0x...`.

### Tooling should use `catalyst_getTxDomain`

When signing transactions, tooling should fetch the domain in a **single RPC call**:

- `catalyst_getTxDomain -> { chain_id, genesis_hash, network_id, ... }`

This avoids signature failures caused by load balancers routing `chainId` and `genesisHash` requests to different backends.

