//! Ethereum Virtual Machine runtime for Catalyst Network
//! 
//! Provides EVM compatibility for Ethereum smart contracts and transactions,
//! allowing seamless migration of existing Ethereum applications.

use async_trait::async_trait;
use catalyst_core::{ExecutionResult, Transaction, ExecutionContext, RuntimeError, ResourceEstimate};
use ethereum_types::{Address, U256, H256};
use revm::{
    primitives::{
        AccountInfo, Bytecode, ExecutionResult as RevmResult, 
        HaltReason, Output, TransactTo, TxEnv, Env, BlockEnv, CfgEnv, SpecId,
    },
    Database, EVM,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

pub mod database;
pub mod execution;
pub mod precompiles;
pub mod types;

pub use database::*;
pub use execution::*;
pub use precompiles::*;
pub use types::*;

#[derive(Error, Debug)]
pub enum EvmError {
    #[error("Execution error: {0:?}")]
    Execution(HaltReason),
    #[error("Invalid transaction: {0}")]
    InvalidTransaction(String),
    #[error("Insufficient gas: provided {provided}, required {required}")]
    InsufficientGas { provided: u64, required: u64 },
    #[error("Account not found: {0}")]
    AccountNotFound(Address),
    #[error("Contract not found: {0}")]
    ContractNotFound(Address),
    #[error("Database error: {0}")]
    Database(String),
    #[error("Serialization error: {0}")]
    Serialization(String),
}

impl From<EvmError> for RuntimeError {
    fn from(err: EvmError) -> Self {
        match err {
            EvmError::Execution(_) => RuntimeError::ExecutionFailed(err.to_string()),
            EvmError::InvalidTransaction(msg) => RuntimeError::InvalidTransaction(msg),
            EvmError::InsufficientGas { .. } => RuntimeError::OutOfGas,
            EvmError::Database(msg) => RuntimeError::ValidationFailed(msg),
            _ => RuntimeError::ExecutionFailed(err.to_string()),
        }
    }
}

/// EVM runtime configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvmConfig {
    /// Chain ID for EVM transactions
    pub chain_id: u64,
    /// Gas limit per block
    pub block_gas_limit: u64,
    /// Base fee per gas
    pub base_fee: U256,
    /// Enable EIP-1559 fee market
    pub enable_eip1559: bool,
    /// Enable London hard fork features
    pub enable_london: bool,
    /// Enable Berlin hard fork features
    pub enable_berlin: bool,
    /// Maximum code size
    pub max_code_size: usize,
    /// Gas limit for view calls
    pub view_gas_limit: u64,
}

impl Default for EvmConfig {
    fn default() -> Self {
        Self {
            chain_id: 31337, // Default local chain ID
            block_gas_limit: 30_000_000,
            base_fee: U256::from(1_000_000_000u64), // 1 gwei
            enable_eip1559: true,
            enable_london: true,
            enable_berlin: true,
            max_code_size: 49152, // 48KB
            view_gas_limit: 50_000_000,
        }
    }
}

/// EVM runtime implementation
pub struct EvmRuntime {
    config: EvmConfig,
    database: CatalystDatabase,
}

impl EvmRuntime {
    /// Create new EVM runtime
    pub fn new(config: EvmConfig) -> Self {
        Self { 
            config,
            database: CatalystDatabase::new(),
        }
    }

    /// Create with genesis accounts
    pub fn with_genesis(config: EvmConfig, accounts: Vec<(Address, U256)>) -> Self {
        Self {
            config,
            database: CatalystDatabase::new().with_genesis_accounts(accounts),
        }
    }

    /// Execute a transaction
    pub async fn execute_transaction(
        &mut self,
        tx: &EvmTransaction,
        block_env: &BlockEnv,
    ) -> Result<EvmExecutionResult, EvmError> {
        let mut evm = EVM::builder()
            .with_db(&mut self.database)
            .with_env(Box::new(Env {
                block: block_env.clone(),
                tx: tx.to_tx_env()?,
                cfg: self.config.to_cfg_env(),
            }))
            .build();

        let result = evm.transact().map_err(|e| {
            tracing::error!("EVM execution failed: {:?}", e);
            EvmError::Database(format!("EVM execution failed: {:?}", e))
        })?;

        Ok(EvmExecutionResult::from_revm_result(result, tx))
    }

