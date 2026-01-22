use alloy_primitives::{Address, B256, Bytes, U256};
use catalyst_storage::{RocksEngine, StorageManager};
use catalyst_utils::state::StateManager;
use revm::{
    bytecode::Bytecode,
    context::TxEnv,
    context::ContextTr,
    database::CacheDB,
    database_interface::{DBErrorMarker, DatabaseRef},
    primitives::{TxKind, KECCAK_EMPTY},
    state::{AccountInfo, EvmState},
    Context, ExecuteEvm, MainBuilder, MainContext,
};

#[derive(Debug, thiserror::Error, Clone)]
pub enum EvmExecError {
    #[error("db error: {0}")]
    Db(String),
    #[error("revm error: {0}")]
    Revm(String),
    #[error("invalid input: {0}")]
    Invalid(String),
}

impl DBErrorMarker for EvmExecError {}

fn key_evm_code(addr: &Address) -> Vec<u8> {
    let mut k = b"evm:code:".to_vec();
    k.extend_from_slice(addr.as_slice());
    k
}

fn key_evm_nonce(addr: &Address) -> Vec<u8> {
    let mut k = b"evm:nonce:".to_vec();
    k.extend_from_slice(addr.as_slice());
    k
}

fn key_evm_balance(addr: &Address) -> Vec<u8> {
    let mut k = b"evm:balance:".to_vec();
    k.extend_from_slice(addr.as_slice());
    k
}

fn key_evm_storage(addr: &Address, slot: &U256) -> Vec<u8> {
    let mut k = b"evm:storage:".to_vec();
    k.extend_from_slice(addr.as_slice());
    let mut slotb = [0u8; 32];
    slotb.copy_from_slice(&slot.to_be_bytes::<32>());
    k.extend_from_slice(&slotb);
    k
}

fn key_evm_last_return(addr: &Address) -> Vec<u8> {
    let mut k = b"evm:last_return:".to_vec();
    k.extend_from_slice(addr.as_slice());
    k
}

fn key_evm_last_logs(addr: &Address) -> Vec<u8> {
    let mut k = b"evm:last_logs:".to_vec();
    k.extend_from_slice(addr.as_slice());
    k
}

fn decode_u64_le(b: &[u8]) -> Option<u64> {
    if b.len() != 8 {
        return None;
    }
    let mut arr = [0u8; 8];
    arr.copy_from_slice(b);
    Some(u64::from_le_bytes(arr))
}

fn encode_u64_le(v: u64) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn decode_u256_be(b: &[u8]) -> Option<U256> {
    if b.len() != 32 {
        return None;
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(b);
    Some(U256::from_be_bytes(arr))
}

fn encode_u256_be(v: U256) -> Vec<u8> {
    v.to_be_bytes::<32>().to_vec()
}

#[derive(Clone)]
pub struct CatalystExternalDb {
    engine: std::sync::Arc<RocksEngine>,
}

impl CatalystExternalDb {
    pub fn new(engine: std::sync::Arc<RocksEngine>) -> Self {
        Self { engine }
    }

    fn get_cf(&self, key: &[u8]) -> Result<Option<Vec<u8>>, EvmExecError> {
        self.engine
            .get("accounts", key)
            .map_err(|e| EvmExecError::Db(e.to_string()))
    }
}

impl DatabaseRef for CatalystExternalDb {
    type Error = EvmExecError;

    fn basic_ref(&self, address: Address) -> Result<Option<AccountInfo>, Self::Error> {
        let balance = self
            .get_cf(&key_evm_balance(&address))?
            .as_deref()
            .and_then(decode_u256_be)
            .unwrap_or(U256::ZERO);
        let nonce = self
            .get_cf(&key_evm_nonce(&address))?
            .as_deref()
            .and_then(decode_u64_le)
            .unwrap_or(0);
        let code_bytes = self
            .get_cf(&key_evm_code(&address))?
            .unwrap_or_default();
        let code = if code_bytes.is_empty() {
            Bytecode::default()
        } else {
            Bytecode::new_raw(Bytes::from(code_bytes))
        };
        let code_hash = if code.is_empty() { KECCAK_EMPTY } else { code.hash_slow() };

        if balance == U256::ZERO && nonce == 0 && code.is_empty() {
            return Ok(None);
        }

        Ok(Some(AccountInfo {
            balance,
            nonce,
            code_hash,
            code: Some(code),
        }))
    }

    fn code_by_hash_ref(&self, _code_hash: B256) -> Result<Bytecode, Self::Error> {
        // We always return code inline in `basic_ref`, so this should not be needed.
        Ok(Bytecode::default())
    }

    fn storage_ref(&self, address: Address, index: U256) -> Result<U256, Self::Error> {
        let key = key_evm_storage(&address, &index);
        Ok(self
            .get_cf(&key)?
            .as_deref()
            .and_then(decode_u256_be)
            .unwrap_or(U256::ZERO))
    }

    fn block_hash_ref(&self, _number: u64) -> Result<B256, Self::Error> {
        Ok(B256::ZERO)
    }
}

pub struct EvmPersisted {
    pub touched_keys: Vec<Vec<u8>>,
}

fn persist_state(store: &StorageManager, state: EvmState) -> impl std::future::Future<Output = Result<Vec<Vec<u8>>, EvmExecError>> + '_ {
    async move {
        let mut touched: Vec<Vec<u8>> = Vec::new();
        for (addr, acc) in state {
            let a = Address::from(addr);
            let kb = key_evm_balance(&a);
            let kn = key_evm_nonce(&a);
            store
                .set_state(&kb, encode_u256_be(acc.info.balance))
                .await
                .map_err(|e| EvmExecError::Db(e.to_string()))?;
            store
                .set_state(&kn, encode_u64_le(acc.info.nonce))
                .await
                .map_err(|e| EvmExecError::Db(e.to_string()))?;
            touched.push(kb);
            touched.push(kn);

            if let Some(code) = acc.info.code {
                if !code.is_empty() {
                    let kc = key_evm_code(&a);
                    store
                        .set_state(&kc, code.original_byte_slice().to_vec())
                        .await
                        .map_err(|e| EvmExecError::Db(e.to_string()))?;
                    touched.push(kc);
                }
            }

            for (slot, slotval) in acc.storage {
                if !slotval.is_changed() {
                    continue;
                }
                let kv = key_evm_storage(&a, &slot);
                store
                    .set_state(&kv, encode_u256_be(slotval.present_value))
                    .await
                    .map_err(|e| EvmExecError::Db(e.to_string()))?;
                touched.push(kv);
            }
        }
        Ok(touched)
    }
}

