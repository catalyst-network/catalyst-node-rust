use alloy_primitives::{Address, U256, B256, Bytes};
use serde::{Deserialize, Serialize};

/// EVM transaction representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvmTransaction {
    pub from: Address,
    pub to: Option<Address>,
    pub value: U256,
    pub data: Bytes,
    pub gas_limit: u64,
    pub gas_price: U256,
    pub gas_priority_fee: Option<U256>,
    pub nonce: u64,
    pub chain_id: u64,
    pub access_list: Option<Vec<AccessListItem>>,
}

impl EvmTransaction {
    /// Create a simple transfer transaction
    pub fn transfer(from: Address, to: Address, value: U256, nonce: u64, gas_price: U256) -> Self {
        Self {
            from,
            to: Some(to),
            value,
            data: Bytes::new(),
            gas_limit: 21000,
            gas_price,
            gas_priority_fee: None,
            nonce,
            chain_id: 31337,
            access_list: None,
        }
    }

    /// Create a contract deployment transaction
    pub fn deploy_contract(from: Address, bytecode: Bytes, value: U256, nonce: u64, gas_price: U256, gas_limit: u64) -> Self {
        Self {
            from,
            to: None, // Deployment transaction
            value,
            data: bytecode,
            gas_limit,
            gas_price,
            gas_priority_fee: None,
            nonce,
            chain_id: 31337,
            access_list: None,
        }
    }

    /// Create a contract call transaction
    pub fn contract_call(
        from: Address,
        to: Address,
        data: Bytes,
        value: U256,
        nonce: u64,
        gas_price: U256,
        gas_limit: u64,
    ) -> Self {
        Self {
            from,
            to: Some(to),
            value,
            data,
            gas_limit,
            gas_price,
            gas_priority_fee: None,
            nonce,
            chain_id: 31337,
            access_list: None,
        }
    }

    /// Check if this is a contract deployment
    pub fn is_create(&self) -> bool {
        self.to.is_none()
    }

    /// Get the recipient address if this is a call
    pub fn get_to(&self) -> Option<Address> {
        self.to
    }

    /// Calculate the total gas cost
    pub fn total_gas_cost(&self) -> U256 {
        U256::from(self.gas_limit) * self.gas_price
    }
}

/// Access list item for EIP-2930
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessListItem {
    pub address: Address,
    pub storage_keys: Vec<B256>,
}

impl AccessListItem {
    pub fn new(address: Address, storage_keys: Vec<B256>) -> Self {
        Self {
            address,
            storage_keys,
        }
    }

    pub fn with_single_key(address: Address, key: B256) -> Self {
        Self {
            address,
            storage_keys: vec![key],
        }
    }
}

/// Contract deployment parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractDeployment {
    pub from: Address,
    pub bytecode: Bytes,
    pub value: Option<U256>,
    pub gas_limit: u64,
    pub gas_price: U256,
    pub gas_priority_fee: Option<U256>,
    pub nonce: u64,
    pub access_list: Option<Vec<AccessListItem>>,
    pub constructor_args: Option<Bytes>,
}

impl ContractDeployment {
    pub fn new(from: Address, bytecode: Bytes, gas_limit: u64, gas_price: U256, nonce: u64) -> Self {
        Self {
            from,
            bytecode,
            value: None,
            gas_limit,
            gas_price,
            gas_priority_fee: None,
            nonce,
            access_list: None,
            constructor_args: None,
        }
    }

    pub fn with_value(mut self, value: U256) -> Self {
        self.value = Some(value);
        self
    }

    pub fn with_constructor_args(mut self, args: Bytes) -> Self {
        self.constructor_args = Some(args);
        self
    }

    pub fn to_evm_transaction(&self) -> EvmTransaction {
        let mut data = self.bytecode.clone();
        if let Some(args) = &self.constructor_args {
            data = [data.as_ref(), args.as_ref()].concat().into();
        }

        EvmTransaction {
            from: self.from,
            to: None, // Contract deployment
            value: self.value.unwrap_or(U256::ZERO),
            data,
            gas_limit: self.gas_limit,
            gas_price: self.gas_price,
            gas_priority_fee: self.gas_priority_fee,
            nonce: self.nonce,
            chain_id: 31337, // Default to Catalyst testnet
            access_list: self.access_list.clone(),
        }
    }
}

/// Contract deployment result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractDeploymentResult {
    pub success: bool,
    pub contract_address: Option<Address>,
    pub return_data: Bytes,
    pub gas_used: u64,
    pub gas_refunded: u64,
    pub transaction_hash: Option<B256>,
}

impl ContractDeploymentResult {
    pub fn success(contract_address: Address, gas_used: u64) -> Self {
        Self {
            success: true,
            contract_address: Some(contract_address),
            return_data: Bytes::new(),
            gas_used,
            gas_refunded: 0,
            transaction_hash: None,
        }
    }

    pub fn failure(gas_used: u64, return_data: Bytes) -> Self {
        Self {
            success: false,
            contract_address: None,
            return_data,
            gas_used,
            gas_refunded: 0,
            transaction_hash: None,
        }
    }
}

/// State change representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateChange {
    pub address: Address,
    pub kind: StateChangeKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StateChangeKind {
    BalanceChanged {
        old_balance: U256,
        new_balance: U256,
    },
    NonceChanged {
        old_nonce: u64,
        new_nonce: u64,
    },
    CodeChanged {
        old_code_hash: B256,
        new_code_hash: B256,
    },
    StorageChanged {
        key: U256,
        old_value: U256,
        new_value: U256,
    },
    AccountCreated {
        balance: U256,
    },
    AccountDeleted,
}

