// Add these to crates/catalyst-core/src/lib.rs
// (Replace the problematic Hash/Address definitions)

// First, add these imports at the top:
use serde::{Deserialize, Serialize};

// Then add these type definitions (avoid conflicts by using different names or removing imports):

/// 32-byte hash type used throughout Catalyst
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CatalystHash([u8; 32]);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncStatus {
    /// Node is fully synced
    Synced,
    /// Node is currently syncing
    Syncing { progress: f64 },
    /// Node is not synced
    NotSynced,
}

/// Resource metrics for monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceMetrics {
    /// CPU usage percentage (0.0 - 100.0)
    pub cpu_usage: f64,
    /// Memory usage in bytes
    pub memory_usage: u64,
    /// Disk usage in bytes
    pub disk_usage: u64,
}

/// Complete node status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeStatus {
    /// Node identifier
    pub id: String,
    /// Uptime in seconds
    pub uptime: u64,
    /// Synchronization status
    pub sync_status: SyncStatus,
    /// Resource usage metrics
    pub metrics: ResourceMetrics,
}

impl CatalystHash {
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
}

impl Default for CatalystHash {
    fn default() -> Self {
        Self::zero()
    }
}

impl std::fmt::Display for CatalystHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "0x{}", hex::encode(self.0))
    }
}

impl From<[u8; 32]> for CatalystHash {
    fn from(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl AsRef<[u8]> for CatalystHash {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

/// Ethereum-style address (20 bytes)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CatalystAddress([u8; 20]);

impl CatalystAddress {
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
}

impl Default for CatalystAddress {
    fn default() -> Self {
        Self::zero()
    }
}

impl std::fmt::Display for CatalystAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "0x{}", hex::encode(self.0))
    }
}

impl From<[u8; 20]> for CatalystAddress {
    fn from(bytes: [u8; 20]) -> Self {
        Self(bytes)
    }
}

impl AsRef<[u8]> for CatalystAddress {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

// Type aliases for compatibility
pub type Hash = CatalystHash;
pub type Address = CatalystAddress;
