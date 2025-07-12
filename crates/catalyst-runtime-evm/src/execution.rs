// execution.rs - Re-export from lib.rs and add stub types

use revm::primitives::{Address, U256, B256};

// Stub execution context for compatibility
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    pub block_number: u64,
    pub block_timestamp: u64,
    pub block_hash: B256,
    pub coinbase: Address,
    pub gas_limit: u64,
    pub base_fee: U256,
    pub chain_id: u64,
}

// Stub executor for compatibility
pub struct EvmExecutor<DB> {
    _db: std::marker::PhantomData<DB>,
}

impl<DB> EvmExecutor<DB> {
    pub fn new(_database: DB, _context: ExecutionContext) -> Self {
        Self {
            _db: std::marker::PhantomData,
        }
    }
}