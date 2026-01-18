use alloy_primitives::{Address, U256, B256, Bytes};
use std::collections::HashMap;
use thiserror::Error;
use serde::{Serialize, Deserialize};

#[derive(Error, Debug)]
pub enum DatabaseError {
    #[error("Account not found: {0:?}")]
    AccountNotFound(Address),
    #[error("Storage error: {0}")]
    StorageError(String),
    #[error("Code not found for address: {0:?}")]
    CodeNotFound(Address),
    #[error("Block hash not found for number: {0}")]
    BlockHashNotFound(u64),
    #[error("Invalid operation: {0}")]
    InvalidOperation(String),
}

/// Account information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountInfo {
    pub balance: U256,
    pub nonce: u64,
    pub code_hash: B256,
    pub code: Option<Bytes>,
}

impl AccountInfo {
    pub fn new(balance: U256, nonce: u64) -> Self {
        Self {
            balance,
            nonce,
            code_hash: B256::from_slice(keccak_hash::keccak(&[]).as_bytes()),
            code: None,
        }
    }

    pub fn with_code(balance: U256, nonce: u64, code: Bytes) -> Self {
        let code_hash = keccak_hash::keccak(&code);
        Self {
            balance,
            nonce,
            code_hash: B256::from_slice(code_hash.as_bytes()),
            code: Some(code),
        }
    }

    pub fn is_contract(&self) -> bool {
        self.code.is_some() && !self.code.as_ref().unwrap().is_empty()
    }

    pub fn get_code(&self) -> &Bytes {
        static EMPTY_BYTES: Bytes = Bytes::new();
        self.code.as_ref().unwrap_or(&EMPTY_BYTES)
    }
}

impl Default for AccountInfo {
    fn default() -> Self {
        Self::new(U256::ZERO, 0)
    }
}

/// In-memory EVM database implementation
#[derive(Debug, Clone, Default)]
pub struct InMemoryDatabase {
    accounts: HashMap<Address, AccountInfo>,
    storage: HashMap<(Address, U256), U256>,
    block_hashes: HashMap<u64, B256>,
}

impl InMemoryDatabase {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create an account with balance
    pub fn create_account(&mut self, address: Address, balance: U256) -> Result<(), DatabaseError> {
        let account = AccountInfo::new(balance, 0);
        self.accounts.insert(address, account);
        Ok(())
    }

    /// Deploy contract code
    pub fn deploy_contract(&mut self, address: Address, code: Bytes, balance: U256) -> Result<(), DatabaseError> {
        let account = AccountInfo::with_code(balance, 1, code);
        self.accounts.insert(address, account);
        Ok(())
    }

    /// Get account info
    pub fn get_account(&self, address: &Address) -> Option<&AccountInfo> {
        self.accounts.get(address)
    }

    /// Get account info (mutable)
    pub fn get_account_mut(&mut self, address: &Address) -> Option<&mut AccountInfo> {
        self.accounts.get_mut(address)
    }

    /// Set account balance
    pub fn set_balance(&mut self, address: Address, balance: U256) -> Result<(), DatabaseError> {
        if let Some(account) = self.accounts.get_mut(&address) {
            account.balance = balance;
            Ok(())
        } else {
            self.create_account(address, balance)
        }
    }

    /// Get account balance
    pub fn get_balance(&self, address: &Address) -> U256 {
        self.accounts.get(address)
            .map(|acc| acc.balance)
            .unwrap_or(U256::ZERO)
    }

    /// Set account nonce
    pub fn set_nonce(&mut self, address: Address, nonce: u64) -> Result<(), DatabaseError> {
        if let Some(account) = self.accounts.get_mut(&address) {
            account.nonce = nonce;
            Ok(())
        } else {
            Err(DatabaseError::AccountNotFound(address))
        }
    }

    /// Get account nonce
    pub fn get_nonce(&self, address: &Address) -> u64 {
        self.accounts.get(address)
            .map(|acc| acc.nonce)
            .unwrap_or(0)
    }

    /// Increment account nonce
    pub fn increment_nonce(&mut self, address: Address) -> Result<u64, DatabaseError> {
        if let Some(account) = self.accounts.get_mut(&address) {
            account.nonce += 1;
            Ok(account.nonce)
        } else {
            Err(DatabaseError::AccountNotFound(address))
        }
    }

