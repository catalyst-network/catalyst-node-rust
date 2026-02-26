use catalyst_core::protocol as corep;

fn hex32(s: &str) -> [u8; 32] {
    let s = s.trim().strip_prefix("0x").unwrap_or(s.trim());
    let bytes = hex::decode(s).expect("hex decode");
    assert_eq!(bytes.len(), 32);
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    out
}

#[test]
fn tx_v2_wire_and_txid_vectors_are_stable() {
    // If you intentionally change any canonical serialization rules for `CTX2` or the v2 signing
    // payload, update these golden vectors in the same PR and explain why in #207.

    // Fixed chain domain.
    let chain_id: u64 = 200820092;
    let genesis_hash = hex32("0x1111111111111111111111111111111111111111111111111111111111111111");

    // Fixed keys and payload.
    let pk_sender = hex32("0x2222222222222222222222222222222222222222222222222222222222222222");
    let pk_recipient = hex32("0x3333333333333333333333333333333333333333333333333333333333333333");

    let tx = corep::Transaction {
        core: corep::TransactionCore {
            tx_type: corep::TransactionType::NonConfidentialTransfer,
            entries: vec![
                corep::TransactionEntry {
                    public_key: pk_sender,
                    amount: corep::EntryAmount::NonConfidential(-123),
                },
                corep::TransactionEntry {
                    public_key: pk_recipient,
                    amount: corep::EntryAmount::NonConfidential(123),
                },
            ],
            nonce: 7,
            lock_time: 0,
            fees: 1,
            data: vec![],
        },
        signature_scheme: corep::sig_scheme::SCHNORR_V1,
        // Signature bytes are part of the wire tx. Use a fixed blob (not a real signature).
        signature: corep::AggregatedSignature(vec![0x44; 64]),
        sender_pubkey: None,
        timestamp: 1700000000000,
    };

    let wire = corep::encode_wire_tx_v2(&tx).expect("encode v2");
    let wire_hex = format!("0x{}", hex::encode(&wire));

    // NOTE: This value is expected to change only when the canonical encoding rules change.
    const EXPECT_WIRE_HEX: &str = "0x43545832000200000022222222222222222222222222222222222222222222222222222222222222220085ffffffffffffff3333333333333333333333333333333333333333333333333333333333333333007b00000000000000070000000000000000000000010000000000000000000000004000000044444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444000068e5cf8b010000";
    assert_eq!(
        wire_hex, EXPECT_WIRE_HEX,
        "CTX2 wire bytes changed; update golden vectors intentionally"
    );

    let txid = corep::tx_id_v2(&tx).expect("tx_id_v2");
    let txid_hex = format!("0x{}", hex::encode(txid));
    const EXPECT_TXID_HEX: &str =
        "0x548c6ac95792c2ee8f12f13f7113115837a5732769184ed2c61ca5c0b1732427";
    assert_eq!(txid_hex, EXPECT_TXID_HEX, "tx_id_v2 changed");

    // Signing payload v2 vector.
    let payload = tx
        .signing_payload_v2(chain_id, genesis_hash)
        .or_else(|_| tx.signing_payload_v1(chain_id, genesis_hash))
        .expect("signing payload");
    let payload_hex = format!("0x{}", hex::encode(payload));
    const EXPECT_PAYLOAD_HEX: &str = "0x434154414c5953545f5349475f56327c45f80b0000000011111111111111111111111111111111111111111111111111111111111111110000000200000022222222222222222222222222222222222222222222222222222222222222220085ffffffffffffff3333333333333333333333333333333333333333333333333333333333333333007b000000000000000700000000000000000000000100000000000000000000000068e5cf8b010000";
    assert_eq!(
        payload_hex, EXPECT_PAYLOAD_HEX,
        "v2 signing payload changed; update golden vectors intentionally"
    );
}
