//! Solana Virtual Machine runtime for Catalyst Network
//! 
//! Provides BPF execution capability for smart contracts using rbpf.

use async_trait::async_trait;
use catalyst_core::{
    Transaction, ExecutionResult, ExecutionContext, RuntimeError, ResourceEstimate,
    runtime::CatalystRuntime, RuntimeType
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

// Phase 2: Solana compatibility - commented out for Phase 1
// #[cfg(feature = "solana-compat")]
// pub mod solana;

pub mod native;
pub mod runtime;

/// SVM runtime configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SvmConfig {
    /// Maximum instruction count per execution
    pub max_instruction_count: u64,
    /// Maximum stack size
    pub max_stack_size: usize,
    /// Maximum heap size  
    pub max_heap_size: usize,
    /// Enable debug output
    pub enable_debug: bool,
    /// BPF features to enable
    pub bpf_features: BpfFeatures,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BpfFeatures {
    pub enable_stack_frame_gaps: bool,
    pub enable_symbol_and_section_labels: bool,
    pub enable_elf_vaddr: bool,
}

impl Default for SvmConfig {
    fn default() -> Self {
        Self {
            max_instruction_count: 200_000,
            max_stack_size: 4096,
            max_heap_size: 32 * 1024,
            enable_debug: false,
            bpf_features: BpfFeatures::default(),
        }
    }
}

impl Default for BpfFeatures {
    fn default() -> Self {
        Self {
            enable_stack_frame_gaps: true,
            enable_symbol_and_section_labels: true,
            enable_elf_vaddr: true,
        }
    }
}

/// Main SVM runtime manager
pub struct SvmRuntime {
    config: SvmConfig,
    runtime_manager: Arc<RwLock<RuntimeManager>>,
}

impl SvmRuntime {
    pub fn new(config: SvmConfig) -> Self {
        Self {
            config,
            runtime_manager: Arc::new(RwLock::new(RuntimeManager::new())),
        }
    }

    pub async fn execute_program(
        &self,
        program: &[u8],
        input: &[u8],
        context: ExecutionContext,
    ) -> Result<ExecutionResult, RuntimeError> {
        let manager = self.runtime_manager.read().await;
        manager.execute_program(program, input, context).await
    }

    pub async fn validate_program(&self, program: &[u8]) -> Result<(), RuntimeError> {
        let manager = self.runtime_manager.read().await;
        manager.validate_program(program).await
    }

    pub fn supports_runtime(&self, runtime_type: &RuntimeType) -> bool {
        match runtime_type {
            RuntimeType::Native => true,
            RuntimeType::SVM => true,
            _ => false,
        }
    }
}

#[async_trait]
impl CatalystRuntime for SvmRuntime {
    type Program = Vec<u8>;
    type Account = Vec<u8>;
    type Transaction = Transaction;
    type ExecutionContext = ExecutionContext;

    async fn execute(
        &self,
        program: &Self::Program,
        context: &Self::ExecutionContext,
        _transaction: &Self::Transaction,
    ) -> Result<ExecutionResult, RuntimeError> {
        let input = vec![]; // TODO: Extract input from transaction
        self.execute_program(program, &input, context.clone()).await
    }

    async fn validate_program(&self, program: &Self::Program) -> Result<(), RuntimeError> {
        self.validate_program(program).await
    }

    async fn estimate_resources(
        &self,
        program: &Self::Program,
        _context: &Self::ExecutionContext,
        _transaction: &Self::Transaction,
    ) -> Result<ResourceEstimate, RuntimeError> {
        // Basic estimation based on program size
        let compute_units = (program.len() as u64).min(self.config.max_instruction_count);
        let memory_bytes = self.config.max_heap_size as u64 + self.config.max_stack_size as u64;
        let storage_bytes = program.len() as u64;
        let network_bytes = program.len() as u64 + 100; // Program + overhead

        Ok(ResourceEstimate::new(
            compute_units,
            memory_bytes,
            storage_bytes,
            network_bytes,
        ))
    }

    fn runtime_type(&self) -> RuntimeType {
        RuntimeType::SVM
    }
}

/// Internal runtime manager
pub struct RuntimeManager {
    native_runtime: native::NativeRuntime,
}

impl RuntimeManager {
    pub fn new() -> Self {
        Self {
            native_runtime: native::NativeRuntime::new(SvmConfig::default()),
        }
    }

    pub async fn execute_program(
        &self,
        program: &[u8],
        input: &[u8],
        context: ExecutionContext,
    ) -> Result<ExecutionResult, RuntimeError> {
        // For Phase 1, use native BPF execution
        self.native_runtime.execute(program, input, context).await
    }

    pub async fn validate_program(&self, program: &[u8]) -> Result<(), RuntimeError> {
        self.native_runtime.validate(program).await
    }
}

impl Default for RuntimeManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_svm_runtime_creation() {
        let config = SvmConfig::default();
        let runtime = SvmRuntime::new(config);
        
        assert!(runtime.supports_runtime(&RuntimeType::SVM));
        assert!(runtime.supports_runtime(&RuntimeType::Native));
        assert!(!runtime.supports_runtime(&RuntimeType::WASM));
    }

    #[tokio::test]
    async fn test_resource_estimation() {
        let runtime = SvmRuntime::new(SvmConfig::default());
        let context = ExecutionContext::new(1, [0u8; 20]);
        let transaction = Transaction::new([0u8; 20], None, 100, vec![]);
        let program = vec![1, 2, 3, 4]; // Simple test program

        let estimate = runtime.estimate_resources(&program, &context, &transaction).await.unwrap();
        
        assert_eq!(estimate.compute_units, 4); // program.len()
        assert_eq!(estimate.storage_bytes, 4); // program.len()
        assert_eq!(estimate.network_bytes, 104); // program.len() + 100
    }

    #[test]
    fn test_config_defaults() {
        let config = SvmConfig::default();
        assert_eq!(config.max_instruction_count, 200_000);
        assert_eq!(config.max_stack_size, 4096);
        assert_eq!(config.max_heap_size, 32 * 1024);
        assert!(!config.enable_debug);
    }
}