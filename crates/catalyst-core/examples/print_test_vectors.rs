use catalyst_core::protocol as p;

fn hex32(b: &[u8; 32]) -> String {
    format!("0x{}", hex::encode(b))
}

fn hex_bytes(b: &[u8]) -> String {
    format!("0x{}", hex::encode(b))
}

fn main() {
    // --- Transaction signing payload + txid vector ---
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
    let tx = p::Transaction {
        core: core.clone(),
        signature: p::AggregatedSignature(vec![0u8; 64]),
        timestamp,
    };
    let txid = p::transaction_id(&tx).unwrap();

    println!("tx_signing_payload_hex={}", hex_bytes(&payload));
    println!("tx_id_hex={}", hex32(&txid));

    // --- Producer selection vector ---
    let seed: p::Hash32 = [42u8; 32];
    let workers: Vec<p::WorkerId> = (0u8..10u8)
        .map(|i| {
            let mut id = [0u8; 32];
            id[0] = i;
            id
        })
        .collect();
    let selected = p::select_producers_for_next_cycle(&workers, &seed, 4);

    println!("producer_seed_hex={}", hex32(&seed));
    println!("producer_selected_count={}", selected.len());
    for (i, w) in selected.iter().enumerate() {
        println!("producer_selected_{}_hex={}", i, hex32(w));
    }
}

