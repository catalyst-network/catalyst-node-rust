//! LSU cryptographic finality (ADR 0001): `H_cert` digest and certificate verification.

use blake2::{Blake2b512, Digest};
use catalyst_crypto::{PublicKey, Signature, SignatureScheme};
use serde::{Deserialize, Serialize};

use crate::phases::hash_producer_list_merkle;
use crate::types::{hash_data, LedgerStateUpdate};

pub const FINALITY_CERT_STYLE_INDIVIDUAL: u8 = 1;
pub const MAX_FINALITY_VOTES: usize = 256;

#[derive(Debug, Clone, thiserror::Error)]
pub enum LsuFinalityVerifyError {
    #[error("bad certificate version or style")]
    BadFormat,
    #[error("too many votes")]
    TooManyVotes,
    #[error("empty producer list")]
    EmptyCommittee,
    #[error("lsu hash or cycle mismatch")]
    LsuHashMismatch,
    #[error("committee hash mismatch")]
    CommitteeHashMismatch,
    #[error("vote list hash mismatch")]
    VoteListHashMismatch,
    #[error("duplicate voter")]
    DuplicateVoter,
    #[error("voter not in committee")]
    VoterNotInCommittee,
    #[error("voter not in vote list")]
    VoterNotInVoteList,
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

pub fn finality_domain_padded() -> [u8; 16] {
    const S: &[u8] = b"CATALYST_LSUFC";
    let mut d = [0u8; 16];
    let n = S.len().min(16);
    d[..n].copy_from_slice(&S[..n]);
    d
}

pub fn blake2b512_trunc32(data: &[u8]) -> [u8; 32] {
    let mut h = Blake2b512::new();
    h.update(data);
    let r = h.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&r[..32]);
    out
}

/// 32-byte pubkey from 64-char hex `ProducerId`.
pub fn parse_producer_hex32(s: &str) -> Option<[u8; 32]> {
    if s.len() != 64 {
        return None;
    }
    let v = hex::decode(s).ok()?;
    if v.len() != 32 {
        return None;
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&v);
    Some(out)
}

/// Committee digest: concat 32-byte pubkeys in `producer_list` order.
pub fn committee_hash_ordered_producer_list(producer_list: &[String]) -> Option<[u8; 32]> {
    if producer_list.is_empty() {
        return None;
    }
    let mut raw = Vec::with_capacity(producer_list.len() * 32);
    for id in producer_list {
        raw.extend_from_slice(&parse_producer_hex32(id)?);
    }
    Some(blake2b512_trunc32(&raw))
}

pub fn h_cert_v1(
    chain_id: u64,
    genesis_hash: &[u8; 32],
    cycle: u64,
    lsu_hash: &[u8; 32],
    committee_hash: &[u8; 32],
    vote_list_hash: &[u8; 32],
) -> [u8; 32] {
    let mut buf = Vec::with_capacity(129);
    buf.extend_from_slice(&finality_domain_padded());
    buf.push(1u8);
    buf.extend_from_slice(&chain_id.to_le_bytes());
    buf.extend_from_slice(genesis_hash);
    buf.extend_from_slice(&cycle.to_le_bytes());
    buf.extend_from_slice(lsu_hash);
    buf.extend_from_slice(committee_hash);
    buf.extend_from_slice(vote_list_hash);
    blake2b512_trunc32(&buf)
}

