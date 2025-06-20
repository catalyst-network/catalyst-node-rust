use crate::{EvmError, EvmTransaction, EvmExecutionResult, EvmCall, EvmCallResult};
use ethereum_types::{Address, U256, H256};
use revm::primitives::{BlockEnv, TxEnv, Env, CfgEnv, SpecId, ExecutionResult as RevmResult};
use revm::{Database, EVM};

/// EVM execution context
pub struct ExecutionContext {
    pub block_number: u64,
    pub block_timestamp: u64,
    pub block_hash: H256,
    pub coinbase: Address,
    pub gas_limit: u64,
    pub base_fee: U256,
    pub chain_id: u64,
}

impl ExecutionContext {
    pub fn to_block_env(&self) -> BlockEnv {
        BlockEnv {
            number: U256::from(self.block_number),
            coinbase: self.coinbase,
            timestamp: U256::from(self.block_timestamp),
            gas_limit: U256::from(self.gas_limit),
            basefee: self.base_fee,
            difficulty: U256::zero(), // PoS doesn't use difficulty
            prevrandao: Some(self.block_hash.into()),
            // Remove blob_excess_gas_and_price for older revm version
        }
    }

    pub fn to_cfg_env(&self) -> CfgEnv {
        CfgEnv {
            chain_id: self.chain_id,
            spec_id: SpecId::LONDON, // Use London hard fork by default
            perf_analyse_created_bytecodes: revm::primitives::AnalysisKind::Analyse,
            limit_contract_code_size: Some(0xc000), // 48KB limit
            memory_limit: 2u64.pow(32), // 4GB memory limit
            disable_balance_check: false,
            disable_block_gas_limit: false,
            disable_eip3607: false,
            disable_gas_refund: false,
            disable_base_fee: false,
        }
    }
}

/// Execute EVM transactions with proper error handling
pub struct EvmExecutor<DB: Database> {
    database: DB,
    context: ExecutionContext,
}

impl<DB: Database> EvmExecutor<DB> {
    pub fn new(database: DB, context: ExecutionContext) -> Self {
        Self { database, context }
    }

    /// Execute a transaction
    pub fn execute_transaction(
        &mut self,
        tx: &EvmTransaction,
    ) -> Result<EvmExecutionResult, EvmError>
    where
        DB::Error: Into<EvmError>,
    {
        let tx_env = tx.to_tx_env()?;
        let block_env = self.context.to_block_env();
        let cfg_env = self.context.to_cfg_env();

        let env = Env {
            block: block_env,
            tx: tx_env,
            cfg: cfg_env,
        };

        let mut evm = EVM::builder()
            .with_db(&mut self.database)
            .with_env(Box::new(env))
            .build();

        let result = evm.transact().map_err(|e| {
            EvmError::Database(format!("EVM execution failed: {:?}", e))
        })?;

        Ok(EvmExecutionResult::from_revm_result(result, tx))
    }

    /// Execute a read-only call
    pub fn call(
        &mut self,
        call: &EvmCall,
    ) -> Result<EvmCallResult, EvmError>
    where
        DB::Error: Into<EvmError>,
    {
        let tx_env = TxEnv {
            caller: call.from,
            gas_limit: call.gas_limit.unwrap_or(50_000_000),
            gas_price: U256::zero(),
            transact_to: revm::primitives::TransactTo::Call(call.to),
            value: call.value.unwrap_or(U256::zero()),
            data: call.data.clone().unwrap_or_default().into(),
            gas_priority_fee: None,
            chain_id: Some(self.context.chain_id),
            nonce: None,
            access_list: Vec::new(),
        };

        let block_env = self.context.to_block_env();
        let cfg_env = self.context.to_cfg_env();

        let env = Env {
            block: block_env,
            tx: tx_env,
            cfg: cfg_env,
        };

        let mut evm = EVM::builder()
            .with_db(&mut self.database)
            .with_env(Box::new(env))
            .build();

        let result = evm.transact().map_err(|e| {
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
            
            let mut test_tx = tx.clone();
            test_tx.gas_limit = mid;
            
            match self.execute_transaction(&test_tx) {
                Ok(result) if result.success => {
                    best_estimate = mid;
                    high = mid - 1;
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
}

/// Helper functions for common EVM operations
pub mod utils {
    use super::*;
    use keccak_hash::keccak;

    /// Calculate contract address for CREATE opcode
    pub fn calculate_create_address(sender: &Address, nonce: u64) -> Address {
        use ethabi::ethereum_types::*;
        
        let mut stream = ethabi::encode(&[
            ethabi::Token::Address(*sender),
            ethabi::Token::Uint(U256::from(nonce)),
        ]);
        
        let hash = keccak(&stream);
        Address::from_slice(&hash[12..])
    }

    /// Calculate contract address for CREATE2 opcode
    pub fn calculate_create2_address(
        sender: &Address,
        salt: &H256,
        init_code: &[u8],
    ) -> Address {
        let mut hasher = keccak_hash::Keccak::new_keccak256();
        hasher.update(&[0xff]);
        hasher.update(sender.as_bytes());
        hasher.update(salt.as_bytes());
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::InMemoryDatabase;

    #[test]
    fn test_execution_context() {
        let context = ExecutionContext {
            block_number: 1,
            block_timestamp: 1234567890,
            block_hash: H256::zero(),
            coinbase: Address::zero(),
            gas_limit: 30_000_000,
            base_fee: U256::from(1_000_000_000u64),
            chain_id: 31337,
        };

        let block_env = context.to_block_env();
        assert_eq!(block_env.number, U256::from(1));
        assert_eq!(block_env.gas_limit, U256::from(30_000_000));

        let cfg_env = context.to_cfg_env();
        assert_eq!(cfg_env.chain_id, 31337);
    }

    #[test]
    fn test_utils_create_address() {
        let sender = Address::from_low_u64_be(1);
        let nonce = 0;
        
        let address = utils::calculate_create_address(&sender, nonce);
        // Should produce a deterministic address
        assert_ne!(address, Address::zero());
    }
}