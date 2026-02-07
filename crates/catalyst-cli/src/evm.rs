use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EvmTxKind {
    Deploy { bytecode: Vec<u8> },
    Call { to: [u8; 20], input: Vec<u8> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvmTxPayload {
    pub nonce: u64,
    pub kind: EvmTxKind,
}

pub const EVM_MARKER_PREFIX: &[u8] = b"EVM1";

pub fn encode_evm_marker(payload: &EvmTxPayload, sig64: &[u8]) -> Result<Vec<u8>, String> {
    if sig64.len() != 64 {
        return Err("sig must be 64 bytes".to_string());
    }
    let p = bincode::serialize(payload).map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(EVM_MARKER_PREFIX.len() + 4 + p.len() + 64);
    out.extend_from_slice(EVM_MARKER_PREFIX);
    out.extend_from_slice(&(p.len() as u32).to_le_bytes());
    out.extend_from_slice(&p);
    out.extend_from_slice(sig64);
    Ok(out)
}

pub fn decode_evm_marker(sig: &[u8]) -> Option<(EvmTxPayload, [u8; 64])> {
    if sig.len() < EVM_MARKER_PREFIX.len() + 4 + 64 {
        return None;
    }
    if !sig.starts_with(EVM_MARKER_PREFIX) {
        return None;
    }
    let mut lenb = [0u8; 4];
    lenb.copy_from_slice(&sig[EVM_MARKER_PREFIX.len()..EVM_MARKER_PREFIX.len() + 4]);
    let plen = u32::from_le_bytes(lenb) as usize;
    let start = EVM_MARKER_PREFIX.len() + 4;
    let end = start + plen;
    if sig.len() != end + 64 {
        return None;
    }
    let payload = bincode::deserialize::<EvmTxPayload>(&sig[start..end]).ok()?;
    let mut sig64 = [0u8; 64];
    sig64.copy_from_slice(&sig[end..end + 64]);
    Some((payload, sig64))
}