    /// Call a contract (read-only)
    pub async fn call(
        &mut self,
        call: &EvmCall,
        block_env: &BlockEnv,
    ) -> Result<EvmCallResult, EvmError> {
        let tx_env = TxEnv {
            caller: call.from,
            gas_limit: call.gas_limit.unwrap_or(self.config.view_gas_limit),
            gas_price: U256::zero(),
            transact_to: TransactTo::Call(call.to),
            value: call.value.unwrap_or(U256::zero()),
            data: call.data.clone().unwrap_or_default().into(),
            gas_priority_fee: None,
            chain_id: Some(self.config.chain_id),
            nonce: None,
            access_list: Vec::new(),
        };

        let mut evm = EVM::builder()
            .with_db(&mut self.database)
            .with_env(Box::new(Env {
                block: block_env.clone(),
                tx: tx_env,
                cfg: self.config.to_cfg_env(),
            }))
            .build();

        let result = evm.transact().map_err(|e| {
            tracing::error!("EVM call failed: {:?}", e);
            EvmError::Database(format!("EVM call failed: {:?}", e))
        })?;

        Ok(EvmCallResult::from_revm_result(result))
    }

    /// Deploy a contract
    pub async fn deploy_contract(
        &mut self,
        deployment: &ContractDeployment,
        block_env: &BlockEnv,
    ) -> Result<ContractDeploymentResult, EvmError> {
        let tx_env = TxEnv {
            caller: deployment.from,
            gas_limit: deployment.gas_limit,
            gas_price: deployment.gas_price,
            transact_to: TransactTo::Create,
            value: deployment.value.unwrap_or(U256::zero()),
            data: deployment.bytecode.clone().into(),
            gas_priority_fee: deployment.gas_priority_fee,
            chain_id: Some(self.config.chain_id),
            nonce: Some(deployment.nonce),
            access_list: deployment.access_list.clone().unwrap_or_default(),
        };

        let mut evm = EVM::builder()
            .with_db(&mut self.database)
            .with_env(Box::new(Env {
                block: block_env.clone(),
                tx: tx_env,
                cfg: self.config.to_cfg_env(),
            }))
            .build();

        let result = evm.transact().map_err(|e| {
            tracing::error!("Contract deployment failed: {:?}", e);
            EvmError::Database(format!("Contract deployment failed: {:?}", e))
        })?;

        Ok(ContractDeploymentResult::from_revm_result(result))
    }

    /// Get database reference
    pub fn database(&self) -> &CatalystDatabase {
        &self.database
    }

    /// Get mutable database reference
    pub fn database_mut(&mut self) -> &mut CatalystDatabase {
        &mut self.database
    }
}

// Implement the Catalyst runtime trait
#[async_trait]
impl catalyst_core::CatalystRuntime for EvmRuntime {
    type Program = Vec<u8>;
    type Account = Vec<u8>;
    type Transaction = Transaction;
    type ExecutionContext = ExecutionContext;

    async fn execute(
        &self,
        program: &Self::Program,
        context: &Self::ExecutionContext,
        transaction: &Self::Transaction,
    ) -> Result<ExecutionResult, RuntimeError> {
        // Parse the program as an EVM transaction
        let tx: EvmTransaction = serde_json::from_slice(program)
            .map_err(|e| RuntimeError::InvalidProgram(format!("Failed to parse EVM transaction: {}", e)))?;

        let block_env = BlockEnv {
            number: U256::from(context.block_number),
            coinbase: Address::zero(), // TODO: Set proper coinbase
            timestamp: U256::from(context.block_timestamp),
            gas_limit: U256::from(self.config.block_gas_limit),
            basefee: self.config.base_fee,
            difficulty: U256::zero(), // PoS doesn't use difficulty
            prevrandao: Some(context.block_hash.into()),
            // Remove blob_excess_gas_and_price for older revm version
        };

        // Clone self to get mutable access (this is a limitation of the trait design)
        let mut runtime = EvmRuntime {
            config: self.config.clone(),
            database: self.database.clone(),
        };

        let result = runtime.execute_transaction(&tx, &block_env).await
            .map_err(|e| e.into())?;

        Ok(ExecutionResult {
            success: result.success,
            return_data: result.return_data,
            gas_used: result.gas_used,
            state_changes: result.state_changes.into_iter().map(|sc| {
                catalyst_core::StateChange {
                    address: sc.address.0,
                    key: vec![], // TODO: Map EVM state changes properly
                    old_value: None,
                    new_value: None,
                }
            }).collect(),
            events: result.logs.into_iter().map(|log| log.into()).collect(),
            error: None,
        })
    }

    async fn validate_program(&self, program: &Self::Program) -> Result<(), RuntimeError> {
        // Validate that the program is a valid EVM transaction
        serde_json::from_slice::<EvmTransaction>(program)
            .map_err(|e| RuntimeError::InvalidProgram(format!("Invalid EVM transaction: {}", e)))?;
        Ok(())
    }

