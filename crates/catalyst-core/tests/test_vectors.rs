use catalyst_core::protocol as p;

#[test]
fn tx_signing_payload_vector() {
    let from_pk = [1u8; 32];
    let to_pk = [2u8; 32];

    let core = p::TransactionCore {
        tx_type: p::TransactionType::NonConfidentialTransfer,
        entries: vec![
            p::TransactionEntry {
                public_key: from_pk,
                amount: p::EntryAmount::NonConfidential(-5),
            },
            p::TransactionEntry {
                public_key: to_pk,
                amount: p::EntryAmount::NonConfidential(5),
            },
        ],
        nonce: 1,
        lock_time: 0,
        fees: 0,
        data: Vec::new(),
    };
    let timestamp = 123u64;

    let payload = p::transaction_signing_payload(&core, timestamp).unwrap();
    assert_eq!(
        format!("0x{}", hex::encode(payload)),
        "0x000000000200000000000000010101010101010101010101010101010101010101010101010101010101010100000000fbffffffffffffff0202020202020202020202020202020202020202020202020202020202020202000000000500000000000000010000000000000000000000000000000000000000000000000000007b00000000000000"
    );
}

#[test]
fn tx_id_vector() {
    let from_pk = [1u8; 32];
    let to_pk = [2u8; 32];

    let core = p::TransactionCore {
        tx_type: p::TransactionType::NonConfidentialTransfer,
        entries: vec![
            p::TransactionEntry {
                public_key: from_pk,
                amount: p::EntryAmount::NonConfidential(-5),
            },
            p::TransactionEntry {
                public_key: to_pk,
                amount: p::EntryAmount::NonConfidential(5),
            },
        ],
        nonce: 1,
        lock_time: 0,
        fees: 0,
        data: Vec::new(),
    };
    let timestamp = 123u64;

    let tx = p::Transaction {
        core,
        signature: p::AggregatedSignature(vec![0u8; 64]),
        timestamp,
    };
    let txid = p::transaction_id(&tx).unwrap();
    assert_eq!(
        format!("0x{}", hex::encode(txid)),
        "0x40385854022fe17953c55fddd2fdd62a9ab0baa830282ec143460b64534e9177"
    );
}

#[test]
fn producer_selection_vector() {
    let seed: p::Hash32 = [42u8; 32];
    let workers: Vec<p::WorkerId> = (0u8..10u8)
        .map(|i| {
            let mut id = [0u8; 32];
            id[0] = i;
            id
        })
        .collect();

    let selected = p::select_producers_for_next_cycle(&workers, &seed, 4);
    let got: Vec<String> = selected.iter().map(|id| format!("0x{}", hex::encode(id))).collect();

    assert_eq!(
        got,
        vec![
            "0x0800000000000000000000000000000000000000000000000000000000000000".to_string(),
            "0x0900000000000000000000000000000000000000000000000000000000000000".to_string(),
            "0x0200000000000000000000000000000000000000000000000000000000000000".to_string(),
            "0x0300000000000000000000000000000000000000000000000000000000000000".to_string(),
        ]
    );
}

