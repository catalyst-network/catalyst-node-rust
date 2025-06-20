use std::collections::HashMap;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use crate::{Transaction, ExecutionResult, ExecutionContext};

// TODO: Create these modules for Phase 1
// pub mod native;
// pub mod wasm;

// Re-export runtime types
pub use crate::types::{RuntimeType, ResourceEstimate};

/// Configuration for runtime execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    pub max_gas: u64,
    pub max_memory: u64,
    pub timeout_ms: u64,
    pub enable_debug: bool,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            max_gas: 1_000_000,
            max_memory: 64 * 1024 * 1024, // 64MB
            timeout_ms: 30_000, // 30 seconds
            enable_debug: false,
        }
    }
}

/// Runtime execution environment interface
/// 
/// This trait defines the interface for different runtime environments
/// (native, WASM, SVM, etc.) that can execute programs in Catalyst.
#[async_trait]
pub trait CatalystRuntime: Send + Sync {
    type Program;
    type Account;
    type Transaction;
    type ExecutionContext;

    /// Execute a program with given context and transaction
    async fn execute(
        &self,
        program: &Self::Program,
        context: &Self::ExecutionContext,
        transaction: &Self::Transaction,
    ) -> Result<ExecutionResult, RuntimeError>;

    /// Validate a program without executing it
    async fn validate_program(&self, program: &Self::Program) -> Result<(), RuntimeError>;

    /// Estimate resources needed for execution
    async fn estimate_resources(
        &self,
        program: &Self::Program,
        context: &Self::ExecutionContext,
        transaction: &Self::Transaction,
    ) -> Result<ResourceEstimate, RuntimeError>;

    /// Get runtime type identifier
    fn runtime_type(&self) -> RuntimeType;
}

/// Runtime execution errors
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuntimeError {
    InvalidProgram(String),
    ExecutionFailed(String),
    OutOfGas,
    OutOfMemory,
    Timeout,
    InvalidTransaction(String),
    ValidationFailed(String),
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuntimeError::InvalidProgram(msg) => write!(f, "Invalid program: {}", msg),
            RuntimeError::ExecutionFailed(msg) => write!(f, "Execution failed: {}", msg),
            RuntimeError::OutOfGas => write!(f, "Out of gas"),
            RuntimeError::OutOfMemory => write!(f, "Out of memory"),
            RuntimeError::Timeout => write!(f, "Execution timeout"),
            RuntimeError::InvalidTransaction(msg) => write!(f, "Invalid transaction: {}", msg),
            RuntimeError::ValidationFailed(msg) => write!(f, "Validation failed: {}", msg),
        }
    }
}

impl std::error::Error for RuntimeError {}

/// Runtime manager that handles multiple runtime types
pub struct RuntimeManager {
    runtimes: HashMap<RuntimeType, Box<dyn CatalystRuntime<
        Program = Vec<u8>,
        Account = Vec<u8>, 
        Transaction = Transaction,
        ExecutionContext = ExecutionContext
    >>>,
    config: RuntimeConfig,
}

impl RuntimeManager {
    pub fn new(config: RuntimeConfig) -> Self {
        Self {
            runtimes: HashMap::new(),
            config,
        }
    }

    pub fn register_runtime(
        &mut self,
        runtime_type: RuntimeType,
        runtime: Box<dyn CatalystRuntime<
            Program = Vec<u8>,
            Account = Vec<u8>,
            Transaction = Transaction,
            ExecutionContext = ExecutionContext
        >>,
    ) {
        self.runtimes.insert(runtime_type, runtime);
    }

    pub async fn execute(
        &self,
        runtime_type: &RuntimeType,
        program: &Vec<u8>,
        context: &ExecutionContext,
        transaction: &Transaction,
    ) -> Result<ExecutionResult, RuntimeError> {
        match self.runtimes.get(runtime_type) {
            Some(runtime) => runtime.execute(program, context, transaction).await,
            None => Err(RuntimeError::InvalidProgram(
                format!("Runtime type {:?} not registered", runtime_type)
            )),
        }
    }

    pub async fn validate_program(
        &self,
        runtime_type: &RuntimeType,
        program: &Vec<u8>,
    ) -> Result<(), RuntimeError> {
        match self.runtimes.get(runtime_type) {
            Some(runtime) => runtime.validate_program(program).await,
            None => Err(RuntimeError::InvalidProgram(
                format!("Runtime type {:?} not registered", runtime_type)
            )),
        }
    }

    pub async fn estimate_resources(
        &self,
        runtime_type: &RuntimeType,
        program: &Vec<u8>,
        context: &ExecutionContext,
        transaction: &Transaction,
    ) -> Result<ResourceEstimate, RuntimeError> {
        match self.runtimes.get(runtime_type) {
            Some(runtime) => runtime.estimate_resources(program, context, transaction).await,
            None => Err(RuntimeError::InvalidProgram(
                format!("Runtime type {:?} not registered", runtime_type)
            )),
        }
    }

    pub fn get_config(&self) -> &RuntimeConfig {
        &self.config
    }

    pub fn list_runtimes(&self) -> Vec<RuntimeType> {
        self.runtimes.keys().cloned().collect()
    }
}

impl Default for RuntimeManager {
    fn default() -> Self {
        Self::new(RuntimeConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Address, ExecutionContext};

    #[tokio::test]
    async fn test_runtime_manager() {
        let mut manager = RuntimeManager::default();
        
        // Test with no runtimes registered
        let program = vec![1, 2, 3, 4];
        let context = ExecutionContext::new(1, [0u8; 20]);
        let transaction = Transaction::new([0u8; 20], None, 100, vec![]);
        
        let result = manager.execute(&RuntimeType::Native, &program, &context, &transaction).await;
        assert!(result.is_err());
    }
}