    /// Set storage value
    pub fn set_storage(&mut self, address: Address, key: U256, value: U256) {
        if value == U256::ZERO {
            self.storage.remove(&(address, key));
        } else {
            self.storage.insert((address, key), value);
        }
    }

    /// Get storage value
    pub fn get_storage(&self, address: &Address, key: &U256) -> U256 {
        self.storage.get(&(*address, *key)).copied().unwrap_or(U256::ZERO)
    }

    /// Set block hash
    pub fn set_block_hash(&mut self, number: u64, hash: B256) {
        self.block_hashes.insert(number, hash);
    }

    /// Get block hash
    pub fn get_block_hash(&self, number: u64) -> Option<B256> {
        self.block_hashes.get(&number).copied()
    }

    /// Check if account exists
    pub fn account_exists(&self, address: &Address) -> bool {
        self.accounts.contains_key(address)
    }

    /// Check if address is a contract
    pub fn is_contract(&self, address: &Address) -> bool {
        self.accounts.get(address)
            .map(|acc| acc.is_contract())
            .unwrap_or(false)
    }

    /// Get contract code
    pub fn get_code(&self, address: &Address) -> Bytes {
        self.accounts.get(address)
            .map(|acc| acc.get_code().clone())
            .unwrap_or_default()
    }

    /// Set contract code
    pub fn set_code(&mut self, address: Address, code: Bytes) -> Result<(), DatabaseError> {
        if let Some(account) = self.accounts.get_mut(&address) {
            let code_hash = keccak_hash::keccak(&code);
            account.code_hash = B256::from_slice(code_hash.as_bytes());
            account.code = Some(code);
            Ok(())
        } else {
            Err(DatabaseError::AccountNotFound(address))
        }
    }

    /// Get all accounts (for debugging/testing)
    pub fn get_all_accounts(&self) -> &HashMap<Address, AccountInfo> {
        &self.accounts
    }

    /// Clear all data
    pub fn clear(&mut self) {
        self.accounts.clear();
        self.storage.clear();
        self.block_hashes.clear();
    }

    /// Get storage entries for an address
    pub fn get_storage_entries(&self, address: &Address) -> Vec<(U256, U256)> {
        self.storage.iter()
            .filter_map(|((addr, key), value)| {
                if addr == address {
                    Some((*key, *value))
                } else {
                    None
                }
            })
            .collect()
    }
}

/// Wrapper for Catalyst state integration
#[derive(Clone)]
pub struct CatalystDatabase {
    inner: InMemoryDatabase,
    // TODO: Add integration with Catalyst state manager
    precompiles_enabled: bool,
}

impl CatalystDatabase {
    pub fn new() -> Self {
        Self {
            inner: InMemoryDatabase::new(),
            precompiles_enabled: false,
        }
    }

    pub fn with_genesis_accounts(mut self, accounts: Vec<(Address, U256)>) -> Self {
        for (address, balance) in accounts {
            let _ = self.inner.create_account(address, balance);
        }
        self
    }

    /// Add precompiled contracts
    pub fn with_precompiles(mut self) -> Self {
        self.add_ethereum_precompiles();
        self.add_catalyst_precompiles();
        self.precompiles_enabled = true;
        self
    }

    fn add_ethereum_precompiles(&mut self) {
        // ECRecover (0x01)
        let _ = self.inner.create_account(Address::from([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]), U256::ZERO);
        
        // SHA256 (0x02)
        let _ = self.inner.create_account(Address::from([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2]), U256::ZERO);
        
        // RIPEMD160 (0x03)
        let _ = self.inner.create_account(Address::from([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3]), U256::ZERO);
        
        // Identity (0x04)
        let _ = self.inner.create_account(Address::from([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 4]), U256::ZERO);
        
        // ModExp (0x05)
        let _ = self.inner.create_account(Address::from([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5]), U256::ZERO);
        
        // ECAdd (0x06)
        let _ = self.inner.create_account(Address::from([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 6]), U256::ZERO);
        
        // ECMul (0x07)
        let _ = self.inner.create_account(Address::from([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 7]), U256::ZERO);
        
        // ECPairing (0x08)
        let _ = self.inner.create_account(Address::from([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 8]), U256::ZERO);
        
        // Blake2F (0x09)
        let _ = self.inner.create_account(Address::from([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 9]), U256::ZERO);
    }

