//! Catalyst utilities and common types
//! 
//! This crate provides shared utilities, types, and functions used across
//! the Catalyst blockchain implementation.

use serde::{Deserialize, Serialize};
use std::fmt;

// Re-export commonly used external crates
pub use serde;
pub use serde_json;
pub use tokio;

// Module declarations
pub mod error;
pub mod logging;
pub mod metrics;
pub mod network;
pub mod patterns;
pub mod serialization;
pub mod state;

// Re-export error types for convenience
pub use error::{CatalystError, CatalystResult};

/// 32-byte hash type used throughout Catalyst
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Hash([u8; 32]);

impl Hash {
    /// Create a new hash from a 32-byte array
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
    
    /// Create a hash from a slice (panics if not 32 bytes)
    pub fn from_slice(slice: &[u8]) -> Self {
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(slice);
        Self(bytes)
    }
    
    /// Get the hash as a byte array
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
    
    /// Get the hash as a slice
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }
    
    /// Create a zero hash
    pub fn zero() -> Self {
        Self([0u8; 32])
    }
    
    /// Check if this is a zero hash
    pub fn is_zero(&self) -> bool {
        self.0 == [0u8; 32]
    }
    
    /// Get the length of the hash (always 32)
    pub fn len(&self) -> usize {
        32
    }
}

impl Default for Hash {
    fn default() -> Self {
        Self::zero()
    }
}

impl fmt::Display for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", hex::encode(self.0))
    }
}

