use crate::{EvmError, EvmTransaction, CatalystExecutionResult, EvmCall, EvmCallResult};
use alloy_primitives::{Address, U256, B256};
use revm::primitives::{BlockEnv, TxEnv, Env, CfgEnv, SpecId, ExecutionResult as RevmResult};
use revm::{Database, Evm};

/// EVM execution context for REVM 27.0
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    pub block_number: u64,
    pub block_timestamp: u64,
    pub block_hash: B256,
    pub coinbase: Address,
    pub gas_limit: u64,
    pub base_fee: U256,
    pub chain_id: u64,
    pub spec_id: SpecId,
}

impl ExecutionContext {
    pub fn new(chain_id: u64) -> Self {
        Self {
            block_number: 1,
            block_timestamp: 1234567890,
            block_hash: B256::ZERO,
            coinbase: Address::ZERO,
            gas_limit: 30_000_000,
            base_fee: U256::from(1_000_000_000u64),
            chain_id,
            spec_id: SpecId::CANCUN, // Use latest stable spec
        }
    }

    pub fn to_block_env(&self) -> BlockEnv {
        BlockEnv {
            number: U256::from(self.block_number),
            coinbase: self.coinbase,
            timestamp: U256::from(self.block_timestamp),
            gas_limit: U256::from(self.gas_limit),
            basefee: self.base_fee,
            difficulty: U256::ZERO, // PoS doesn't use difficulty
            prevrandao: Some(self.block_hash),
            blob_excess_gas_and_price: None, // No blob transactions for now
        }
    }

    pub fn to_cfg_env(&self) -> CfgEnv {
        CfgEnv {
            chain_id: self.chain_id,
            spec_id: self.spec_id,
            limit_contract_code_size: Some(0x6000), // 24KB limit (EIP-170)
            ..Default::default()
        }
    }

    /// Update block number and timestamp for new block
    pub fn next_block(&mut self) {
        self.block_number += 1;
        self.block_timestamp += 12; // 12 second block time
        // In practice, this would be the actual block hash
        self.block_hash = B256::from([self.block_number as u8; 32]);
    }

    /// Set specific block parameters
    pub fn set_block(&mut self, number: u64, timestamp: u64, hash: B256) {
        self.block_number = number;
        self.block_timestamp = timestamp;
        self.block_hash = hash;
    }
}

impl Default for ExecutionContext {
    fn default() -> Self {
        Self::new(31337) // Default to Catalyst testnet
    }
}

/// Execute EVM transactions with proper error handling for REVM 27.0
pub struct EvmExecutor<DB: Database> {
    evm: Evm<'static, (), DB>,
    context: ExecutionContext,
}

impl<DB: Database> EvmExecutor<DB> {
    pub fn new(database: DB, context: ExecutionContext) -> Self {
        let evm = Evm::builder()
            .with_db(database)
            .with_empty_env()
            .build();

        Self { evm, context }
    }

    /// Execute a transaction
    pub fn execute_transaction(
        &mut self,
        tx: &EvmTransaction,
    ) -> Result<CatalystExecutionResult, EvmError>
    where
        DB::Error: Into<EvmError>,
    {
        // Set up the environment
        self.setup_env(tx)?;

        // Execute the transaction
        let result = self.evm.transact().map_err(|e| {
            EvmError::Database(format!("EVM execution failed: {:?}", e))
        })?;

        Ok(CatalystExecutionResult::from_revm_result(result, tx))
    }

    /// Execute a read-only call
    pub fn call(
        &mut self,
        call: &EvmCall,
    ) -> Result<EvmCallResult, EvmError>
    where
        DB::Error: Into<EvmError>,
    {
        // Set up environment for call
        self.setup_call_env(call);

        // Execute the call
        let result = self.evm.transact().map_err(|e| {
            EvmError::Database(format!("EVM call failed: {:?}", e))
        })?;

        Ok(EvmCallResult::from_revm_result(result))
    }

