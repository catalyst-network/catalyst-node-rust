use ethereum_types::{Address, U256, H256};
use revm::primitives::{AccountInfo, Bytecode};
use revm::Database;
use std::collections::HashMap;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DatabaseError {
    #[error("Account not found: {0:?}")]
    AccountNotFound(Address),
    #[error("Storage error: {0}")]
    StorageError(String),
    #[error("Code not found for address: {0:?}")]
    CodeNotFound(Address),
}

/// In-memory EVM database implementation
#[derive(Debug, Clone, Default)]
pub struct InMemoryDatabase {
    accounts: HashMap<Address, AccountInfo>,
    storage: HashMap<(Address, U256), U256>,
    code: HashMap<Address, Bytecode>,
    block_hashes: HashMap<u64, H256>,
}

impl InMemoryDatabase {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_account(&mut self, address: Address, account: AccountInfo) {
        self.accounts.insert(address, account);
    }

    pub fn insert_code(&mut self, address: Address, code: Bytecode) {
        self.code.insert(address, code);
    }

    pub fn set_storage(&mut self, address: Address, key: U256, value: U256) {
        self.storage.insert((address, key), value);
    }

    pub fn set_block_hash(&mut self, number: u64, hash: H256) {
        self.block_hashes.insert(number, hash);
    }

    /// Create an account with balance
    pub fn create_account(&mut self, address: Address, balance: U256) {
        let account = AccountInfo {
            balance,
            nonce: 0,
            code_hash: keccak_hash::keccak(&[]).into(),
            code: None,
        };
        self.insert_account(address, account);
    }

    /// Deploy contract code
    pub fn deploy_contract(&mut self, address: Address, code: Vec<u8>, balance: U256) {
        let code_hash = keccak_hash::keccak(&code);
        let bytecode = Bytecode::new_raw(code.into());
        
        let account = AccountInfo {
            balance,
            nonce: 1,
            code_hash: code_hash.into(),
            code: Some(bytecode.clone()),
        };
        
        self.insert_account(address, account);
        self.insert_code(address, bytecode);
    }
}

impl Database for InMemoryDatabase {
    type Error = DatabaseError;

    fn basic(&mut self, address: Address) -> Result<Option<AccountInfo>, Self::Error> {
        Ok(self.accounts.get(&address).cloned())
    }

    fn code_by_hash(&mut self, code_hash: H256) -> Result<Bytecode, Self::Error> {
        // For simplicity, iterate through all code to find by hash
        for (_, code) in &self.code {
            if code.hash_slow() == code_hash {
                return Ok(code.clone());
            }
        }
        Err(DatabaseError::CodeNotFound(Address::zero())) // Can't determine address from hash
    }

    fn storage(&mut self, address: Address, index: U256) -> Result<U256, Self::Error> {
        Ok(self.storage.get(&(address, index)).copied().unwrap_or(U256::zero()))
    }

    fn block_hash(&mut self, number: U256) -> Result<H256, Self::Error> {
        let block_number = number.low_u64();
        Ok(self.block_hashes
            .get(&block_number)
            .copied()
            .unwrap_or(H256::zero()))
    }
}

/// Wrapper for Catalyst state integration
#[derive(Clone)]
pub struct CatalystDatabase {
    inner: InMemoryDatabase,
    // TODO: Add integration with Catalyst state manager
}

impl CatalystDatabase {
    pub fn new() -> Self {
        Self {
            inner: InMemoryDatabase::new(),
        }
    }

    pub fn with_genesis_accounts(mut self, accounts: Vec<(Address, U256)>) -> Self {
        for (address, balance) in accounts {
            self.inner.create_account(address, balance);
        }
        self
    }
}

impl Database for CatalystDatabase {
    type Error = DatabaseError;

    fn basic(&mut self, address: Address) -> Result<Option<AccountInfo>, Self::Error> {
        self.inner.basic(address)
    }

    fn code_by_hash(&mut self, code_hash: H256) -> Result<Bytecode, Self::Error> {
        self.inner.code_by_hash(code_hash)
    }

    fn storage(&mut self, address: Address, index: U256) -> Result<U256, Self::Error> {
        self.inner.storage(address, index)
    }

    fn block_hash(&mut self, number: U256) -> Result<H256, Self::Error> {
        self.inner.block_hash(number)
    }
}

impl Default for CatalystDatabase {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_in_memory_database() {
        let mut db = InMemoryDatabase::new();
        let address = Address::from_low_u64_be(1);
        let balance = U256::from(1000);

        db.create_account(address, balance);
        
        let account = db.basic(address).unwrap().unwrap();
        assert_eq!(account.balance, balance);
        assert_eq!(account.nonce, 0);
    }

    #[test]
    fn test_storage() {
        let mut db = InMemoryDatabase::new();
        let address = Address::from_low_u64_be(1);
        let key = U256::from(42);
        let value = U256::from(1337);

        db.set_storage(address, key, value);
        
        let stored_value = db.storage(address, key).unwrap();
        assert_eq!(stored_value, value);
    }
}