    async fn estimate_resources(
        &self,
        program: &Self::Program,
        _context: &Self::ExecutionContext,
        _transaction: &Self::Transaction,
    ) -> Result<ResourceEstimate, RuntimeError> {
        let tx: EvmTransaction = serde_json::from_slice(program)
            .map_err(|e| RuntimeError::InvalidProgram(format!("Failed to parse EVM transaction: {}", e)))?;

        // Estimate based on transaction properties
        let compute_units = tx.gas_limit;
        let memory_bytes = tx.data.len() as u64 * 32; // Rough estimate
        let storage_bytes = if tx.to.is_none() { tx.data.len() as u64 } else { 0 }; // Contract creation
        let network_bytes = tx.data.len() as u64 + 200; // Transaction overhead

        Ok(ResourceEstimate::new(
            compute_units,
            memory_bytes,
            storage_bytes,
            network_bytes,
        ))
    }

    fn runtime_type(&self) -> catalyst_core::RuntimeType {
        catalyst_core::RuntimeType::Native // EVM runs as native runtime
    }
}

impl EvmConfig {
    /// Convert to REVM configuration environment
    fn to_cfg_env(&self) -> CfgEnv {
        let mut cfg = CfgEnv::default();
        cfg.chain_id = self.chain_id;
        
        // Enable hard fork features
        if self.enable_london {
            cfg.spec_id = SpecId::LONDON;
        } else if self.enable_berlin {
            cfg.spec_id = SpecId::BERLIN;
        }
        
        // Set limits
        cfg.limit_contract_code_size = Some(self.max_code_size);
        
        cfg
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_evm_config_default() {
        let config = EvmConfig::default();
        assert_eq!(config.chain_id, 31337);
        assert!(config.enable_london);
        assert!(config.enable_berlin);
        assert_eq!(config.max_code_size, 49152);
    }

    #[test]
    fn test_cfg_env_conversion() {
        let config = EvmConfig::default();
        let cfg_env = config.to_cfg_env();
        assert_eq!(cfg_env.chain_id, config.chain_id);
    }

    #[tokio::test]
    async fn test_evm_runtime_creation() {
        let config = EvmConfig::default();
        let runtime = EvmRuntime::new(config);
        assert_eq!(runtime.config.chain_id, 31337);
    }

    #[tokio::test]
    async fn test_evm_runtime_with_genesis() {
        let config = EvmConfig::default();
        let genesis_accounts = vec![
            (Address::from_low_u64_be(1), U256::from(1000)),
            (Address::from_low_u64_be(2), U256::from(2000)),
        ];
        
        let runtime = EvmRuntime::with_genesis(config, genesis_accounts);
        assert_eq!(runtime.config.chain_id, 31337);
    }

    #[tokio::test]
    async fn test_validate_program() {
        let runtime = EvmRuntime::new(EvmConfig::default());
        
        let valid_tx = EvmTransaction {
            from: Address::from_low_u64_be(1),
            to: Some(Address::from_low_u64_be(2)),
            value: U256::from(100),
            data: vec![],
            gas_limit: 21000,
            gas_price: U256::from(1_000_000_000u64),
            gas_priority_fee: None,
            nonce: 0,
            chain_id: 31337,
            access_list: None,
        };
        
        let program = serde_json::to_vec(&valid_tx).unwrap();
        assert!(runtime.validate_program(&program).await.is_ok());
        
        // Test invalid program
        let invalid_program = b"invalid json".to_vec();
        assert!(runtime.validate_program(&invalid_program).await.is_err());
    }

    #[tokio::test]
    async fn test_estimate_resources() {
        let runtime = EvmRuntime::new(EvmConfig::default());
        let context = ExecutionContext::new(1, [0u8; 20]);
        let transaction = catalyst_core::Transaction::new([0u8; 20], None, 100, vec![]);
        
        let tx = EvmTransaction {
            from: Address::from_low_u64_be(1),
            to: Some(Address::from_low_u64_be(2)),
            value: U256::from(100),
            data: vec![1, 2, 3, 4], // 4 bytes of data
            gas_limit: 50000,
            gas_price: U256::from(1_000_000_000u64),
            gas_priority_fee: None,
            nonce: 0,
            chain_id: 31337,
            access_list: None,
        };
        
        let program = serde_json::to_vec(&tx).unwrap();
        let estimate = runtime.estimate_resources(&program, &context, &transaction).await.unwrap();
        
        assert_eq!(estimate.compute_units, 50000);
        assert_eq!(estimate.memory_bytes, 4 * 32); // 4 bytes * 32
        assert_eq!(estimate.storage_bytes, 0); // Not a contract creation
        assert_eq!(estimate.network_bytes, 204); // 4 + 200 overhead
    }
}