/// BFT quorum ⌈2n/3⌉ for n producers.
pub fn bft_vote_threshold(n: usize) -> usize {
    if n == 0 {
        usize::MAX
    } else {
        (2 * n + 2) / 3
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProducerFinalityVote {
    pub producer_id: String,
    /// Schnorr signature, 64 bytes (`Signature::to_bytes`).
    pub signature: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LsuFinalityCertificateV1 {
    pub version: u8,
    pub style: u8,
    pub chain_id: u64,
    pub genesis_hash: [u8; 32],
    pub cycle: u64,
    pub lsu_hash: [u8; 32],
    pub committee_hash: [u8; 32],
    pub vote_list_hash: [u8; 32],
    pub votes: Vec<ProducerFinalityVote>,
}

impl LsuFinalityCertificateV1 {
    pub fn from_lsu_and_votes(
        lsu: &LedgerStateUpdate,
        chain_id: u64,
        genesis_hash: [u8; 32],
        lsu_hash: [u8; 32],
        votes: Vec<ProducerFinalityVote>,
    ) -> Result<Self, LsuFinalityVerifyError> {
        let committee_hash =
            committee_hash_ordered_producer_list(&lsu.producer_list).ok_or(LsuFinalityVerifyError::EmptyCommittee)?;
        let vote_list_hash = hash_producer_list_merkle(&lsu.vote_list);
        Ok(Self {
            version: 1,
            style: FINALITY_CERT_STYLE_INDIVIDUAL,
            chain_id,
            genesis_hash,
            cycle: lsu.cycle_number,
            lsu_hash,
            committee_hash,
            vote_list_hash,
            votes,
        })
    }
}

pub fn verify_lsu_finality_certificate(
    cert: &LsuFinalityCertificateV1,
    lsu: &LedgerStateUpdate,
) -> Result<(), LsuFinalityVerifyError> {
    if cert.version != 1 || cert.style != FINALITY_CERT_STYLE_INDIVIDUAL {
        return Err(LsuFinalityVerifyError::BadFormat);
    }
    if cert.votes.len() > MAX_FINALITY_VOTES {
        return Err(LsuFinalityVerifyError::TooManyVotes);
    }
    let lsu_hash = hash_data(lsu).map_err(|_| LsuFinalityVerifyError::HashError)?;
    if lsu_hash != cert.lsu_hash || lsu.cycle_number != cert.cycle {
        return Err(LsuFinalityVerifyError::LsuHashMismatch);
    }
    let committee_hash =
        committee_hash_ordered_producer_list(&lsu.producer_list).ok_or(LsuFinalityVerifyError::EmptyCommittee)?;
    if committee_hash != cert.committee_hash {
        return Err(LsuFinalityVerifyError::CommitteeHashMismatch);
    }
    let vote_list_hash = hash_producer_list_merkle(&lsu.vote_list);
    if vote_list_hash != cert.vote_list_hash {
        return Err(LsuFinalityVerifyError::VoteListHashMismatch);
    }

    let n = lsu.producer_list.len();
    let need = bft_vote_threshold(n);
    let scheme = SignatureScheme::new();
    let h = h_cert_v1(
        cert.chain_id,
        &cert.genesis_hash,
        cert.cycle,
        &cert.lsu_hash,
        &cert.committee_hash,
        &cert.vote_list_hash,
    );

    let mut seen = std::collections::BTreeSet::new();
    let mut valid = 0usize;
    for v in &cert.votes {
        if !seen.insert(v.producer_id.clone()) {
            return Err(LsuFinalityVerifyError::DuplicateVoter);
        }
        if !lsu.producer_list.iter().any(|p| p == &v.producer_id) {
            return Err(LsuFinalityVerifyError::VoterNotInCommittee);
        }
        if !lsu.vote_list.iter().any(|p| p == &v.producer_id) {
            return Err(LsuFinalityVerifyError::VoterNotInVoteList);
        }
        let pk_bytes =
            parse_producer_hex32(&v.producer_id).ok_or(LsuFinalityVerifyError::InvalidPublicKey)?;
        let pk = PublicKey::from_bytes(pk_bytes).map_err(|_| LsuFinalityVerifyError::InvalidPublicKey)?;
        if v.signature.len() != 64 {
            return Err(LsuFinalityVerifyError::InvalidSignatureBytes);
        }
        let mut sb = [0u8; 64];
        sb.copy_from_slice(&v.signature[..64]);
        let sig = Signature::from_bytes(sb).map_err(|_| LsuFinalityVerifyError::InvalidSignatureBytes)?;
        let ok = scheme
            .verify(&h, &sig, &pk)
            .map_err(|_| LsuFinalityVerifyError::InvalidSignature)?;
        if !ok {
            return Err(LsuFinalityVerifyError::InvalidSignature);
        }
        valid += 1;
    }
    if valid < need {
        return Err(LsuFinalityVerifyError::InsufficientQuorum);
    }
    Ok(())
}

/// Verify a single attestation (producer signed `H_cert` for the given preimage fields).
pub fn verify_finality_attestation(
    chain_id: u64,
    genesis_hash: &[u8; 32],
    cycle: u64,
    lsu_hash: &[u8; 32],
    committee_hash: &[u8; 32],
    vote_list_hash: &[u8; 32],
    producer_id: &str,
    signature: &[u8],
) -> Result<(), LsuFinalityVerifyError> {
    let h = h_cert_v1(
        chain_id,
        genesis_hash,
        cycle,
        lsu_hash,
        committee_hash,
        vote_list_hash,
    );
    let pk_bytes = parse_producer_hex32(producer_id).ok_or(LsuFinalityVerifyError::InvalidPublicKey)?;
    let pk = PublicKey::from_bytes(pk_bytes).map_err(|_| LsuFinalityVerifyError::InvalidPublicKey)?;
    if signature.len() != 64 {
        return Err(LsuFinalityVerifyError::InvalidSignatureBytes);
    }
    let mut sb = [0u8; 64];
    sb.copy_from_slice(&signature[..64]);
    let sig = Signature::from_bytes(sb).map_err(|_| LsuFinalityVerifyError::InvalidSignatureBytes)?;
    let scheme = SignatureScheme::new();
    let ok = scheme
        .verify(&h, &sig, &pk)
        .map_err(|_| LsuFinalityVerifyError::InvalidSignature)?;
    if !ok {
        return Err(LsuFinalityVerifyError::InvalidSignature);
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
    fn h_cert_v1_golden_vector() {
        let chain_id = 0x0102_0304_0506_0708u64;
        let genesis_hash = [0xabu8; 32];
        let cycle = 0x1122_3344_5566_7788u64;
        let lsu_hash = [0xcdu8; 32];
        let committee_hash = [0xefu8; 32];
        let vote_list_hash = [0x12u8; 32];
        let h = h_cert_v1(
            chain_id,
            &genesis_hash,
            cycle,
            &lsu_hash,
            &committee_hash,
            &vote_list_hash,
        );
        assert_eq!(
            hex::encode(h),
            "f52f6af24f1261e9d03a71a69a787c9481b9a3c820aa78d88f75c23c245044c2"
        );
    }

    #[test]
    fn certificate_round_trip_verify() {
        let pk_a = PrivateKey::generate(&mut OsRng);
        let pk_b = PrivateKey::generate(&mut OsRng);
        let id_a = hex::encode(pk_a.public_key().to_bytes());
        let id_b = hex::encode(pk_b.public_key().to_bytes());

        let lsu = LedgerStateUpdate {
            partial_update: PartialLedgerStateUpdate {
                transaction_entries: vec![TransactionEntry {
                    public_key: pk_a.public_key().to_bytes(),
                    amount: 0,
                    signature: vec![],
                }],
                transaction_signatures_hash: [1u8; 32],
                total_fees: 0,
                timestamp: 1,
            },
            compensation_entries: vec![CompensationEntry {
                producer_id: id_a.clone(),
                public_key: pk_a.public_key().to_bytes(),
                amount: 0,
            }],
            cycle_number: 42,
            producer_list: vec![id_a.clone(), id_b.clone(), "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff".to_string()],
            vote_list: vec![id_a.clone(), id_b.clone()],
        };

        let lsu_hash = hash_data(&lsu).unwrap();
        let chain_id = 7u64;
        let genesis_hash = [3u8; 32];
        let committee_hash = committee_hash_ordered_producer_list(&lsu.producer_list).unwrap();
        let vote_list_hash = hash_producer_list_merkle(&lsu.vote_list);
        let h = h_cert_v1(
            chain_id,
            &genesis_hash,
            lsu.cycle_number,
            &lsu_hash,
            &committee_hash,
            &vote_list_hash,
        );

        let mut rng = OsRng;
        let scheme = SignatureScheme::new();
        let sig_a = scheme.sign(&mut rng, &pk_a, &h).unwrap().to_bytes();
        let sig_b = scheme.sign(&mut rng, &pk_b, &h).unwrap().to_bytes();

        let cert = LsuFinalityCertificateV1 {
            version: 1,
            style: FINALITY_CERT_STYLE_INDIVIDUAL,
            chain_id,
            genesis_hash,
            cycle: lsu.cycle_number,
            lsu_hash,
            committee_hash,
            vote_list_hash,
            votes: vec![
                ProducerFinalityVote {
                    producer_id: id_a,
                    signature: sig_a.to_vec(),
                },
                ProducerFinalityVote {
                    producer_id: id_b,
                    signature: sig_b.to_vec(),
                },
            ],
        };
        verify_lsu_finality_certificate(&cert, &lsu).unwrap();
    }
}
