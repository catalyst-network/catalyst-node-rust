use ethereum_types::{Address, U256, H256};
use revm::primitives::{TxEnv, TransactTo, AccessListItem, ExecutionResult as RevmResult, Output, Log};
use serde::{Deserialize, Serialize};

/// EVM transaction representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvmTransaction {
    pub from: Address,
    pub to: Option<Address>,
    pub value: U256,
    pub data: Vec<u8>,
    pub gas_limit: u64,
    pub gas_price: U256,
    pub gas_priority_fee: Option<U256>,
    pub nonce: u64,
    pub chain_id: u64,
    pub access_list: Option<Vec<AccessListItem>>,
}

impl EvmTransaction {
    pub fn to_tx_env(&self) -> Result<TxEnv, crate::EvmError> {
        Ok(TxEnv {
            caller: self.from,
            gas_limit: self.gas_limit,
            gas_price: self.gas_price,
            transact_to: match self.to {
                Some(addr) => TransactTo::Call(addr),
                None => TransactTo::Create,
            },
            value: self.value,
            data: self.data.clone().into(),
            gas_priority_fee: self.gas_priority_fee,
            chain_id: Some(self.chain_id),
            nonce: Some(self.nonce),
            access_list: self.access_list.clone().unwrap_or_default(),
        })
    }
}

/// EVM call (read-only operation)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvmCall {
    pub from: Address,
    pub to: Address,
    pub value: Option<U256>,
    pub data: Option<Vec<u8>>,
    pub gas_limit: Option<u64>,
}

/// Contract deployment parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractDeployment {
    pub from: Address,
    pub bytecode: Vec<u8>,
    pub value: Option<U256>,
    pub gas_limit: u64,
    pub gas_price: U256,
    pub gas_priority_fee: Option<U256>,
    pub nonce: u64,
    pub access_list: Option<Vec<AccessListItem>>,
}

/// EVM execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvmExecutionResult {
    pub success: bool,
    pub return_data: Vec<u8>,
    pub gas_used: u64,
    pub gas_refunded: u64,
    pub logs: Vec<EvmLog>,
    pub state_changes: Vec<StateChange>,
    pub created_address: Option<Address>,
}

impl EvmExecutionResult {
    pub fn from_revm_result(result: RevmResult, tx: &EvmTransaction) -> Self {
        let success = result.is_success();
        let (return_data, gas_used, gas_refunded, logs, created_address) = match result {
            RevmResult::Success { output, gas_used, gas_refunded, logs } => {
                let return_data = match output {
                    Output::Call(data) => data.to_vec(),
                    Output::Create(data, addr) => {
                        return Self {
                            success: true,
                            return_data: data.to_vec(),
                            gas_used,
                            gas_refunded,
                            logs: logs.into_iter().map(EvmLog::from).collect(),
                            state_changes: Vec::new(), // TODO: Extract from state changes
                            created_address: addr,
                        };
                    }
                };
                (return_data, gas_used, gas_refunded, logs, None)
            }
            RevmResult::Revert { output, gas_used } => {
                (output.to_vec(), gas_used, 0, Vec::new(), None)
            }
            RevmResult::Halt { reason: _, gas_used } => {
                (Vec::new(), gas_used, 0, Vec::new(), None)
            }
        };

        Self {
            success,
            return_data,
            gas_used,
            gas_refunded,
            logs: logs.into_iter().map(EvmLog::from).collect(),
            state_changes: Vec::new(), // TODO: Extract from state changes
            created_address,
        }
    }
}

/// EVM call result (read-only)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvmCallResult {
    pub success: bool,
    pub return_data: Vec<u8>,
    pub gas_used: u64,
}

impl EvmCallResult {
    pub fn from_revm_result(result: RevmResult) -> Self {
        let success = result.is_success();
        let (return_data, gas_used) = match result {
            RevmResult::Success { output, gas_used, .. } => {
                let data = match output {
                    Output::Call(data) => data.to_vec(),
                    Output::Create(data, _) => data.to_vec(),
                };
                (data, gas_used)
            }
            RevmResult::Revert { output, gas_used } => {
                (output.to_vec(), gas_used)
            }
            RevmResult::Halt { gas_used, .. } => {
                (Vec::new(), gas_used)
            }
        };

        Self {
            success,
            return_data,
            gas_used,
        }
    }
}

/// Contract deployment result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractDeploymentResult {
    pub success: bool,
    pub contract_address: Option<Address>,
    pub return_data: Vec<u8>,
    pub gas_used: u64,
    pub gas_refunded: u64,
}

impl ContractDeploymentResult {
    pub fn from_revm_result(result: RevmResult) -> Self {
        match result {
            RevmResult::Success { output, gas_used, gas_refunded, .. } => {
                match output {
                    Output::Create(data, Some(addr)) => Self {
                        success: true,
                        contract_address: Some(addr),
                        return_data: data.to_vec(),
                        gas_used,
                        gas_refunded,
                    },
                    Output::Create(data, None) => Self {
                        success: false,
                        contract_address: None,
                        return_data: data.to_vec(),
                        gas_used,
                        gas_refunded,
                    },
                    Output::Call(data) => Self {
                        success: false,
                        contract_address: None,
                        return_data: data.to_vec(),
                        gas_used,
                        gas_refunded,
                    },
                }
            }
            RevmResult::Revert { output, gas_used } => Self {
                success: false,
                contract_address: None,
                return_data: output.to_vec(),
                gas_used,
                gas_refunded: 0,
            },
            RevmResult::Halt { gas_used, .. } => Self {
                success: false,
                contract_address: None,
                return_data: Vec::new(),
                gas_used,
                gas_refunded: 0,
            },
        }
    }
}

/// EVM log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvmLog {
    pub address: Address,
    pub topics: Vec<H256>,
    pub data: Vec<u8>,
}

impl From<Log> for EvmLog {
    fn from(log: Log) -> Self {
        Self {
            address: log.address,
            topics: log.topics,
            data: log.data.to_vec(),
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
        old_code_hash: H256,
        new_code_hash: H256,
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

/// Convert EVM log to Catalyst core log format
impl From<EvmLog> for catalyst_core::Event {
    fn from(log: EvmLog) -> Self {
        catalyst_core::Event::Custom {
            event_type: "evm_log".to_string(),
            data: serde_json::to_vec(&log).unwrap_or_default(),
        }
    }
}