impl StateChange {
    pub fn balance_change(address: Address, old_balance: U256, new_balance: U256) -> Self {
        Self {
            address,
            kind: StateChangeKind::BalanceChanged { old_balance, new_balance },
        }
    }

    pub fn nonce_change(address: Address, old_nonce: u64, new_nonce: u64) -> Self {
        Self {
            address,
            kind: StateChangeKind::NonceChanged { old_nonce, new_nonce },
        }
    }

    pub fn account_created(address: Address, balance: U256) -> Self {
        Self {
            address,
            kind: StateChangeKind::AccountCreated { balance },
        }
    }
}

/// Transaction receipt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionReceipt {
    pub transaction_hash: B256,
    pub transaction_index: u64,
    pub block_hash: B256,
    pub block_number: u64,
    pub from: Address,
    pub to: Option<Address>,
    pub cumulative_gas_used: u64,
    pub gas_used: u64,
    pub contract_address: Option<Address>,
    pub logs: Vec<crate::EvmLog>,
    pub status: bool,
}

impl TransactionReceipt {
    pub fn new(
        transaction_hash: B256,
        transaction_index: u64,
        block_hash: B256,
        block_number: u64,
        from: Address,
        to: Option<Address>,
        gas_used: u64,
        status: bool,
    ) -> Self {
        Self {
            transaction_hash,
            transaction_index,
            block_hash,
            block_number,
            from,
            to,
            cumulative_gas_used: gas_used,
            gas_used,
            contract_address: None,
            logs: Vec::new(),
            status,
        }
    }

    pub fn with_contract_address(mut self, address: Address) -> Self {
        self.contract_address = Some(address);
        self
    }

    pub fn with_logs(mut self, logs: Vec<crate::EvmLog>) -> Self {
        self.logs = logs;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_evm_transaction_transfer() {
        let tx = EvmTransaction::transfer(
            Address::ZERO,
            Address::from([1u8; 20]),
            U256::from(1000),
            1,
            U256::from(21000),
        );

        assert_eq!(tx.from, Address::ZERO);
        assert_eq!(tx.to, Some(Address::from([1u8; 20])));
        assert_eq!(tx.value, U256::from(1000));
        assert_eq!(tx.gas_limit, 21000);
        assert!(!tx.is_create());
    }

    #[test]
    fn test_contract_deployment() {
        let bytecode = Bytes::from(vec![0x60, 0x60, 0x60, 0x40, 0x52]);
        let deployment = ContractDeployment::new(
            Address::ZERO,
            bytecode.clone(),
            1000000,
            U256::from(20_000_000_000u64),
            0,
        );

        let tx = deployment.to_evm_transaction();
        assert_eq!(tx.to, None); // Deployment has no recipient
        assert_eq!(tx.data, bytecode);
        assert_eq!(tx.gas_limit, 1000000);
        assert!(tx.is_create());
    }

    #[test]
    fn test_contract_deployment_with_args() {
        let bytecode = Bytes::from(vec![0x60, 0x60, 0x60, 0x40, 0x52]);
        let constructor_args = Bytes::from(vec![0x12, 0x34, 0x56, 0x78]);
        
        let deployment = ContractDeployment::new(
            Address::ZERO,
            bytecode.clone(),
            1000000,
            U256::from(20_000_000_000u64),
            0,
        ).with_constructor_args(constructor_args.clone());

        let tx = deployment.to_evm_transaction();
        
        // Should contain both bytecode and constructor args
        assert!(tx.data.len() > bytecode.len());
        assert!(tx.data.starts_with(&bytecode));
        assert!(tx.data.ends_with(&constructor_args));
    }

    #[test]
    fn test_access_list_item() {
        let item = AccessListItem::with_single_key(
            Address::from([1u8; 20]),
            B256::ZERO,
        );

        assert_eq!(item.address, Address::from([1u8; 20]));
        assert_eq!(item.storage_keys.len(), 1);
        assert_eq!(item.storage_keys[0], B256::ZERO);
    }

    #[test]
    fn test_state_changes() {
        let balance_change = StateChange::balance_change(
            Address::ZERO,
            U256::from(1000),
            U256::from(2000),
        );

        match balance_change.kind {
            StateChangeKind::BalanceChanged { old_balance, new_balance } => {
                assert_eq!(old_balance, U256::from(1000));
                assert_eq!(new_balance, U256::from(2000));
            }
            _ => panic!("Expected balance change"),
        }
    }

    #[test]
    fn test_transaction_receipt() {
        let receipt = TransactionReceipt::new(
            B256::ZERO,
            0,
            B256::from([1u8; 32]),
            100,
            Address::ZERO,
            Some(Address::from([1u8; 20])),
            21000,
            true,
        );

        assert_eq!(receipt.block_number, 100);
        assert_eq!(receipt.gas_used, 21000);
        assert!(receipt.status);
        assert_eq!(receipt.to, Some(Address::from([1u8; 20])));
    }

    #[test]
    fn test_gas_cost_calculation() {
        let tx = EvmTransaction::transfer(
            Address::ZERO,
            Address::from([1u8; 20]),
            U256::from(1000),
            1,
            U256::from(20_000_000_000u64), // 20 gwei
        );

        let total_cost = tx.total_gas_cost();
        let expected = U256::from(21000) * U256::from(20_000_000_000u64);
        assert_eq!(total_cost, expected);
    }
}