/// Execute a CREATE (deployment) using REVM and persist resulting state into Catalyst storage.
///
/// - `init_code` is the EVM init code (constructor code)
/// - `protocol_nonce` is Catalyst protocol nonce (starts at 1); EVM nonce = protocol_nonce - 1
pub async fn execute_deploy_and_persist(
    store: &StorageManager,
    from: Address,
    protocol_nonce: u64,
    init_code: Vec<u8>,
    gas_limit: u64,
) -> Result<(Address, Bytes, EvmPersisted), EvmExecError> {
    if protocol_nonce == 0 {
        return Err(EvmExecError::Invalid("protocol nonce must be >= 1".to_string()));
    }
    let evm_nonce = protocol_nonce - 1;

    let (out, runtime_code) = {
        let ext = CatalystExternalDb::new(store.engine().clone());
        let db = CacheDB::new(ext);
        let mut evm = Context::mainnet()
            .modify_cfg_chained(|cfg| {
                cfg.disable_nonce_check = false;
            })
            .with_db(db)
            .build_mainnet();

        let tx = TxEnv::builder()
            .tx_type(Some(0)) // legacy
            .caller(from)
            .nonce(evm_nonce)
            .gas_limit(gas_limit)
            .gas_price(0u128)
            .kind(TxKind::Create)
            .data(Bytes::from(init_code))
            .chain_id(Some(31337))
            .build_fill();

        let out = evm
            .transact(tx)
            .map_err(|e| EvmExecError::Revm(format!("{e:?}")))?;

        // Try to extract the deployed runtime bytecode from the CacheDB contract map.
        let created = out
            .result
            .created_address()
            .unwrap_or_else(|| catalyst_runtime_evm::utils::calculate_ethereum_create_address(&from, evm_nonce));
        let runtime = out
            .state
            .get(&created)
            .and_then(|acc| {
                let h = acc.info.code_hash;
                evm.ctx
                    .journal_mut()
                    .database
                    .cache
                    .contracts
                    .get(&h)
                    .map(|bc: &Bytecode| bc.original_byte_slice().to_vec())
            });

        (out, runtime)
    };

    let created = out
        .result
        .created_address()
        .unwrap_or_else(|| catalyst_runtime_evm::utils::calculate_ethereum_create_address(&from, evm_nonce));
    let return_data = out.result.output().cloned().unwrap_or_else(Bytes::new);

    let mut touched = persist_state(store, out.state).await?;

    // Ensure runtime code is persisted in our address->code mapping so RPC `getCode` works and future CALLs can execute.
    if let Some(code) = runtime_code {
        let kc = key_evm_code(&created);
        store
            .set_state(&kc, code)
            .await
            .map_err(|e| EvmExecError::Db(e.to_string()))?;
        touched.push(kc);
    }

    Ok((created, return_data, EvmPersisted { touched_keys: touched }))
}

/// Execute a CALL and persist resulting state + return data.
pub async fn execute_call_and_persist(
    store: &StorageManager,
    from: Address,
    to: Address,
    protocol_nonce: u64,
    input: Vec<u8>,
    gas_limit: u64,
) -> Result<(Bytes, EvmPersisted), EvmExecError> {
    if protocol_nonce == 0 {
        return Err(EvmExecError::Invalid("protocol nonce must be >= 1".to_string()));
    }
    let evm_nonce = protocol_nonce - 1;

    let out = {
        let ext = CatalystExternalDb::new(store.engine().clone());
        let db = CacheDB::new(ext);
        let mut evm = Context::mainnet()
            .modify_cfg_chained(|cfg| {
                cfg.disable_nonce_check = false;
            })
            .with_db(db)
            .build_mainnet();

        let tx = TxEnv::builder()
            .tx_type(Some(0)) // legacy
            .caller(from)
            .nonce(evm_nonce)
            .gas_limit(gas_limit)
            .gas_price(0u128)
            .kind(TxKind::Call(to))
            .data(Bytes::from(input))
            .chain_id(Some(31337))
            .build_fill();

        evm.transact(tx)
            .map_err(|e| EvmExecError::Revm(format!("{e:?}")))?
    };
    let ret = out.result.output().cloned().unwrap_or_else(Bytes::new);
    let logs_ser = bincode::serialize(out.result.logs()).unwrap_or_default();

    let mut touched = persist_state(store, out.state).await?;
    let kr = key_evm_last_return(&to);
    store
        .set_state(&kr, ret.to_vec())
        .await
        .map_err(|e| EvmExecError::Db(e.to_string()))?;
    touched.push(kr);
    let kl = key_evm_last_logs(&to);
    store
        .set_state(&kl, logs_ser)
        .await
        .map_err(|e| EvmExecError::Db(e.to_string()))?;
    touched.push(kl);

    Ok((ret, EvmPersisted { touched_keys: touched }))
}

