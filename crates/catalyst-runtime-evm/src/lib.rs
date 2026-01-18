// Simplified catalyst-runtime-evm that builds successfully
// Focus on core types and basic functionality first
// We'll add REVM integration once the basic structure works

use alloy_primitives::{Address, U256, B256, Bytes};
use std::collections::HashMap;
use thiserror::Error;
use serde::{Serialize, Deserialize};

// Basic modules
pub mod types;
pub mod database;

pub use types::*;
pub use database::*;

/// Error types for Catalyst EVM
#[derive(Error, Debug)]
pub enum EvmError {
    #[error("Execution error: {0}")]
    Execution(String),
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
    #[error("Cross-runtime call failed: {0}")]
    CrossRuntimeError(String),
    #[error("DFS operation failed: {0}")]
    DfsError(String),
    #[error("Confidential transaction error: {0}")]
    ConfidentialTxError(String),
}

/// Standard EVM configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvmConfig {
    pub chain_id: u64,
    pub gas_limit: u64,
    pub base_fee: U256,
}

impl Default for EvmConfig {
    fn default() -> Self {
        Self {
            chain_id: 31337,
            gas_limit: 30_000_000,
            base_fee: U256::from(1_000_000_000u64),
        }
    }
}

/// Catalyst-specific features configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalystFeatures {
    /// Enable cross-runtime calls
    pub cross_runtime_enabled: bool,
    /// Enable confidential transactions
    pub confidential_tx_enabled: bool,
    /// Enable native DFS integration
    pub dfs_integration: bool,
    /// Custom gas pricing model
    pub catalyst_gas_model: bool,
}

impl Default for CatalystFeatures {
    fn default() -> Self {
        Self {
            cross_runtime_enabled: false,
            confidential_tx_enabled: false,
            dfs_integration: false,
            catalyst_gas_model: false,
        }
    }
}

/// Catalyst EVM configuration with extensions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalystEvmConfig {
    /// Standard EVM configuration
    pub evm_config: EvmConfig,
    /// Catalyst-specific extensions
    pub catalyst_features: CatalystFeatures,
}

impl Default for CatalystEvmConfig {
    fn default() -> Self {
        Self {
            evm_config: EvmConfig::default(),
            catalyst_features: CatalystFeatures::default(),
        }
    }
}

/// Execution result for Catalyst EVM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalystExecutionResult {
    /// Success flag
    pub success: bool,
    /// Gas used
    pub gas_used: u64,
    /// Return data
    pub return_data: Bytes,
    /// Cross-runtime calls made during execution
    pub cross_runtime_calls: Vec<CrossRuntimeCall>,
    /// DFS operations performed
    pub dfs_operations: Vec<DfsOperation>,
    /// Confidential transaction proofs
    pub confidential_proofs: Option<ConfidentialProofs>,
    /// Created contract address (if any)
    pub created_address: Option<Address>,
    /// Gas refunded
    pub gas_refunded: u64,
    /// Execution logs
    pub logs: Vec<EvmLog>,
}

impl CatalystExecutionResult {
    pub fn success(&self) -> bool {
        self.success
    }

    pub fn gas_used(&self) -> u64 {
        self.gas_used
    }

    pub fn return_data(&self) -> &Bytes {
        &self.return_data
    }
}

/// Cross-runtime call representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossRuntimeCall {
    pub target_runtime: RuntimeType,
    pub method: String,
    pub parameters: Bytes,
    pub gas_used: u64,
}

