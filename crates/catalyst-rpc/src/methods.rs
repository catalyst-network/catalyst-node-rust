//! RPC method implementations and utilities
//! Create this as crates/catalyst-rpc/src/methods.rs

use crate::types::*;
use crate::CatalystRpcError;
use serde::{Deserialize, Serialize};

/// Utility functions for RPC method implementations
pub mod utils {
    use super::*;
    
    /// Validate hex string format
    pub fn validate_hex_string(input: &str) -> Result<(), CatalystRpcError> {
        if !input.starts_with("0x") {
            return Err(CatalystRpcError::InvalidParams(
                "Hex string must start with 0x".to_string(),
            ));
        }
        
        let hex_part = &input[2..];
        if hex_part.len() % 2 != 0 {
            return Err(CatalystRpcError::InvalidParams(
                "Hex string must have even length".to_string(),
            ));
        }
        
        if !hex_part.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(CatalystRpcError::InvalidParams(
                "Invalid hex characters".to_string(),
            ));
        }
        
        Ok(())
    }
    
    /// Validate Ethereum-style address
    pub fn validate_address(address: &str) -> Result<(), CatalystRpcError> {
        validate_hex_string(address)?;
        
        if address.len() != 42 { // "0x" + 40 hex chars
            return Err(CatalystRpcError::InvalidParams(
                "Address must be 42 characters long (0x + 40 hex chars)".to_string(),
            ));
        }
        
        Ok(())
    }
    
    /// Validate transaction hash
    pub fn validate_tx_hash(hash: &str) -> Result<(), CatalystRpcError> {
        validate_hex_string(hash)?;
        
        if hash.len() != 66 { // "0x" + 64 hex chars
            return Err(CatalystRpcError::InvalidParams(
                "Transaction hash must be 66 characters long (0x + 64 hex chars)".to_string(),
            ));
        }
        
        Ok(())
    }
    
    /// Validate block hash
    pub fn validate_block_hash(hash: &str) -> Result<(), CatalystRpcError> {
        validate_tx_hash(hash) // Same format as transaction hash
    }
    
    /// Parse numeric string to u64
    pub fn parse_numeric_string(input: &str) -> Result<u64, CatalystRpcError> {
        if input.starts_with("0x") {
            // Parse hex
            u64::from_str_radix(&input[2..], 16)
                .map_err(|_| CatalystRpcError::InvalidParams(
                    "Invalid hex number".to_string(),
                ))
        } else {
            // Parse decimal
            input.parse::<u64>()
                .map_err(|_| CatalystRpcError::InvalidParams(
                    "Invalid decimal number".to_string(),
                ))
        }
    }
    
    /// Format u64 as hex string
    pub fn format_hex_u64(value: u64) -> String {
        format!("0x{:x}", value)
    }
    
    /// Format bytes as hex string
    pub fn format_hex_bytes(bytes: &[u8]) -> String {
        format!("0x{}", hex::encode(bytes))
    }
    
    /// Validate and parse gas price
    pub fn validate_gas_price(gas_price: &str) -> Result<u64, CatalystRpcError> {
        let price = parse_numeric_string(gas_price)?;
        
        if price == 0 {
            return Err(CatalystRpcError::InvalidParams(
                "Gas price must be greater than 0".to_string(),
            ));
        }
        
        // Reasonable upper limit (1000 Gwei)
        if price > 1_000_000_000_000 {
            return Err(CatalystRpcError::InvalidParams(
                "Gas price too high".to_string(),
            ));
        }
        
        Ok(price)
    }
    
    /// Validate transaction value
    pub fn validate_transaction_value(value: &str) -> Result<u128, CatalystRpcError> {
        if value.starts_with("0x") {
            u128::from_str_radix(&value[2..], 16)
                .map_err(|_| CatalystRpcError::InvalidParams(
                    "Invalid hex value".to_string(),
                ))
        } else {
            value.parse::<u128>()
                .map_err(|_| CatalystRpcError::InvalidParams(
                    "Invalid decimal value".to_string(),
                ))
        }
    }
}

/// Block-related RPC method helpers
pub mod blocks {
    use super::*;
    
    /// Create a mock block for development
    pub fn create_mock_block(number: u64, hash: Option<String>) -> RpcBlock {
        let block_hash = hash.unwrap_or_else(|| format!("0x{:064x}", number));
        
        RpcBlock {
            hash: block_hash,
            number,
            parent_hash: if number > 0 {
                format!("0x{:064x}", number - 1)
            } else {
                "0x0000000000000000000000000000000000000000000000000000000000000000".to_string()
            },
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            transactions: vec![],
            transaction_count: 0,
            size: 1024 + (number * 10), // Simulate growing block size
        }
    }
    
    /// Create a block with transactions
    pub fn create_block_with_transactions(
        number: u64,
        transactions: Vec<RpcTransactionSummary>,
    ) -> RpcBlock {
        let mut block = create_mock_block(number, None);
        block.transaction_count = transactions.len();
        block.transactions = transactions;
        block.size += (block.transaction_count as u64) * 100; // Approximate size increase
        block
    }
}

