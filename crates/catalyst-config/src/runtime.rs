//! Runtime configuration structures

use serde::{Deserialize, Serialize};
use crate::ConfigError;

/// Runtime configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    /// Enabled runtime modules
    pub enabled_runtimes: Vec<RuntimeType>,
    /// EVM configuration
    pub evm: EvmConfig,
    /// SVM configuration
    pub svm: SvmConfig,
    /// Native WASM configuration
    pub wasm: WasmConfig,
    /// Resource limits
    pub limits: ResourceLimits,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            enabled_runtimes: vec![RuntimeType::Evm, RuntimeType::Native],
            evm: EvmConfig::default(),
            svm: SvmConfig::default(),
            wasm: WasmConfig::default(),
            limits: ResourceLimits::default(),
        }
    }
}

/// Available runtime types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RuntimeType {
    /// Ethereum Virtual Machine
    Evm,
    /// Solana Virtual Machine
    Svm,
    /// Native WebAssembly
    Native,
    /// Custom runtime
    Custom(String),
}

/// EVM runtime configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvmConfig {
    /// Chain ID
    pub chain_id: u64,
    /// Block gas limit
    pub block_gas_limit: u64,
    /// Base fee per gas
    pub base_fee_per_gas: u64,
    /// Enable EIP-1559
    pub enable_eip1559: bool,
    /// Enable precompiles
    pub enable_precompiles: bool,
    /// Maximum code size
    pub max_code_size: usize,
}

impl Default for EvmConfig {
    fn default() -> Self {
        Self {
            chain_id: 31337,
            block_gas_limit: 30_000_000,
            base_fee_per_gas: 1_000_000_000, // 1 gwei
            enable_eip1559: true,
            enable_precompiles: true,
            max_code_size: 49152, // 48KB
        }
    }
}

/// SVM runtime configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SvmConfig {
    /// Maximum compute units per transaction
    pub max_compute_units: u64,
    /// Compute unit price
    pub compute_unit_price: u64,
    /// Maximum accounts per transaction
    pub max_accounts_per_tx: usize,
    /// Enable native programs
    pub enable_native_programs: bool,
}

impl Default for SvmConfig {
    fn default() -> Self {
        Self {
            max_compute_units: 1_400_000,
            compute_unit_price: 1,
            max_accounts_per_tx: 64,
            enable_native_programs: true,
        }
    }
}

/// WebAssembly runtime configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmConfig {
    /// Maximum memory pages (64KB per page)
    pub max_memory_pages: u32,
    /// Maximum execution time in milliseconds
    pub max_execution_time_ms: u64,
    /// Enable SIMD instructions
    pub enable_simd: bool,
    /// Enable bulk memory operations
    pub enable_bulk_memory: bool,
}

impl Default for WasmConfig {
    fn default() -> Self {
        Self {
            max_memory_pages: 1024, // 64MB max memory
            max_execution_time_ms: 10000, // 10 seconds
            enable_simd: true,
            enable_bulk_memory: true,
        }
    }
}

/// Resource limits for runtime execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// Maximum memory usage in MB
    pub max_memory_mb: usize,
    /// Maximum execution time in seconds
    pub max_execution_time_secs: u64,
    /// Maximum stack depth
    pub max_stack_depth: usize,
    /// Maximum concurrent executions
    pub max_concurrent_executions: usize,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_memory_mb: 512,
            max_execution_time_secs: 30,
            max_stack_depth: 1024,
            max_concurrent_executions: 100,
        }
    }
}

impl RuntimeConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.enabled_runtimes.is_empty() {
            return Err(ConfigError::Invalid(
                "At least one runtime must be enabled".to_string()
            ));
        }

        for runtime_type in &self.enabled_runtimes {
            match runtime_type {
                RuntimeType::Evm => self.evm.validate()?,
                RuntimeType::Svm => self.svm.validate()?,
                RuntimeType::Native => self.wasm.validate()?,
                RuntimeType::Custom(_) => {
                    // Custom runtime validation would be handled by the runtime itself
                }
            }
        }

        self.limits.validate()?;

        Ok(())
    }
}

impl EvmConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        if self.chain_id == 0 {
            return Err(ConfigError::Invalid(
                "EVM chain ID cannot be zero".to_string()
            ));
        }

        if self.block_gas_limit == 0 {
            return Err(ConfigError::Invalid(
                "EVM block gas limit cannot be zero".to_string()
            ));
        }

        if self.max_code_size == 0 {
            return Err(ConfigError::Invalid(
                "EVM max code size cannot be zero".to_string()
            ));
        }

        Ok(())
    }
}

impl SvmConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        if self.max_compute_units == 0 {
            return Err(ConfigError::Invalid(
                "SVM max compute units cannot be zero".to_string()
            ));
        }

        if self.max_accounts_per_tx == 0 {
            return Err(ConfigError::Invalid(
                "SVM max accounts per transaction cannot be zero".to_string()
            ));
        }

        Ok(())
    }
}

impl WasmConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        if self.max_memory_pages == 0 {
            return Err(ConfigError::Invalid(
                "WASM max memory pages cannot be zero".to_string()
            ));
        }

        if self.max_execution_time_ms == 0 {
            return Err(ConfigError::Invalid(
                "WASM max execution time cannot be zero".to_string()
            ));
        }

        Ok(())
    }
}

impl ResourceLimits {
    fn validate(&self) -> Result<(), ConfigError> {
        if self.max_memory_mb == 0 {
            return Err(ConfigError::Invalid(
                "Max memory cannot be zero".to_string()
            ));
        }

        if self.max_execution_time_secs == 0 {
            return Err(ConfigError::Invalid(
                "Max execution time cannot be zero".to_string()
            ));
        }

        if self.max_stack_depth == 0 {
            return Err(ConfigError::Invalid(
                "Max stack depth cannot be zero".to_string()
            ));
        }

        if self.max_concurrent_executions == 0 {
            return Err(ConfigError::Invalid(
                "Max concurrent executions cannot be zero".to_string()
            ));
        }

        Ok(())
    }
}