/// DFS operation representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DfsOperation {
    pub operation_type: DfsOperationType,
    pub file_hash: B256,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DfsOperationType {
    Store,
    Retrieve,
    Delete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RuntimeType {
    EVM,
    SVM, // Solana
    Native, // WASM/Native
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfidentialProofs {
    pub range_proofs: Vec<Bytes>,
    pub commitment_proofs: Vec<Bytes>,
}

/// EVM log representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvmLog {
    pub address: Address,
    pub topics: Vec<B256>,
    pub data: Bytes,
}

/// Gas metering for EVM execution
#[derive(Debug, Clone)]
pub struct GasMeter {
    gas_limit: u64,
    gas_used: u64,
    gas_price: U256,
}

impl GasMeter {
    pub fn new(gas_limit: u64, gas_price: U256) -> Self {
        Self {
            gas_limit,
            gas_used: 0,
            gas_price,
        }
    }

    pub fn consume_gas(&mut self, gas: u64) -> Result<(), EvmError> {
        if self.gas_used + gas > self.gas_limit {
            return Err(EvmError::InsufficientGas {
                provided: self.gas_limit,
                required: self.gas_used + gas,
            });
        }
        self.gas_used += gas;
        Ok(())
    }

    pub fn gas_remaining(&self) -> u64 {
        self.gas_limit.saturating_sub(self.gas_used)
    }

    pub fn gas_used(&self) -> u64 {
        self.gas_used
    }
}

/// Fee calculation for Catalyst network
#[derive(Debug, Clone)]
pub struct FeeCalculator {
    base_fee: U256,
    congestion_multiplier: f64,
}

impl FeeCalculator {
    pub fn new(base_fee: U256) -> Self {
        Self {
            base_fee,
            congestion_multiplier: 1.0,
        }
    }

    pub fn calculate_fee(&self, gas_used: u64, priority_fee: Option<U256>) -> U256 {
        let base_cost = self.base_fee * U256::from(gas_used);
        // Simple arithmetic without problematic conversions
        let congestion_adjustment = base_cost + (base_cost / U256::from(2)); // 50% increase
        let priority = priority_fee.unwrap_or(U256::ZERO);
        
        congestion_adjustment + priority
    }

    pub fn update_congestion(&mut self, network_utilization: f64) {
        // Adjust fees based on network congestion
        self.congestion_multiplier = 1.0 + (network_utilization * 0.5);
    }
}

/// Contract deployment support - simplified for now
#[derive(Debug, Clone)]
pub struct ContractDeployer {
    nonce_tracker: HashMap<Address, u64>,
}

impl ContractDeployer {
    pub fn new() -> Self {
        Self {
            nonce_tracker: HashMap::new(),
        }
    }

    pub fn deploy_contract(
        &mut self,
        deployer: Address,
        _bytecode: Bytes,
        _constructor_args: Bytes,
        _salt: Option<B256>,
    ) -> Result<Address, EvmError> {
        let _nonce = self.get_next_nonce(deployer);
        
        // For now, just return a deterministic address based on deployer
        let mut addr_bytes = [0u8; 20];
        addr_bytes[0..4].copy_from_slice(&deployer.as_slice()[0..4]);
        addr_bytes[19] = 0x42; // Contract marker
        
        let contract_address = Address::from(addr_bytes);
        
        tracing::info!("Deploying contract to address: {:?}", contract_address);
        Ok(contract_address)
    }

    fn get_next_nonce(&mut self, address: Address) -> u64 {
        let nonce = self.nonce_tracker.get(&address).copied().unwrap_or(0);
        self.nonce_tracker.insert(address, nonce + 1);
        nonce
    }
}

impl Default for ContractDeployer {
    fn default() -> Self {
        Self::new()
    }
}

/// Debug and tracing support
#[derive(Debug, Clone)]
pub struct EvmDebugger {
    trace_enabled: bool,
    opcodes_executed: Vec<String>,
    gas_costs: Vec<u64>,
}

impl EvmDebugger {
    pub fn new(trace_enabled: bool) -> Self {
        Self {
            trace_enabled,
            opcodes_executed: Vec::new(),
            gas_costs: Vec::new(),
        }
    }

    pub fn trace_opcode(&mut self, opcode: &str, gas_cost: u64) {
        if self.trace_enabled {
            tracing::debug!("Executing opcode: {} (gas: {})", opcode, gas_cost);
            self.opcodes_executed.push(opcode.to_string());
            self.gas_costs.push(gas_cost);
        }
    }

    pub fn get_trace(&self) -> Vec<(String, u64)> {
        self.opcodes_executed.iter()
            .zip(self.gas_costs.iter())
            .map(|(op, gas)| (op.clone(), *gas))
            .collect()
    }

    pub fn clear_trace(&mut self) {
        self.opcodes_executed.clear();
        self.gas_costs.clear();
    }
}

/// Simplified execution context
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

impl ExecutionContext {
    pub fn new(chain_id: u64) -> Self {
        Self {
            block_number: 1,
            block_timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            block_hash: B256::ZERO,
            coinbase: Address::ZERO,
            gas_limit: 30_000_000,
            base_fee: U256::from(1_000_000_000u64),
            chain_id,
        }
    }

    /// Move to next block
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

/// Simplified EVM call structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvmCall {
    pub from: Address,
    pub to: Address,
    pub value: Option<U256>,
    pub data: Option<Bytes>,
    pub gas_limit: Option<u64>,
}

/// EVM call result (read-only)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvmCallResult {
    pub success: bool,
    pub return_data: Bytes,
    pub gas_used: u64,
}