    fn add_catalyst_precompiles(&mut self) {
        // Catalyst consensus operations (0x100)
        let mut addr_bytes = [0u8; 20];
        addr_bytes[18] = 0x01; // 0x0100
        let _ = self.inner.create_account(Address::from(addr_bytes), U256::ZERO);
        
        // Catalyst DFS operations (0x101)
        addr_bytes[19] = 0x01; // 0x0101
        let _ = self.inner.create_account(Address::from(addr_bytes), U256::ZERO);
        
        // Catalyst cross-runtime calls (0x102)
        addr_bytes[19] = 0x02; // 0x0102
        let _ = self.inner.create_account(Address::from(addr_bytes), U256::ZERO);
        
        // Catalyst confidential transactions (0x103)
        addr_bytes[19] = 0x03; // 0x0103
        let _ = self.inner.create_account(Address::from(addr_bytes), U256::ZERO);
    }

    /// Access the inner database for advanced operations
    pub fn inner(&self) -> &InMemoryDatabase {
        &self.inner
    }

    /// Mutable access to inner database
    pub fn inner_mut(&mut self) -> &mut InMemoryDatabase {
        &mut self.inner
    }

    /// Check if precompiles are enabled
    pub fn has_precompiles(&self) -> bool {
        self.precompiles_enabled
    }

    /// Transfer value between accounts
    pub fn transfer(&mut self, from: Address, to: Address, value: U256) -> Result<(), DatabaseError> {
        // Check if sender has sufficient balance
        let from_balance = self.inner.get_balance(&from);
        if from_balance < value {
            return Err(DatabaseError::InvalidOperation(
                format!("Insufficient balance: {} < {}", from_balance, value)
            ));
        }

        // Deduct from sender
        self.inner.set_balance(from, from_balance - value)?;
        
        // Add to receiver
        let to_balance = self.inner.get_balance(&to);
        self.inner.set_balance(to, to_balance + value)?;

        Ok(())
    }

    /// Execute a simple contract call (placeholder)
    pub fn call_contract(
        &mut self,
        _from: Address,
        _to: Address,
        _data: &[u8],
        _gas_limit: u64,
    ) -> Result<(bool, Bytes, u64), DatabaseError> {
        // Placeholder implementation
        // In a real implementation, this would execute EVM bytecode
        Ok((true, Bytes::new(), 21000))
    }
}

impl Default for CatalystDatabase {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper functions for database operations
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

    /// Check if address is a precompile
    pub fn is_precompile(address: &Address) -> bool {
        let addr_bytes = address.as_slice();
        
        // Standard Ethereum precompiles (0x01 - 0x09)
        if addr_bytes[0..19] == [0u8; 19] && addr_bytes[19] >= 1 && addr_bytes[19] <= 9 {
            return true;
        }
        
        // Catalyst precompiles (0x0100 - 0x0103)
        if addr_bytes[0..18] == [0u8; 18] && addr_bytes[18] == 1 && addr_bytes[19] <= 3 {
            return true;
        }
        
        false
    }

    /// Get precompile name for debugging
    pub fn get_precompile_name(address: &Address) -> Option<&'static str> {
        let addr_bytes = address.as_slice();
        