impl From<[u8; 32]> for Hash {
    fn from(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl AsRef<[u8]> for Hash {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

// Add conversion from GenericArray for hashing compatibility
impl<T> From<sha2::digest::generic_array::GenericArray<u8, T>> for Hash 
where
    T: sha2::digest::generic_array::ArrayLength<u8>,
{
    fn from(array: sha2::digest::generic_array::GenericArray<u8, T>) -> Self {
        let mut bytes = [0u8; 32];
        let len = std::cmp::min(array.len(), 32);
        bytes[..len].copy_from_slice(&array[..len]);
        Self(bytes)
    }
}

/// Ethereum-style address (20 bytes)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Address([u8; 20]);

impl Address {
    /// Create a new address from a 20-byte array
    pub fn new(bytes: [u8; 20]) -> Self {
        Self(bytes)
    }
    
    /// Create an address from a slice (panics if not 20 bytes)
    pub fn from_slice(slice: &[u8]) -> Self {
        let mut bytes = [0u8; 20];
        bytes.copy_from_slice(slice);
        Self(bytes)
    }
    
    /// Get the address as a byte array
    pub fn as_bytes(&self) -> &[u8; 20] {
        &self.0
    }
    
    /// Get the address as a slice
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }
    
    /// Create a zero address
    pub fn zero() -> Self {
        Self([0u8; 20])
    }
    
    /// Check if this is a zero address
    pub fn is_zero(&self) -> bool {
        self.0 == [0u8; 20]
    }
    
    /// Get the length of the address (always 20)
    pub fn len(&self) -> usize {
        20
    }
}

impl Default for Address {
    fn default() -> Self {
        Self::zero()
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{}", hex::encode(self.0))
    }
}

impl From<[u8; 20]> for Address {
    fn from(bytes: [u8; 20]) -> Self {
        Self(bytes)
    }
}

impl AsRef<[u8]> for Address {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

/// Type aliases for common cryptographic types
pub type PublicKey = [u8; 32];
pub type Signature = [u8; 64];

/// System information structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    pub node_id: String,
    pub version: String,
    pub start_time: u64,
    pub uptime: u64,
}

impl SystemInfo {
    pub fn new() -> Self {
        Self {
            node_id: format!("node_{}", rand::random::<u32>()),
            version: "0.1.0".to_string(),
            start_time: utils::current_timestamp(),
            uptime: 0,
        }
    }
}

impl Default for SystemInfo {
    fn default() -> Self {
        Self::new()
    }
}

/// Transaction entry in a ledger update
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TransactionEntry {
    /// Transaction hash
    pub hash: Hash,
    /// Sender address
    pub from: Address,
    /// Recipient address (None for contract creation)
    pub to: Option<Address>,
    /// Value transferred
    pub value: u64,
    /// Gas used by this transaction
    pub gas_used: u64,
    /// Transaction status
    pub status: TransactionStatus,
}

/// Status of a transaction
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TransactionStatus {
    /// Transaction was successful
    Success,
    /// Transaction failed
    Failed,
    /// Transaction is pending
    Pending,
}

/// Account state update
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccountUpdate {
    /// Account address
    pub address: Address,
    /// New balance
    pub balance: u64,
    /// New nonce
    pub nonce: u64,
    /// Storage root hash (for contracts)
    pub storage_root: Option<Hash>,
    /// Code hash (for contracts)
    pub code_hash: Option<Hash>,
}

/// Ledger state update containing all changes in a block
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LedgerStateUpdate {
    /// State root hash after this update
    pub state_root: Hash,
    /// Block number this update corresponds to
    pub block_number: u64,
    /// Timestamp of the update
    pub timestamp: u64,
    /// Transaction entries in this update
    pub transaction_entries: Vec<TransactionEntry>,
    /// Updated account states
    pub account_updates: Vec<AccountUpdate>,
    /// Total gas used in this update
    pub gas_used: u64,
    /// Gas limit for this block
    pub gas_limit: u64,
    /// Block producer/validator address
    pub producer: Address,
    /// Additional metadata
    pub metadata: Option<Vec<u8>>,
}

impl LedgerStateUpdate {
    /// Create a new ledger state update
    pub fn new(
        state_root: Hash,
        block_number: u64,
        timestamp: u64,
        producer: Address,
    ) -> Self {
        Self {
            state_root,
            block_number,
            timestamp,
            transaction_entries: Vec::new(),
            account_updates: Vec::new(),
            gas_used: 0,
            gas_limit: 0,
            producer,
            metadata: None,
        }
    }
    
    /// Add a transaction entry to this update
    pub fn add_transaction(&mut self, entry: TransactionEntry) {
        self.gas_used += entry.gas_used;
        self.transaction_entries.push(entry);
    }
    
    /// Add an account update
    pub fn add_account_update(&mut self, update: AccountUpdate) {
        self.account_updates.push(update);
    }
    
    /// Get the number of transactions in this update
    pub fn transaction_count(&self) -> usize {
        self.transaction_entries.len()
    }
    
    /// Get the number of account updates
    pub fn account_update_count(&self) -> usize {
        self.account_updates.len()
    }
    
    /// Serialize the update to bytes
    pub fn serialize(&self) -> Result<Vec<u8>, CatalystError> {
        serde_json::to_vec(self)
            .map_err(|e| CatalystError::Serialization(e.to_string()))
    }
    
    /// Deserialize the update from bytes
    pub fn deserialize(data: &[u8]) -> Result<Self, CatalystError> {
        serde_json::from_slice(data)
            .map_err(|e| CatalystError::Serialization(e.to_string()))
    }
}

/// Partial ledger state update for consensus
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PartialLedgerStateUpdate {
    /// Block number
    pub block_number: u64,
    /// Partial state root
    pub partial_state_root: Hash,
    /// Transaction hashes included
    pub transaction_hashes: Vec<Hash>,
    /// Timestamp
    pub timestamp: u64,
    /// Producer address
    pub producer: Address,
}

impl PartialLedgerStateUpdate {
    /// Create a new partial update
    pub fn new(
        block_number: u64,
        partial_state_root: Hash,
        timestamp: u64,
        producer: Address,
    ) -> Self {
        Self {
            block_number,
            partial_state_root,
            transaction_hashes: Vec::new(),
            timestamp,
            producer,
        }
    }
    
    /// Add a transaction hash
    pub fn add_transaction_hash(&mut self, hash: Hash) {
        self.transaction_hashes.push(hash);
    }
}

/// Utility functions
pub mod utils {
    use std::time::SystemTime;
    
    /// Get current Unix timestamp in seconds
    pub fn current_timestamp() -> u64 {
        SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }
    
    /// Get current Unix timestamp in milliseconds
    pub fn current_timestamp_ms() -> u64 {
        SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }
    
    /// Generate random bytes
    pub fn random_bytes(len: usize) -> Vec<u8> {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        (0..len).map(|_| rng.gen()).collect()
    }
    
    /// Convert bytes to hex string
    pub fn bytes_to_hex(bytes: &[u8]) -> String {
        hex::encode(bytes)
    }
    
    /// Convert hex string to bytes
    pub fn hex_to_bytes(hex_str: &str) -> Result<Vec<u8>, hex::FromHexError> {
        let cleaned = if hex_str.starts_with("0x") {
            &hex_str[2..]
        } else {
            hex_str
        };
        hex::decode(cleaned)
    }
    
    /// Format bytes in human-readable format
    pub fn format_bytes(bytes: u64) -> String {
        const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB", "PB"];
        const THRESHOLD: f64 = 1024.0;
        
        if bytes == 0 {
            return "0 B".to_string();
        }
        
        let mut size = bytes as f64;
        let mut unit_index = 0;
        
        while size >= THRESHOLD && unit_index < UNITS.len() - 1 {
            size /= THRESHOLD;
            unit_index += 1;
        }
        
        if unit_index == 0 {
            format!("{} {}", bytes, UNITS[unit_index])
        } else {
            format!("{:.2} {}", size, UNITS[unit_index])
        }
    }
    
    /// Simple rate limiter
    pub struct RateLimiter {
        tokens: u32,
        capacity: u32,
        refill_rate: u32,
        last_refill: std::time::Instant,
    }
    
    impl RateLimiter {
        pub fn new(capacity: u32, refill_rate: u32) -> Self {
            Self {
                tokens: capacity,
                capacity,
                refill_rate,
                last_refill: std::time::Instant::now(),
            }
        }
        
        pub fn try_acquire(&mut self) -> bool {
            self.refill();
            if self.tokens > 0 {
                self.tokens -= 1;
                true
            } else {
                false
            }
        }
        
        fn refill(&mut self) {
            let now = std::time::Instant::now();
            let elapsed = now.duration_since(self.last_refill).as_secs();
            
            if elapsed > 0 {
                let new_tokens = elapsed as u32 * self.refill_rate;
                self.tokens = std::cmp::min(self.capacity, self.tokens + new_tokens);
                self.last_refill = now;
            }
        }
    }
}

/// Async trait re-export
pub use async_trait::async_trait;

/// Logging macros for Catalyst components
#[macro_export]
macro_rules! log_info {
    ($category:expr, $($arg:tt)*) => {
        println!("[INFO] [{:?}] {}", $category, format!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_warn {
    ($category:expr, $($arg:tt)*) => {
        eprintln!("[WARN] [{:?}] {}", $category, format!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_error {
    ($category:expr, $($arg:tt)*) => {
        eprintln!("[ERROR] [{:?}] {}", $category, format!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_debug {
    ($category:expr, $($arg:tt)*) => {
        println!("[DEBUG] [{:?}] {}", $category, format!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_trace {
    ($category:expr, $($arg:tt)*) => {
        println!("[TRACE] [{:?}] {}", $category, format!($($arg)*))
    };
}

/// Cryptographic utilities
pub mod crypto {
    use super::Hash;
    use sha2::{Sha256, Digest};
    
    /// Hash arbitrary data using SHA-256
    pub fn hash_data(data: &[u8]) -> Hash {
        let mut hasher = Sha256::new();
        hasher.update(data);
        let result = hasher.finalize();
        Hash::from(result)
    }
    
    /// Hash multiple pieces of data together
    pub fn hash_multiple(data_parts: &[&[u8]]) -> Hash {
        let mut hasher = Sha256::new();
        for part in data_parts {
            hasher.update(part);
        }
        let result = hasher.finalize();
        Hash::from(result)
    }
}

/// Time utilities
pub mod time {
    /// Get current Unix timestamp in seconds
    pub fn current_timestamp() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }
    
    /// Get current Unix timestamp in milliseconds
    pub fn current_timestamp_millis() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }
}

/// Serialization traits for Catalyst types
pub mod serialization_traits {
    use super::CatalystError;
    
    /// Trait for serializing Catalyst types
    pub trait CatalystSerialize {
        fn serialize(&self) -> Result<Vec<u8>, CatalystError>;
    }
    
    /// Trait for deserializing Catalyst types
    pub trait CatalystDeserialize: Sized {
        fn deserialize(data: &[u8]) -> Result<Self, CatalystError>;
    }
    
    // Implement for common types
    impl CatalystSerialize for String {
        fn serialize(&self) -> Result<Vec<u8>, CatalystError> {
            Ok(self.as_bytes().to_vec())
        }
    }
    
    impl CatalystDeserialize for String {
        fn deserialize(data: &[u8]) -> Result<Self, CatalystError> {
            String::from_utf8(data.to_vec())
                .map_err(|e| CatalystError::Serialization(e.to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_hash_creation() {
        let hash = Hash::new([1u8; 32]);
        assert_eq!(hash.as_bytes(), &[1u8; 32]);
        assert!(!hash.is_zero());
        assert_eq!(hash.len(), 32);
        
        let zero_hash = Hash::zero();
        assert!(zero_hash.is_zero());
    }
    
    #[test]
    fn test_address_creation() {
        let addr = Address::new([1u8; 20]);
        assert_eq!(addr.as_bytes(), &[1u8; 20]);
        assert!(!addr.is_zero());
        assert_eq!(addr.len(), 20);
        
        let zero_addr = Address::zero();
        assert!(zero_addr.is_zero());
    }
    
    #[test]
    fn test_ledger_state_update() {
        let state_root = Hash::new([1u8; 32]);
        let producer = Address::new([2u8; 20]);
        let mut update = LedgerStateUpdate::new(state_root, 100, 1234567890, producer);
        
        assert_eq!(update.block_number, 100);
        assert_eq!(update.timestamp, 1234567890);
        assert_eq!(update.transaction_count(), 0);
        
        let tx_entry = TransactionEntry {
            hash: Hash::new([3u8; 32]),
            from: Address::new([4u8; 20]),
            to: Some(Address::new([5u8; 20])),
            value: 1000,
            gas_used: 21000,
            status: TransactionStatus::Success,
        };
        
        update.add_transaction(tx_entry);
        assert_eq!(update.transaction_count(), 1);
        assert_eq!(update.gas_used, 21000);
    }
    
    #[test]
    fn test_serialization() {
        let update = LedgerStateUpdate::new(
            Hash::new([1u8; 32]),
            100,
            1234567890,
            Address::new([2u8; 20]),
        );
        
        let serialized = update.serialize().unwrap();
        let deserialized = LedgerStateUpdate::deserialize(&serialized).unwrap();
        
        assert_eq!(update, deserialized);
    }
    
    #[test]
    fn test_crypto_utils() {
        let data = b"hello world";
        let hash1 = crypto::hash_data(data);
        let hash2 = crypto::hash_data(data);
        
        // Same data should produce same hash
        assert_eq!(hash1, hash2);
        
        // Different data should produce different hash
        let hash3 = crypto::hash_data(b"hello world!");
        assert_ne!(hash1, hash3);
    }
    
    #[test]
    fn test_time_utils() {
        let timestamp = time::current_timestamp();
        let timestamp_millis = time::current_timestamp_millis();
        
        // Timestamp in millis should be larger
        assert!(timestamp_millis > timestamp);
        
        // Should be reasonable timestamps (after 2020)
        assert!(timestamp > 1577836800); // 2020-01-01
    }
    
    #[test]
    fn test_utils() {
        // Test hex conversion
        let data = vec![0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF];
        let hex = utils::bytes_to_hex(&data);
        let decoded = utils::hex_to_bytes(&hex).unwrap();
        assert_eq!(data, decoded);
        
        // Test format bytes
        assert_eq!(utils::format_bytes(0), "0 B");
        assert_eq!(utils::format_bytes(1024), "1.00 KB");
        assert_eq!(utils::format_bytes(1536), "1.50 KB");
        
        // Test random bytes
        let random1 = utils::random_bytes(32);
        let random2 = utils::random_bytes(32);
        assert_eq!(random1.len(), 32);
        assert_eq!(random2.len(), 32);
        assert_ne!(random1, random2);
    }
    
    #[test]
    fn test_rate_limiter() {
        let mut limiter = utils::RateLimiter::new(5, 1);
        
        // Should allow 5 requests initially
        for _ in 0..5 {
            assert!(limiter.try_acquire());
        }
        
        // Should reject the 6th request
        assert!(!limiter.try_acquire());
    }
    
    #[test]
    fn test_system_info() {
        let info1 = SystemInfo::new();
        let info2 = SystemInfo::default();
        
        assert_eq!(info1.version, "0.1.0");
        assert_eq!(info2.version, "0.1.0");
        assert!(info1.node_id.starts_with("node_"));
    }
}