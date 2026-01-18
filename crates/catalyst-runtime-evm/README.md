# Catalyst Runtime EVM

A high-performance Ethereum Virtual Machine runtime designed for the Catalyst Network, built on top of REVM 27.0.

## Features

- **Full EVM Compatibility**: Execute Ethereum smart contracts with complete compatibility
- **REVM 27.0 Integration**: Built on the latest REVM for optimal performance
- **Catalyst Extensions**: Support for cross-runtime calls, DFS integration, and confidential transactions
- **Async Support**: Full async/await support for integration with modern Rust applications
- **Comprehensive Testing**: Extensive test suite covering all major EVM operations

## Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
catalyst-runtime-evm = "0.1.0"
```

### Basic Usage

```rust
use catalyst_runtime_evm::{
    CatalystDatabase, EvmExecutor, ExecutionContext, EvmTransaction
};
use alloy_primitives::{Address, U256};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create database and execution context
    let db = CatalystDatabase::new();
    let context = ExecutionContext::new(31337); // Chain ID
    let mut executor = EvmExecutor::new(db, context);
    
    // Create a simple transfer transaction
    let tx = EvmTransaction::transfer(
        Address::from([1u8; 20]),    // from
        Address::from([2u8; 20]),    // to
        U256::from(1000),            // value
        0,                           // nonce
        U256::from(21000),           // gas price
    );
    
    // Execute transaction
    let result = executor.execute_transaction(&tx)?;
    println!("Gas used: {}", result.gas_used());
    
    Ok(())
}
```

### Contract Deployment

```rust
use catalyst_runtime_evm::{EvmTransaction, execution::utils};
use alloy_primitives::{Address, U256, Bytes};

// Deploy a contract
let bytecode = Bytes::from(vec![0x60, 0x60, 0x60, 0x40, 0x52]); // Example bytecode
let deploy_tx = EvmTransaction::deploy_contract(
    deployer_address,
    bytecode,
    U256::ZERO,      // value
    0,               // nonce
    U256::from(20_000_000_000u64), // gas price
    1_000_000,       // gas limit
);

let result = executor.execute_transaction(&deploy_tx)?;
if let Some(contract_address) = result.created_address {
    println!("Contract deployed at: {:?}", contract_address);
}
```

### Read-Only Calls

```rust
use catalyst_runtime_evm::EvmCall;

// Make a read-only call to a contract
let call = EvmCall {
    from: caller_address,
    to: contract_address,
    value: None,
    data: Some(function_call_data),
    gas_limit: Some(100_000),
};

let result = executor.call(&call)?;
println!("Return data: {:?}", result.return_data);
```

## Architecture

### Core Components

- **CatalystDatabase**: In-memory database implementation with Catalyst-specific precompiles
- **EvmExecutor**: Main execution engine for transactions and calls
- **ExecutionContext**: Block and network configuration for EVM execution
- **Types**: Comprehensive type system for transactions, calls, and results

### Database Layer

The database layer provides a clean abstraction over state management:

```rust
use catalyst_runtime_evm::CatalystDatabase;

let mut db = CatalystDatabase::new()
    .with_genesis_accounts(vec![(address, balance)])
    .with_precompiles();
```

### Execution Context

Configure the EVM execution environment:

```rust
use catalyst_runtime_evm::ExecutionContext;
use revm::primitives::SpecId;

let mut context = ExecutionContext::new(31337);
context.set_block(100, 1234567890, block_hash);
```

## Advanced Features

### Catalyst Precompiles

The runtime includes Catalyst-specific precompiled contracts:

- **0x100**: Consensus operations
- **0x101**: DFS operations  
- **0x102**: Cross-runtime calls
- **0x103**: Confidential transactions

### Batch Execution

Execute multiple transactions efficiently:

```rust
use catalyst_runtime_evm::BatchExecutor;

let mut batch = BatchExecutor::new(db, context);
let results = batch.execute_batch(&transactions)?;
```

### Gas Estimation

Estimate gas requirements for transactions:

```rust
let estimated_gas = executor.estimate_gas(&transaction)?;
println!("Estimated gas: {}", estimated_gas);
```

## Examples

Run the included examples:

```bash
# Simple transfer
cargo run --example simple_transfer

# Contract deployment
cargo run --example deploy_contract

# Batch execution
cargo run --example batch_execution
```

## Testing

Run the test suite:

```bash
cargo test
```

Run benchmarks:

```bash
cargo bench
```

## Configuration

### Features

- `std` (default): Standard library support
- `ethereum-tests`: Enable Ethereum official test suite
- `debug-mode`: Enable debug tracing
- `cross-runtime`: Enable cross-runtime call support
- `confidential-tx`: Enable confidential transaction support
- `dfs-integration`: Enable DFS integration
- `catalyst-core-integration`: Enable integration with catalyst-core

### Environment Variables

- `RUST_LOG`: Set logging level (debug, info, warn, error)
- `EVM_TRACE`: Enable EVM execution tracing

## Performance

The runtime is optimized for high-performance execution:

- Zero-copy operations where possible
- Efficient state management with RocksDB backend
- Optimized gas metering
- Batch execution support

## Contributing

Contributions are welcome! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.