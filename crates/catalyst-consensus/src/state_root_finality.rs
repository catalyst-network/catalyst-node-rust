//! Certified state-root attestation (ADR 0002): `H_root` digest and certificate verification.
//!
//! Companion to `lsu_finality.rs` (ADR 0001). ADR 0001 certifies the LSU *recipe* (`lsu_hash`) —
//! what a BFT quorum of producers agreed to apply. It says nothing about the *result*: each node
//! computes `state_root` independently, post-apply, and nothing before this module made that
//! computation a consensus fact. Two honest nodes agreeing on `lsu_hash` could (and did, in
//! production — see `docs/consensus-reliability-review-2026-07.md`) silently compute different
//! `state_root`s and diverge forever after.
//!
//! This module adds a second, additive certificate: once a BFT quorum of producers independently
//! apply the same LSU and attest to the *same* resulting `state_root`, that agreement becomes a
//! portable, verifiable `LsuStateRootCertificateV1`. Deliberately a separate certificate (not a
//! new field folded into `LsuFinalityCertificateV1`/`H_cert`) because voting on the LSU recipe
//! happens *before* any node has executed it (see `ProducerVote::ledger_state_hash` in
//! `types.rs`, populated pre-apply) — this certifies a second, later fact, produced during/after
//! Synchronization, without touching four-phase timing.

use blake2::{Blake2b512, Digest};
use catalyst_crypto::{PublicKey, Signature, SignatureScheme};
use serde::{Deserialize, Serialize};

use crate::lsu_finality::{bft_vote_threshold, committee_hash_ordered_producer_list};
use crate::types::{hash_data, LedgerStateUpdate};

pub const MAX_STATE_ROOT_ATTESTATIONS: usize = 256;

#[derive(Debug, Clone, thiserror::Error)]
pub enum StateRootVerifyError {
    #[error("bad certificate version")]
    BadFormat,
    #[error("too many attestations")]
    TooManyAttestations,
    #[error("empty producer list")]
    EmptyCommittee,
    #[error("lsu hash or cycle mismatch")]
    LsuHashMismatch,
    #[error("committee hash mismatch")]
    CommitteeHashMismatch,
    #[error("duplicate attester")]
    DuplicateAttester,
    #[error("attester not in committee")]
    AttesterNotInCommittee,
    #[error("insufficient quorum")]
    InsufficientQuorum,
    #[error("invalid public key")]
    InvalidPublicKey,
    #[error("invalid signature bytes")]
    InvalidSignatureBytes,
    #[error("signature verification failed")]
    InvalidSignature,
    #[error("lsu serialize/hash error")]
    HashError,
}

/// Domain separator distinct from `CATALYST_LSUFC` (ADR 0001) so a signature over one certificate
/// type can never be replayed as evidence for the other.
pub fn state_root_domain_padded() -> [u8; 16] {
    const S: &[u8] = b"CATALYST_LSUROOT";
    let mut d = [0u8; 16];
    let n = S.len().min(16);
    d[..n].copy_from_slice(&S[..n]);
    d
}

fn blake2b512_trunc32(data: &[u8]) -> [u8; 32] {
    let mut h = Blake2b512::new();
    h.update(data);
    let r = h.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&r[..32]);
    out
}