/// Transaction-related RPC method helpers
pub mod transactions {
    use super::*;
    
    /// Create a mock transaction
    pub fn create_mock_transaction(hash: String) -> RpcTransaction {
        RpcTransaction {
            hash,
            block_hash: Some(format!("0x{:064x}", 12345)),
            block_number: Some(12345),
            from: "0x742d35cc6600c14f6d0c6bfb0c6c8e6b5e5b5e5e".to_string(),
            to: Some("0x852d35cc6600c14f6d0c6bfb0c6c8e6b5e5b5e5e".to_string()),
            value: "1000000000000000000".to_string(), // 1 token
            data: "0x".to_string(),
            gas_limit: 21000,
            gas_price: "1000000000".to_string(), // 1 Gwei
            gas_used: Some(21000),
            status: Some("success".to_string()),
        }
    }
    
    /// Validate transaction request
    pub fn validate_transaction_request(tx: &RpcTransactionRequest) -> Result<(), CatalystRpcError> {
        // Validate from address
        utils::validate_address(&tx.from)?;
        
        // Validate to address if present
        if let Some(to) = &tx.to {
            utils::validate_address(to)?;
        }
        
        // Validate value if present
        if let Some(value) = &tx.value {
            utils::validate_transaction_value(value)?;
        }
        
        // Validate gas price if present
        if let Some(gas_price) = &tx.gas_price {
            utils::validate_gas_price(gas_price)?;
        }
        
        // Validate gas limit
        if let Some(gas_limit) = tx.gas_limit {
            if gas_limit == 0 {
                return Err(CatalystRpcError::InvalidParams(
                    "Gas limit must be greater than 0".to_string(),
                ));
            }
            
            // Reasonable upper limit
            if gas_limit > 10_000_000 {
                return Err(CatalystRpcError::InvalidParams(
                    "Gas limit too high".to_string(),
                ));
            }
        }
        
        // Validate data if present
        if let Some(data) = &tx.data {
            if !data.is_empty() && data != "0x" {
                utils::validate_hex_string(data)?;
            }
        }
        
        Ok(())
    }
    
    /// Estimate gas for a transaction
    pub fn estimate_gas(tx: &RpcTransactionRequest) -> u64 {
        let mut gas = 21000; // Base transaction cost
        
        // Add cost for data
        if let Some(data) = &tx.data {
            if data.len() > 2 { // More than just "0x"
                let data_bytes = (data.len() - 2) / 2; // Convert hex chars to bytes
                gas += data_bytes as u64 * 68; // 68 gas per byte of data
            }
        }
        
        // Add cost for contract creation
        if tx.to.is_none() {
            gas += 32000; // Contract creation cost
        }
        
        gas
    }
}

/// Account-related RPC method helpers
pub mod accounts {
    use super::*;
    
    /// Create a mock account
    pub fn create_mock_account(address: String) -> RpcAccount {
        // Generate deterministic balance based on address
        let balance_seed = address.chars()
            .skip(2) // Skip "0x"
            .take(8)
            .collect::<String>();
        
        let balance_multiplier = u64::from_str_radix(&balance_seed, 16)
            .unwrap_or(1000000) % 1000;
        
        let balance = (balance_multiplier + 1) * 1_000_000_000_000_000_000; // At least 1 token
        
        RpcAccount {
            address,
            balance: balance.to_string(),
            account_type: "user".to_string(),
            nonce: balance_seed.len() as u64, // Simple nonce based on address
        }
    }
    
    /// Get account type from address pattern
    pub fn determine_account_type(address: &str) -> String {
        // Simple heuristic based on address pattern
        if address.ends_with("0000") {
            "contract".to_string()
        } else if address.starts_with("0x000") {
            "system".to_string()
        } else {
            "user".to_string()
        }
    }
}

/// Network information helpers
pub mod network {
    use super::*;
    
    /// Get mock network information
    pub fn get_network_info(current_block: u64, peer_count: u64) -> RpcNetworkInfo {
        RpcNetworkInfo {
            chain_id: 1337, // Catalyst testnet
            network_id: 1337,
            protocol_version: "catalyst/1.0".to_string(),
            genesis_hash: "0x0000000000000000000000000000000000000000000000000000000000000000".to_string(),
            current_block,
            highest_block: current_block,
            peer_count,
        }
    }
    
    /// Generate mock peer information
    pub fn generate_peer_info() -> Vec<PeerInfo> {
        vec![
            PeerInfo {
                id: "peer1".to_string(),
                address: "/ip4/192.168.1.100/tcp/30333".to_string(),
                protocols: vec!["catalyst/1.0".to_string()],
                connected_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            },
            PeerInfo {
                id: "peer2".to_string(),
                address: "/ip4/192.168.1.101/tcp/30333".to_string(),
                protocols: vec!["catalyst/1.0".to_string()],
                connected_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() - 3600, // Connected 1 hour ago
            },
        ]
    }
}

/// Peer information structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub id: String,
    pub address: String,
    pub protocols: Vec<String>,
    pub connected_at: u64,
}