        if addr_bytes[0..19] == [0u8; 19] {
            match addr_bytes[19] {
                1 => Some("ECRecover"),
                2 => Some("SHA256"),
                3 => Some("RIPEMD160"),
                4 => Some("Identity"),
                5 => Some("ModExp"),
                6 => Some("ECAdd"),
                7 => Some("ECMul"),
                8 => Some("ECPairing"),
                9 => Some("Blake2F"),
                _ => None,
            }
        } else if addr_bytes[0..18] == [0u8; 18] && addr_bytes[18] == 1 {
            match addr_bytes[19] {
                0 => Some("CatalystConsensus"),
                1 => Some("CatalystDFS"),
                2 => Some("CatalystCrossRuntime"),
                3 => Some("CatalystConfidential"),
                _ => None,
            }
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_in_memory_database() {
        let mut db = InMemoryDatabase::new();
        let address = Address::from([1u8; 20]);
        let balance = U256::from(1000);

        db.create_account(address, balance).unwrap();
        
        let account = db.get_account(&address).unwrap();
        assert_eq!(account.balance, balance);
        assert_eq!(account.nonce, 0);
        assert!(!account.is_contract());
    }

    #[test]
    fn test_storage() {
        let mut db = InMemoryDatabase::new();
        let address = Address::from([1u8; 20]);
        let key = U256::from(42);
        let value = U256::from(1337);

        db.set_storage(address, key, value);
        
        let stored_value = db.get_storage(&address, &key);
        assert_eq!(stored_value, value);
    }

    #[test]
    fn test_catalyst_database_with_precompiles() {
        let db = CatalystDatabase::new().with_precompiles();
        
        assert!(db.has_precompiles());
        
        // Test that standard precompiles are present
        let ecrecover_addr = Address::from([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);
        assert!(db.inner().account_exists(&ecrecover_addr));
        
        // Test that Catalyst precompiles are present
        // 0x0100 => last two bytes [0x01, 0x00]
        let catalyst_consensus_addr = Address::from([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0]);
        assert!(db.inner().account_exists(&catalyst_consensus_addr));
    }

    #[test]
    fn test_transfer() {
        let mut db = CatalystDatabase::new();
        let from = Address::from([1u8; 20]);
        let to = Address::from([2u8; 20]);
        
        // Create accounts
        db.inner_mut().create_account(from, U256::from(1000)).unwrap();
        db.inner_mut().create_account(to, U256::from(500)).unwrap();
        
        // Transfer
        db.transfer(from, to, U256::from(200)).unwrap();
        
        assert_eq!(db.inner().get_balance(&from), U256::from(800));
        assert_eq!(db.inner().get_balance(&to), U256::from(700));
    }

    #[test]
    fn test_insufficient_balance_transfer() {
        let mut db = CatalystDatabase::new();
        let from = Address::from([1u8; 20]);
        let to = Address::from([2u8; 20]);
        
        // Create accounts
        db.inner_mut().create_account(from, U256::from(100)).unwrap();
        db.inner_mut().create_account(to, U256::from(0)).unwrap();
        
        // Try to transfer more than balance
        let result = db.transfer(from, to, U256::from(200));
        assert!(result.is_err());
    }

    #[test]
    fn test_contract_deployment() {
        let mut db = InMemoryDatabase::new();
        let address = Address::from([1u8; 20]);
        let code = Bytes::from(vec![0x60, 0x60, 0x60, 0x40, 0x52]);
        let balance = U256::from(1000);

        db.deploy_contract(address, code.clone(), balance).unwrap();
        
        let account = db.get_account(&address).unwrap();
        assert_eq!(account.balance, balance);
        assert_eq!(account.nonce, 1);
        assert!(account.is_contract());
        assert_eq!(*account.get_code(), code);
    }

    #[test]
    fn test_nonce_operations() {
        let mut db = InMemoryDatabase::new();
        let address = Address::from([1u8; 20]);
        
        db.create_account(address, U256::from(1000)).unwrap();
        
        assert_eq!(db.get_nonce(&address), 0);
        
        let new_nonce = db.increment_nonce(address).unwrap();
        assert_eq!(new_nonce, 1);
        assert_eq!(db.get_nonce(&address), 1);
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
    fn test_precompile_detection() {
        let ecrecover = Address::from([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);
        // 0x0100 => last two bytes [0x01, 0x00]
        let catalyst_consensus = Address::from([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0]);
        let regular_address = Address::from([1u8; 20]);
        
        assert!(utils::is_precompile(&ecrecover));
        assert!(utils::is_precompile(&catalyst_consensus));
        assert!(!utils::is_precompile(&regular_address));
        
        assert_eq!(utils::get_precompile_name(&ecrecover), Some("ECRecover"));
        assert_eq!(utils::get_precompile_name(&catalyst_consensus), Some("CatalystConsensus"));
        assert_eq!(utils::get_precompile_name(&regular_address), None);
    }
}