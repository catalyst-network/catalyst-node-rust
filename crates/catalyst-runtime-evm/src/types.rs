// types.rs - Re-export from lib.rs

pub use crate::{
    EvmTransaction, EvmExecutionResult, EvmLog, 
    CrossRuntimeCall, DfsOperation, ConfidentialProofs
};

// Stub types for compatibility
use revm::primitives::{Address, U256};
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvmCall {
    pub from: Address,
    pub to: Address,
    pub value: Option<U256>,
    pub data: Option<Vec<u8>>,
    pub gas_limit: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractDeployment {
    pub from: Address,
    pub bytecode: Vec<u8>,
    pub value: Option<U256>,
    pub gas_limit: u64,
    pub gas_price: U256,
    pub gas_priority_fee: Option<U256>,
    pub nonce: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvmCallResult {
    pub success: bool,
    pub return_data: Vec<u8>,
    pub gas_used: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractDeploymentResult {
    pub success: bool,
    pub contract_address: Option<Address>,
    pub return_data: Vec<u8>,
    pub gas_used: u64,
    pub gas_refunded: u64,
}

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
    AccountCreated {
        balance: U256,
    },
    AccountDeleted,
}