use crate::{SvmConfig, runtime::Runtime};
use catalyst_core::{ExecutionResult, ExecutionContext, RuntimeError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use wasmtime::{Engine, Module, Store, Linker};

/// Native BPF runtime using rbpf
pub struct NativeRuntime {
    config: SvmConfig,
    bpf_config: BpfConfig,
    wasm_engine: Engine,
}

impl std::fmt::Debug for NativeRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeRuntime")
            .field("config", &self.config)
            .field("bpf_config", &self.bpf_config)
            .field("wasm_engine", &"<Engine>")
            .finish()
    }
}

#[derive(Debug, Clone)]
struct BpfConfig {
    max_instruction_count: u64,
    max_stack_size: usize,
    max_heap_size: usize,
}

impl NativeRuntime {
    pub fn new(config: SvmConfig) -> Self {
        let bpf_config = BpfConfig {
            max_instruction_count: config.max_instruction_count,
            max_stack_size: config.max_stack_size,
            max_heap_size: config.max_heap_size,
        };

        let wasm_engine = Engine::default();

        Self {
            config,
            bpf_config,
            wasm_engine,
        }
    }

    pub async fn execute(
        &self,
        program: &[u8],
        input: &[u8],
        context: ExecutionContext,
    ) -> Result<ExecutionResult, RuntimeError> {
        // For Phase 1, provide a simple implementation
        // In Phase 2, we can add full BPF execution using rbpf 0.3+ API
        
        if program.is_empty() {
            return Err(RuntimeError::InvalidProgram("Empty program".to_string()));
        }

        // Simple execution simulation
        let gas_used = std::cmp::min(program.len() as u64 * 10, self.bpf_config.max_instruction_count);
        
        // Create a simple response based on input
        let mut return_data = Vec::new();
        if !input.is_empty() {
            return_data.push(input[0].wrapping_add(1)); // Simple transformation
        }

        tracing::debug!(
            "Executed BPF program: {} bytes, {} gas used, context block {}",
            program.len(),
            gas_used,
            context.block_number
        );

        Ok(ExecutionResult::success(return_data, gas_used))
    }

    pub async fn validate(&self, program: &[u8]) -> Result<(), RuntimeError> {
        if program.is_empty() {
            return Err(RuntimeError::InvalidProgram("Empty program".to_string()));
        }

        // Basic validation - check program size
        if program.len() > 1024 * 1024 {
            return Err(RuntimeError::InvalidProgram("Program too large".to_string()));
        }

        // Check for basic BPF magic bytes (simplified)
        if program.len() >= 4 {
            let magic = u32::from_le_bytes([program[0], program[1], program[2], program[3]]);
            if magic == 0x464c457f {
                // ELF magic - good
                tracing::debug!("Valid ELF BPF program detected");
            } else {
                tracing::warn!("Program does not appear to be ELF format");
            }
        }

        Ok(())
    }

    /// Execute WASM program as fallback
    pub async fn execute_wasm(
        &self,
        wasm_bytes: &[u8],
        _input: &[u8],
        _context: ExecutionContext,
    ) -> Result<ExecutionResult, RuntimeError> {
        // Simple WASM execution using wasmtime
        let module = Module::new(&self.wasm_engine, wasm_bytes)
            .map_err(|e| RuntimeError::InvalidProgram(format!("Invalid WASM: {}", e)))?;

        let mut store = Store::new(&self.wasm_engine, ());
        let linker = Linker::new(&self.wasm_engine);

        // For now, just validate that we can instantiate the module
        linker.instantiate(&mut store, &module)
            .map_err(|e| RuntimeError::ExecutionFailed(format!("WASM instantiation failed: {}", e)))?;

        tracing::debug!("WASM program validated successfully");

        Ok(ExecutionResult::success(vec![], 1000))
    }

    /// Get runtime statistics
    pub fn get_stats(&self) -> RuntimeStats {
        RuntimeStats {
            max_instruction_count: self.bpf_config.max_instruction_count,
            max_stack_size: self.bpf_config.max_stack_size,
            max_heap_size: self.bpf_config.max_heap_size,
            programs_executed: 0, // TODO: Add tracking
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeStats {
    pub max_instruction_count: u64,
    pub max_stack_size: usize,
    pub max_heap_size: usize,
    pub programs_executed: u64,
}

/// BPF program loader
pub struct BpfLoader {
    programs: HashMap<String, Vec<u8>>,
}

impl BpfLoader {
    pub fn new() -> Self {
        Self {
            programs: HashMap::new(),
        }
    }

    pub fn load_program(&mut self, name: String, bytecode: Vec<u8>) -> Result<(), RuntimeError> {
        if bytecode.is_empty() {
            return Err(RuntimeError::InvalidProgram("Empty bytecode".to_string()));
        }

        self.programs.insert(name, bytecode);
        Ok(())
    }

    pub fn get_program(&self, name: &str) -> Option<&Vec<u8>> {
        self.programs.get(name)
    }

    pub fn list_programs(&self) -> Vec<&String> {
        self.programs.keys().collect()
    }
}

impl Default for BpfLoader {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_native_runtime() {
        let config = SvmConfig::default();
        let runtime = NativeRuntime::new(config);
        
        let program = vec![0x7f, 0x45, 0x4c, 0x46]; // ELF magic
        let input = vec![42];
        let context = ExecutionContext::new(1, [0u8; 20]);

        let result = runtime.execute(&program, &input, context).await.unwrap();
        assert!(result.success);
        assert!(!result.return_data.is_empty());
    }

    #[tokio::test]
    async fn test_program_validation() {
        let config = SvmConfig::default();
        let runtime = NativeRuntime::new(config);

        // Test empty program
        assert!(runtime.validate(&[]).await.is_err());

        // Test valid program
        let program = vec![0x7f, 0x45, 0x4c, 0x46, 1, 2, 3, 4];
        assert!(runtime.validate(&program).await.is_ok());
    }

    #[test]
    fn test_bpf_loader() {
        let mut loader = BpfLoader::new();
        
        let program = vec![1, 2, 3, 4];
        assert!(loader.load_program("test".to_string(), program.clone()).is_ok());
        
        let loaded = loader.get_program("test").unwrap();
        assert_eq!(loaded, &program);
        
        let programs = loader.list_programs();
        assert_eq!(programs.len(), 1);
        assert_eq!(programs[0], "test");
    }

    #[test]
    fn test_runtime_stats() {
        let config = SvmConfig::default();
        let runtime = NativeRuntime::new(config.clone());
        
        let stats = runtime.get_stats();
        assert_eq!(stats.max_instruction_count, config.max_instruction_count);
        assert_eq!(stats.max_stack_size, config.max_stack_size);
        assert_eq!(stats.max_heap_size, config.max_heap_size);
    }
}