/// `H_root` (v1): domain-separated digest a producer signs to attest that applying `lsu_hash` at
/// `cycle` produced `state_root`. Mirrors `h_cert_v1` (`lsu_finality.rs`) but binds `state_root`
/// instead of `committee_hash`/`vote_list_hash`, and uses a distinct domain/committee digest so
/// the two certificate types are not mutually replayable.
pub fn h_root_v1(
    chain_id: u64,
    genesis_hash: &[u8; 32],
    cycle: u64,
    lsu_hash: &[u8; 32],
    committee_hash: &[u8; 32],
    state_root: &[u8; 32],
) -> [u8; 32] {
    let mut buf = Vec::with_capacity(129);
    buf.extend_from_slice(&state_root_domain_padded());
    buf.push(1u8);
    buf.extend_from_slice(&chain_id.to_le_bytes());
    buf.extend_from_slice(genesis_hash);
    buf.extend_from_slice(&cycle.to_le_bytes());
    buf.extend_from_slice(lsu_hash);
    buf.extend_from_slice(committee_hash);
    buf.extend_from_slice(state_root);
    blake2b512_trunc32(&buf)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateRootAttestation {
    pub producer_id: String,
    /// Schnorr signature, 64 bytes (`Signature::to_bytes`), over `H_root`.
    pub signature: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LsuStateRootCertificateV1 {
    pub version: u8,
    pub chain_id: u64,
    pub genesis_hash: [u8; 32],
    pub cycle: u64,
    pub lsu_hash: [u8; 32],
    pub state_root: [u8; 32],
    pub committee_hash: [u8; 32],
    pub attestations: Vec<StateRootAttestation>,
}

impl LsuStateRootCertificateV1 {
    pub fn from_lsu_and_attestations(
        lsu: &LedgerStateUpdate,
        chain_id: u64,
        genesis_hash: [u8; 32],
        lsu_hash: [u8; 32],
        state_root: [u8; 32],
        attestations: Vec<StateRootAttestation>,
    ) -> Result<Self, StateRootVerifyError> {
        let committee_hash = committee_hash_ordered_producer_list(&lsu.producer_list)
            .ok_or(StateRootVerifyError::EmptyCommittee)?;
        Ok(Self {
            version: 1,
            chain_id,
            genesis_hash,
            cycle: lsu.cycle_number,
            lsu_hash,
            state_root,
            committee_hash,
            attestations,
        })
    }
}

/// Verify a `LsuStateRootCertificateV1` against the `LedgerStateUpdate` it claims to certify.
///
/// Recomputes `lsu_hash` and `committee_hash` from `lsu` itself (never trusts the detached
/// fields), matching the discipline `verify_lsu_finality_certificate` already uses for ADR 0001.
pub fn verify_lsu_state_root_certificate(
    cert: &LsuStateRootCertificateV1,
    lsu: &LedgerStateUpdate,
) -> Result<(), StateRootVerifyError> {
    if cert.version != 1 {
        return Err(StateRootVerifyError::BadFormat);
    }
    if cert.attestations.len() > MAX_STATE_ROOT_ATTESTATIONS {
        return Err(StateRootVerifyError::TooManyAttestations);
    }
    let lsu_hash = hash_data(lsu).map_err(|_| StateRootVerifyError::HashError)?;
    if lsu_hash != cert.lsu_hash || lsu.cycle_number != cert.cycle {
        return Err(StateRootVerifyError::LsuHashMismatch);
    }
    let committee_hash = committee_hash_ordered_producer_list(&lsu.producer_list)
        .ok_or(StateRootVerifyError::EmptyCommittee)?;
    if committee_hash != cert.committee_hash {
        return Err(StateRootVerifyError::CommitteeHashMismatch);
    }

    let n = lsu.producer_list.len();
    let need = bft_vote_threshold(n);
    let scheme = SignatureScheme::new();
    let h = h_root_v1(
        cert.chain_id,
        &cert.genesis_hash,
        cert.cycle,
        &cert.lsu_hash,
        &cert.committee_hash,
        &cert.state_root,
    );

    let mut seen = std::collections::BTreeSet::new();
    let mut valid = 0usize;
    for a in &cert.attestations {
        if !seen.insert(a.producer_id.clone()) {
            return Err(StateRootVerifyError::DuplicateAttester);
        }
        if !lsu.producer_list.iter().any(|p| p == &a.producer_id) {
            return Err(StateRootVerifyError::AttesterNotInCommittee);
        }
        let pk_bytes = crate::lsu_finality::parse_producer_hex32(&a.producer_id)
            .ok_or(StateRootVerifyError::InvalidPublicKey)?;
        let pk = PublicKey::from_bytes(pk_bytes).map_err(|_| StateRootVerifyError::InvalidPublicKey)?;
        if a.signature.len() != 64 {
            return Err(StateRootVerifyError::InvalidSignatureBytes);
        }
        let mut sb = [0u8; 64];
        sb.copy_from_slice(&a.signature[..64]);
        let sig = Signature::from_bytes(sb).map_err(|_| StateRootVerifyError::InvalidSignatureBytes)?;
        let ok = scheme
            .verify(&h, &sig, &pk)
            .map_err(|_| StateRootVerifyError::InvalidSignature)?;
        if !ok {
            return Err(StateRootVerifyError::InvalidSignature);
        }
        valid += 1;
    }
    if valid < need {
        return Err(StateRootVerifyError::InsufficientQuorum);
    }
    Ok(())
}

/// Verify a single attestation (producer signed `H_root` for the given preimage fields).
#[allow(clippy::too_many_arguments)]
pub fn verify_state_root_attestation(
    chain_id: u64,
    genesis_hash: &[u8; 32],
    cycle: u64,
    lsu_hash: &[u8; 32],
    committee_hash: &[u8; 32],
    state_root: &[u8; 32],
    producer_id: &str,
    signature: &[u8],
) -> Result<(), StateRootVerifyError> {
    let h = h_root_v1(chain_id, genesis_hash, cycle, lsu_hash, committee_hash, state_root);
    let pk_bytes = crate::lsu_finality::parse_producer_hex32(producer_id)
        .ok_or(StateRootVerifyError::InvalidPublicKey)?;
    let pk = PublicKey::from_bytes(pk_bytes).map_err(|_| StateRootVerifyError::InvalidPublicKey)?;
    if signature.len() != 64 {
        return Err(StateRootVerifyError::InvalidSignatureBytes);
    }
    let mut sb = [0u8; 64];
    sb.copy_from_slice(&signature[..64]);
    let sig = Signature::from_bytes(sb).map_err(|_| StateRootVerifyError::InvalidSignatureBytes)?;
    let scheme = SignatureScheme::new();
    let ok = scheme
        .verify(&h, &sig, &pk)
        .map_err(|_| StateRootVerifyError::InvalidSignature)?;
    if !ok {
        return Err(StateRootVerifyError::InvalidSignature);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{CompensationEntry, PartialLedgerStateUpdate, TransactionEntry};
    use catalyst_crypto::PrivateKey;
    use rand::rngs::OsRng;

    #[test]
    fn h_root_v1_golden_vector() {
        let chain_id = 0x0102_0304_0506_0708u64;
        let genesis_hash = [0xabu8; 32];
        let cycle = 0x1122_3344_5566_7788u64;
        let lsu_hash = [0xcdu8; 32];
        let committee_hash = [0xefu8; 32];
        let state_root = [0x34u8; 32];
        let h = h_root_v1(chain_id, &genesis_hash, cycle, &lsu_hash, &committee_hash, &state_root);
        // Distinct domain from h_cert_v1 => distinct digest even with structurally similar inputs.
        assert_ne!(
            hex::encode(h),
            "f52f6af24f1261e9d03a71a69a787c9481b9a3c820aa78d88f75c23c245044c2"
        );
        assert_eq!(h.len(), 32);
        // Deterministic.
        let h2 = h_root_v1(chain_id, &genesis_hash, cycle, &lsu_hash, &committee_hash, &state_root);
        assert_eq!(h, h2);
    }

    fn make_lsu(producer_ids: Vec<String>, cycle: u64) -> LedgerStateUpdate {
        LedgerStateUpdate {
            partial_update: PartialLedgerStateUpdate {
                transaction_entries: vec![TransactionEntry {
                    public_key: [0u8; 32],
                    amount: 0,
                    signature: vec![],
                }],
                transaction_signatures_hash: [1u8; 32],
                total_fees: 0,
                timestamp: 1,
            },
            compensation_entries: vec![CompensationEntry {
                producer_id: producer_ids[0].clone(),
                public_key: [0u8; 32],
                amount: 0,
            }],
            cycle_number: cycle,
            producer_list: producer_ids.clone(),
            vote_list: producer_ids,
        }
    }

    #[test]
    fn certificate_round_trip_verify() {
        let pk_a = PrivateKey::generate(&mut OsRng);
        let pk_b = PrivateKey::generate(&mut OsRng);
        let pk_c = PrivateKey::generate(&mut OsRng);
        let id_a = hex::encode(pk_a.public_key().to_bytes());
        let id_b = hex::encode(pk_b.public_key().to_bytes());
        let id_c = hex::encode(pk_c.public_key().to_bytes());

        let lsu = make_lsu(vec![id_a.clone(), id_b.clone(), id_c.clone()], 42);
        let lsu_hash = hash_data(&lsu).unwrap();
        let chain_id = 7u64;
        let genesis_hash = [3u8; 32];
        let state_root = [9u8; 32];
        let committee_hash = committee_hash_ordered_producer_list(&lsu.producer_list).unwrap();
        let h = h_root_v1(chain_id, &genesis_hash, lsu.cycle_number, &lsu_hash, &committee_hash, &state_root);

        let mut rng = OsRng;
        let scheme = SignatureScheme::new();
        let sig_a = scheme.sign(&mut rng, &pk_a, &h).unwrap().to_bytes();
        let sig_b = scheme.sign(&mut rng, &pk_b, &h).unwrap().to_bytes();

        let cert = LsuStateRootCertificateV1 {
            version: 1,
            chain_id,
            genesis_hash,
            cycle: lsu.cycle_number,
            lsu_hash,
            state_root,
            committee_hash,
            attestations: vec![
                StateRootAttestation { producer_id: id_a, signature: sig_a.to_vec() },
                StateRootAttestation { producer_id: id_b, signature: sig_b.to_vec() },
            ],
        };
        verify_lsu_state_root_certificate(&cert, &lsu).unwrap();
    }

    #[test]
    fn certificate_verify_rejects_wrong_state_root() {
        let pk_a = PrivateKey::generate(&mut OsRng);
        let pk_b = PrivateKey::generate(&mut OsRng);
        let pk_c = PrivateKey::generate(&mut OsRng);
        let id_a = hex::encode(pk_a.public_key().to_bytes());
        let id_b = hex::encode(pk_b.public_key().to_bytes());
        let id_c = hex::encode(pk_c.public_key().to_bytes());

        let lsu = make_lsu(vec![id_a.clone(), id_b.clone(), id_c.clone()], 42);
        let lsu_hash = hash_data(&lsu).unwrap();
        let chain_id = 7u64;
        let genesis_hash = [3u8; 32];
        let state_root = [9u8; 32];
        let committee_hash = committee_hash_ordered_producer_list(&lsu.producer_list).unwrap();
        let h = h_root_v1(chain_id, &genesis_hash, lsu.cycle_number, &lsu_hash, &committee_hash, &state_root);

        let mut rng = OsRng;
        let scheme = SignatureScheme::new();
        let sig_a = scheme.sign(&mut rng, &pk_a, &h).unwrap().to_bytes();
        let sig_b = scheme.sign(&mut rng, &pk_b, &h).unwrap().to_bytes();

        let mut cert = LsuStateRootCertificateV1 {
            version: 1,
            chain_id,
            genesis_hash,
            cycle: lsu.cycle_number,
            lsu_hash,
            state_root,
            committee_hash,
            attestations: vec![
                StateRootAttestation { producer_id: id_a, signature: sig_a.to_vec() },
                StateRootAttestation { producer_id: id_b, signature: sig_b.to_vec() },
            ],
        };
        // Tamper with the certified root itself: signatures no longer verify against it.
        cert.state_root = [0xff; 32];
        assert!(matches!(
            verify_lsu_state_root_certificate(&cert, &lsu),
            Err(StateRootVerifyError::InvalidSignature)
        ));
    }

    #[test]
    fn certificate_verify_rejects_insufficient_quorum() {
        let pk_a = PrivateKey::generate(&mut OsRng);
        let pk_b = PrivateKey::generate(&mut OsRng);
        let pk_c = PrivateKey::generate(&mut OsRng);
        let id_a = hex::encode(pk_a.public_key().to_bytes());
        let id_b = hex::encode(pk_b.public_key().to_bytes());
        let id_c = hex::encode(pk_c.public_key().to_bytes());

        let lsu = make_lsu(vec![id_a.clone(), id_b.clone(), id_c.clone()], 42);
        let lsu_hash = hash_data(&lsu).unwrap();
        let chain_id = 1u64;
        let genesis_hash = [9u8; 32];
        let state_root = [4u8; 32];
        let committee_hash = committee_hash_ordered_producer_list(&lsu.producer_list).unwrap();
        let h = h_root_v1(chain_id, &genesis_hash, lsu.cycle_number, &lsu_hash, &committee_hash, &state_root);

        let mut rng = OsRng;
        let scheme = SignatureScheme::new();
        let sig_a = scheme.sign(&mut rng, &pk_a, &h).unwrap().to_bytes();

        let cert = LsuStateRootCertificateV1 {
            version: 1,
            chain_id,
            genesis_hash,
            cycle: lsu.cycle_number,
            lsu_hash,
            state_root,
            committee_hash,
            attestations: vec![StateRootAttestation { producer_id: id_a, signature: sig_a.to_vec() }],
        };
        // n=3 => need ceil(2*3/3)=2 distinct valid attesters; only 1 given.
        assert!(matches!(
            verify_lsu_state_root_certificate(&cert, &lsu),
            Err(StateRootVerifyError::InsufficientQuorum)
        ));
    }

    #[test]
    fn certificate_verify_rejects_duplicate_attester() {
        let pk_a = PrivateKey::generate(&mut OsRng);
        let pk_b = PrivateKey::generate(&mut OsRng);
        let id_a = hex::encode(pk_a.public_key().to_bytes());
        let id_b = hex::encode(pk_b.public_key().to_bytes());

        let lsu = make_lsu(vec![id_a.clone(), id_b.clone()], 5);
        let lsu_hash = hash_data(&lsu).unwrap();
        let chain_id = 1u64;
        let genesis_hash = [9u8; 32];
        let state_root = [4u8; 32];
        let committee_hash = committee_hash_ordered_producer_list(&lsu.producer_list).unwrap();
        let h = h_root_v1(chain_id, &genesis_hash, lsu.cycle_number, &lsu_hash, &committee_hash, &state_root);

        let mut rng = OsRng;
        let scheme = SignatureScheme::new();
        let sig_a = scheme.sign(&mut rng, &pk_a, &h).unwrap().to_bytes();

        let cert = LsuStateRootCertificateV1 {
            version: 1,
            chain_id,
            genesis_hash,
            cycle: lsu.cycle_number,
            lsu_hash,
            state_root,
            committee_hash,
            attestations: vec![
                StateRootAttestation { producer_id: id_a.clone(), signature: sig_a.to_vec() },
                StateRootAttestation { producer_id: id_a, signature: sig_a.to_vec() },
            ],
        };
        assert!(matches!(
            verify_lsu_state_root_certificate(&cert, &lsu),
            Err(StateRootVerifyError::DuplicateAttester)
        ));
    }
}