    /// Estimate gas for a transaction
    pub fn estimate_gas(
        &mut self,
        tx: &EvmTransaction,
    ) -> Result<u64, EvmError>
    where
        DB::Error: Into<EvmError>,
    {
        // Binary search for optimal gas limit
        let mut low = 21_000u64; // Minimum gas for a transaction
        let mut high = tx.gas_limit.max(10_000_000); // Max reasonable gas
        let mut best_estimate = high;

        while low <= high {
            let mid = (low + high) / 2;
            
            // Create a test transaction with the current gas limit
            let mut test_tx = tx.clone();
            test_tx.gas_limit = mid;
            
            match self.execute_transaction(&test_tx) {
                Ok(result) if result.success => {
                    best_estimate = mid;
                    high = mid.saturating_sub(1);
                }
                _ => {
                    low = mid + 1;
                }
            }
            
            if high <= low {
                break;
            }
        }

        Ok(best_estimate)
    }

    /// Update execution context (for new blocks)
    pub fn update_context(&mut self, context: ExecutionContext) {
        self.context = context;
    }

    /// Get current execution context
    pub fn context(&self) -> &ExecutionContext {
        &self.context
    }

    /// Get mutable reference to the database
    pub fn db_mut(&mut self) -> &mut DB {
        self.evm.db_mut()
    }

    /// Get reference to the database
    pub fn db(&self) -> &DB {
        self.evm.db()
    }

    /// Setup environment for transaction execution
    fn setup_env(&mut self, tx: &EvmTransaction) -> Result<(), EvmError> {
        let tx_env = tx.to_tx_env()?;
        let block_env = self.context.to_block_env();
        let cfg_env = self.context.to_cfg_env();

        self.evm.env = Env {
            block: block_env,
            tx: tx_env,
            cfg: cfg_env,
        };

        Ok(())
    }

    /// Setup environment for read-only call
    fn setup_call_env(&mut self, call: &EvmCall) {
        let tx_env = call.to_tx_env();
        let block_env = self.context.to_block_env();
        let cfg_env = self.context.to_cfg_env();

        self.evm.env = Env {
            block: block_env,
            tx: tx_env,
            cfg: cfg_env,
        };
    }
}

/// Batch executor for multiple transactions
pub struct BatchExecutor<DB: Database> {
    executor: EvmExecutor<DB>,
    results: Vec<CatalystExecutionResult>,
}

impl<DB: Database> BatchExecutor<DB> {
    pub fn new(database: DB, context: ExecutionContext) -> Self {
        Self {
            executor: EvmExecutor::new(database, context),
            results: Vec::new(),
        }
    }

    /// Execute a batch of transactions
    pub fn execute_batch(
        &mut self,
        transactions: &[EvmTransaction],
    ) -> Result<Vec<CatalystExecutionResult>, EvmError>
    where
        DB::Error: Into<EvmError>,
    {
        self.results.clear();
        self.results.reserve(transactions.len());

        for tx in transactions {
            let result = self.executor.execute_transaction(tx)?;
            self.results.push(result);
        }

        Ok(self.results.clone())
    }

    /// Get results from last batch execution
    pub fn results(&self) -> &[CatalystExecutionResult] {
        &self.results
    }

    /// Move to next block
    pub fn next_block(&mut self) {
        self.executor.context.next_block();
    }

    /// Get the underlying executor
    pub fn executor(&self) -> &EvmExecutor<DB> {
        &self.executor
    }

    /// Get mutable reference to the underlying executor
    pub fn executor_mut(&mut self) -> &mut EvmExecutor<DB> {
        &mut self.executor
    }
}

/// Helper functions for common EVM operations
pub mod utils {
    use super::*;
    use keccak_hash::keccak;
    use alloy_primitives::Bytes;

    /// Calculate contract address for CREATE opcode
    pub fn calculate_create_address(sender: &Address, nonce: u64) -> Address {
        use alloy_rlp::Encodable;
        
        let mut stream = alloy_rlp::encode_list::<[&dyn Encodable; 2], &dyn Encodable>(&[sender, &nonce]);
        let hash = keccak(&stream);
        Address::from_slice(&hash[12..])
    }

    /// Calculate contract address for CREATE2 opcode
    pub fn calculate_create2_address(
        sender: &Address,
        salt: &B256,
        init_code: &[u8],
    ) -> Address {
        let mut hasher = keccak_hash::Keccak::new_keccak256();
        hasher.update(&[0xff]);
        hasher.update(sender.as_slice());
        hasher.update(salt.as_slice());
        hasher.update(&keccak(init_code));
        
        let hash = hasher.finalize();
        Address::from_slice(&hash[12..])
    }

