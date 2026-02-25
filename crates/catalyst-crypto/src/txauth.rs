use crate::{CryptoError, CryptoResult, PublicKey, Signature, SignatureScheme};

use catalyst_core::protocol::{sig_scheme, Transaction, TransactionType};

#[derive(Debug, thiserror::Error)]
pub enum TxAuthError {
    #[error("unsupported signature scheme id: {0}")]
    UnsupportedScheme(u8),
    #[error("invalid sender (cannot determine sender pubkey)")]
    InvalidSender,
    #[error("sender_pubkey blob not allowed for this scheme")]
    SenderPubkeyNotAllowed,
    #[error("invalid signature bytes")]
    InvalidSignatureBytes,
    #[error("invalid public key bytes")]
    InvalidPublicKeyBytes,
    #[error("invalid signature")]
    InvalidSignature,
    #[error("unable to build signing payload")]
    SigningPayload,
}

/// Determine the 32-byte sender pubkey for current transaction semantics.
///
/// This matches the node/RPC rules:
/// - transfers: the (single) pubkey with negative NonConfidential amount
/// - worker registration: entry[0].public_key
/// - smart contract: entry[0].public_key
pub fn tx_sender_pubkey_32(tx: &Transaction) -> Option<[u8; 32]> {
    match tx.core.tx_type {
        TransactionType::WorkerRegistration => tx.core.entries.get(0).map(|e| e.public_key),
        TransactionType::SmartContract => tx.core.entries.get(0).map(|e| e.public_key),
        _ => {
            let mut sender: Option<[u8; 32]> = None;
            for e in &tx.core.entries {
                if let catalyst_core::protocol::EntryAmount::NonConfidential(v) = e.amount {
                    if v < 0 {
                        match sender {
                            None => sender = Some(e.public_key),
                            Some(pk) if pk == e.public_key => {}
                            Some(_) => return None, // multi-sender not supported
                        }
                    }
                }
            }
            sender
        }
    }
}

/// Verify a transaction signature using explicit chain domain separation.
///
/// This performs scheme dispatch based on `tx.signature_scheme`.
/// Today only Schnorr (`sig_scheme::SCHNORR_V1` == 0) is supported, but this module is the
/// intended extension point for future PQ and/or ECDSA schemes.
pub fn verify_tx_signature_with_domain(
    tx: &Transaction,
    chain_id: u64,
    genesis_hash: [u8; 32],
) -> Result<(), TxAuthError> {
    match tx.signature_scheme {
        sig_scheme::SCHNORR_V1 => verify_schnorr(tx, chain_id, genesis_hash),
        other => Err(TxAuthError::UnsupportedScheme(other)),
    }
}

fn verify_schnorr(tx: &Transaction, chain_id: u64, genesis_hash: [u8; 32]) -> Result<(), TxAuthError> {
    if tx.sender_pubkey.is_some() {
        return Err(TxAuthError::SenderPubkeyNotAllowed);
    }

    if tx.signature.0.len() != 64 {
        return Err(TxAuthError::InvalidSignatureBytes);
    }
    let mut sig_bytes = [0u8; 64];
    sig_bytes.copy_from_slice(&tx.signature.0);
    let sig = Signature::from_bytes(sig_bytes).map_err(|_| TxAuthError::InvalidSignatureBytes)?;

    let sender_pk_bytes = tx_sender_pubkey_32(tx).ok_or(TxAuthError::InvalidSender)?;
    let sender_pk = PublicKey::from_bytes(sender_pk_bytes).map_err(|_| TxAuthError::InvalidPublicKeyBytes)?;

    // Prefer v2 domain-separated payload; fall back to v1 + legacy.
    let payload = tx
        .signing_payload_v2(chain_id, genesis_hash)
        .or_else(|_| tx.signing_payload_v1(chain_id, genesis_hash))
        .or_else(|_| tx.signing_payload())
        .map_err(|_| TxAuthError::SigningPayload)?;

    let ok = SignatureScheme::new()
        .verify(&payload, &sig, &sender_pk)
        .map_err(|_| TxAuthError::InvalidSignature)?;
    if !ok {
        return Err(TxAuthError::InvalidSignature);
    }
    Ok(())
}

/// Adapter for callers that prefer `CryptoResult`.
pub fn verify_tx_signature_with_domain_crypto(
    tx: &Transaction,
    chain_id: u64,
    genesis_hash: [u8; 32],
) -> CryptoResult<()> {
    verify_tx_signature_with_domain(tx, chain_id, genesis_hash).map_err(|e| CryptoError::InvalidSignature(e.to_string()))
}