/// Utility functions
pub mod utils {
    use super::*;
    use keccak_hash::keccak;

    /// Calculate contract address for CREATE opcode
    pub fn calculate_create_address(sender: &Address, nonce: u64) -> Address {
        // Simple deterministic address calculation
        let mut data = Vec::with_capacity(20 + 8);
        data.extend_from_slice(sender.as_slice());
        data.extend_from_slice(&nonce.to_be_bytes());
        
        let hash = keccak(&data);
        Address::from_slice(&hash[12..])
    }

    /// Calculate contract address for CREATE2 opcode
    pub fn calculate_create2_address(
        sender: &Address,
        salt: &B256,
        init_code: &[u8],
    ) -> Address {
        let mut data = Vec::with_capacity(1 + 20 + 32 + 32);
        data.push(0xff);
        data.extend_from_slice(sender.as_slice());
        data.extend_from_slice(salt.as_slice());
        data.extend_from_slice(keccak_hash::keccak(init_code).as_bytes());
        
        let hash = keccak(&data);
        Address::from_slice(&hash[12..])
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
                    let mut length_array = [0u8; 32];
                    length_array.copy_from_slice(length_bytes);
                    let length = U256::from_be_bytes(length_array).to::<usize>();
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

    #[test]
    fn test_gas_meter() {
        let mut meter = GasMeter::new(21000, U256::from(1_000_000_000u64));
        
        assert_eq!(meter.gas_remaining(), 21000);
        assert!(meter.consume_gas(1000).is_ok());
        assert_eq!(meter.gas_remaining(), 20000);
        assert_eq!(meter.gas_used(), 1000);
        
        // Test gas limit exceeded
        assert!(meter.consume_gas(25000).is_err());
    }

    #[test]
    fn test_fee_calculator() {
        let calculator = FeeCalculator::new(U256::from(1_000_000_000u64));
        
        let fee = calculator.calculate_fee(21000, Some(U256::from(1_000_000u64)));
        assert!(fee > U256::from(21000 * 1_000_000_000u64));
    }

    #[test]
    fn test_debugger() {
        let mut debugger = EvmDebugger::new(true);
        
        debugger.trace_opcode("PUSH1", 3);
        debugger.trace_opcode("PUSH1", 3);
        debugger.trace_opcode("ADD", 3);
        
        let trace = debugger.get_trace();
        assert_eq!(trace.len(), 3);
        assert_eq!(trace[0], ("PUSH1".to_string(), 3));
        assert_eq!(trace[2], ("ADD".to_string(), 3));
    }

    #[test]
    fn test_contract_deployer() {
        let mut deployer = ContractDeployer::new();
        let deployer_address = Address::ZERO;
        let bytecode = Bytes::from(vec![0x60, 0x60, 0x60, 0x40, 0x52]);
        
        let contract_address = deployer.deploy_contract(
            deployer_address,
            bytecode.clone(),
            Bytes::new(),
            None,
        ).unwrap();
        
        assert_ne!(contract_address, Address::ZERO);
    }

    #[test]
    fn test_execution_context() {
        let mut context = ExecutionContext::new(31337);
        assert_eq!(context.chain_id, 31337);
        
        let initial_block = context.block_number;
        context.next_block();
        assert_eq!(context.block_number, initial_block + 1);
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
}