    /// Check if address is a contract (has code)
    pub fn is_contract<DB: Database>(
        database: &mut DB,
        address: &Address,
    ) -> Result<bool, DB::Error> {
        match database.basic(*address)? {
            Some(account) => Ok(account.code.is_some() && !account.code.unwrap().is_empty()),
            None => Ok(false),
        }
    }

    /// Encode function call data
    pub fn encode_function_call(selector: [u8; 4], params: &[u8]) -> Bytes {
        let mut data = Vec::with_capacity(4 + params.len());
        data.extend_from_slice(&selector);
        data.extend_from_slice(params);
        Bytes::from(data)
    }

    /// Decode function selector from call data
    pub fn decode_function_selector(data: &[u8]) -> Option<[u8; 4]> {
        if data.len() >= 4 {
            let mut selector = [0u8; 4];
            selector.copy_from_slice(&data[0..4]);
            Some(selector)
        } else {
            None
        }
    }

    /// Get the revert reason from execution result
    pub fn get_revert_reason(result: &CatalystExecutionResult) -> Option<String> {
        if !result.success && result.return_data.len() >= 68 {
            // Standard revert reason encoding: Error(string)
            // 0x08c379a0 (4 bytes) + offset (32 bytes) + length (32 bytes) + string
            let data = &result.return_data;
            if data.len() >= 4 && &data[0..4] == &[0x08, 0xc3, 0x79, 0xa0] {
                // Extract string length
                if let Some(length_bytes) = data.get(36..68) {
                    let length = U256::from_be_slice(length_bytes).to::<usize>();
                    if let Some(reason_bytes) = data.get(68..68 + length) {
                        return String::from_utf8(reason_bytes.to_vec()).ok();
                    }
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::InMemoryDatabase;

    #[test]
    fn test_execution_context() {
        let context = ExecutionContext::new(31337);
        assert_eq!(context.chain_id, 31337);
        assert_eq!(context.spec_id, SpecId::CANCUN);

        let block_env = context.to_block_env();
        assert_eq!(block_env.number, U256::from(1));
        assert_eq!(block_env.gas_limit, U256::from(30_000_000));

        let cfg_env = context.to_cfg_env();
        assert_eq!(cfg_env.chain_id, 31337);
        assert_eq!(cfg_env.spec_id, SpecId::CANCUN);
    }

    #[test]
    fn test_context_next_block() {
        let mut context = ExecutionContext::new(31337);
        let initial_block = context.block_number;
        let initial_timestamp = context.block_timestamp;

        context.next_block();
        
        assert_eq!(context.block_number, initial_block + 1);
        assert_eq!(context.block_timestamp, initial_timestamp + 12);
    }

    #[test]
    fn test_utils_create_address() {
        let sender = Address::from([1u8; 20]);
        let nonce = 0;
        
        let address = utils::calculate_create_address(&sender, nonce);
        // Should produce a deterministic address
        assert_ne!(address, Address::ZERO);
        
        // Same inputs should produce same result
        let address2 = utils::calculate_create_address(&sender, nonce);
        assert_eq!(address, address2);
    }

    #[test]
    fn test_utils_function_encoding() {
        let selector = [0x12, 0x34, 0x56, 0x78];
        let params = &[0xab, 0xcd, 0xef];
        
        let encoded = utils::encode_function_call(selector, params);
        assert_eq!(encoded.len(), 7);
        assert_eq!(&encoded[0..4], &selector);
        assert_eq!(&encoded[4..], params);
        
        let decoded_selector = utils::decode_function_selector(&encoded).unwrap();
        assert_eq!(decoded_selector, selector);
    }

    #[test]
    fn test_executor_creation() {
        let db = InMemoryDatabase::new();
        let context = ExecutionContext::new(31337);
        let executor = EvmExecutor::new(db, context);
        
        assert_eq!(executor.context().chain_id, 31337);
        assert_eq!(executor.context().block_number, 1);
    }

    #[test]
    fn test_batch_executor() {
        let db = InMemoryDatabase::new();
        let context = ExecutionContext::new(31337);
        let mut batch_executor = BatchExecutor::new(db, context);
        
        // Test that we can create batch executor
        assert_eq!(batch_executor.executor().context().chain_id, 31337);
        
        // Test next block functionality
        let initial_block = batch_executor.executor().context().block_number;
        batch_executor.next_block();
        assert_eq!(batch_executor.executor().context().block_number, initial_block + 1);
    }
}