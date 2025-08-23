use catalyst_crypto::*;
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use rand::thread_rng;

fn bench_key_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("key_operations");

    group.bench_function("key_generation", |b| {
        let mut rng = thread_rng();
        b.iter(|| {
            let _keypair = KeyPair::generate(black_box(&mut rng));
        });
    });

    group.bench_function("public_key_derivation", |b| {
        let mut rng = thread_rng();
        let private_key = PrivateKey::generate(&mut rng);
        b.iter(|| {
            let _public_key = private_key.public_key();
        });
    });

    group.bench_function("address_generation", |b| {
        let mut rng = thread_rng();
        let keypair = KeyPair::generate(&mut rng);
        b.iter(|| {
            let _address = keypair.public_key().to_address();
        });
    });

    group.finish();
}

fn bench_signature_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("signature_operations");
    let mut rng = thread_rng();
    let keypair = KeyPair::generate(&mut rng);
    let scheme = SignatureScheme::new();
    let message = b"benchmark message for signature operations";

    group.bench_function("signature_creation", |b| {
        let mut rng = thread_rng();
        b.iter(|| {
            let _signature = scheme
                .sign(
                    black_box(&mut rng),
                    black_box(keypair.private_key()),
                    black_box(message),
                )
                .unwrap();
        });
    });

    let signature = scheme
        .sign(&mut rng, keypair.private_key(), message)
        .unwrap();
    group.bench_function("signature_verification", |b| {
        b.iter(|| {
            let _valid = scheme
                .verify(
                    black_box(message),
                    black_box(&signature),
                    black_box(keypair.public_key()),
                )
                .unwrap();
        });
    });

    group.bench_function("signature_serialization", |b| {
        b.iter(|| {
            let _bytes = signature.to_bytes();
        });
    });

    let sig_bytes = signature.to_bytes();
    group.bench_function("signature_deserialization", |b| {
        b.iter(|| {
            let _sig = Signature::from_bytes(black_box(sig_bytes)).unwrap();
        });
    });

    group.finish();
}

fn bench_musig_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("musig_operations");
    let mut rng = thread_rng();
    let scheme = SignatureScheme::new();
    let message = b"benchmark message for musig operations";

    for participant_count in [3, 5, 10, 20].iter() {
        let keypairs: Vec<KeyPair> = (0..*participant_count)
            .map(|_| KeyPair::generate(&mut rng))
            .collect();

        let mut partial_signatures = Vec::new();
        for keypair in &keypairs {
            let sig = scheme
                .sign(&mut rng, keypair.private_key(), message)
                .unwrap();
            partial_signatures.push((sig, keypair.public_key().clone()));
        }

        group.bench_with_input(
            BenchmarkId::new("musig_aggregation", participant_count),
            participant_count,
            |b, _| {
                b.iter(|| {
                    let _musig_sig = scheme
                        .aggregate_signatures(black_box(message), black_box(&partial_signatures))
                        .unwrap();
                });
            },
        );

        let musig_sig = scheme
            .aggregate_signatures(message, &partial_signatures)
            .unwrap();
        group.bench_with_input(
            BenchmarkId::new("musig_verification", participant_count),
            participant_count,
            |b, _| {
                b.iter(|| {
                    let _valid = scheme
                        .verify_aggregated(black_box(message), black_box(&musig_sig))
                        .unwrap();
                });
            },
        );
    }

    group.finish();
}

fn bench_hash_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("hash_operations");

    for size in [32, 64, 256, 1024, 4096].iter() {
        let data = vec![0u8; *size];
        group.bench_with_input(BenchmarkId::new("blake2b_hash", size), size, |b, _| {
            b.iter(|| {
                let _hash = blake2b_hash(black_box(&data));
            });
        });
    }

    let data_pieces = vec![&b"piece1"[..], &b"piece2"[..], &b"piece3"[..]];
    group.bench_function("blake2b_hash_multiple", |b| {
        b.iter(|| {
            let _hash = blake2b_hash_multiple(black_box(&data_pieces));
        });
    });

    group.finish();
}

fn bench_commitment_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("commitment_operations");
    let mut rng = thread_rng();
    let scheme = CommitmentScheme::new();

    group.bench_function("commitment_creation", |b| {
        let mut rng = thread_rng();
        b.iter(|| {
            let _commitment = scheme.commit(black_box(&mut rng), black_box(100u64));
        });
    });

    let commitment = scheme.commit(&mut rng, 100u64);
    group.bench_function("commitment_verification", |b| {
        b.iter(|| {
            let _valid = scheme.verify(black_box(&commitment), black_box(100u64));
        });
    });

    let commitment1 = scheme.commit(&mut rng, 100u64);
    let commitment2 = scheme.commit(&mut rng, 200u64);
    group.bench_function("commitment_addition", |b| {
        b.iter(|| {
            let _sum = commitment1.add(black_box(&commitment2));
        });
    });

    group.bench_function("commitment_serialization", |b| {
        b.iter(|| {
            let _bytes = commitment.to_bytes();
        });
    });

    let commitment_bytes = commitment.to_bytes();
    group.bench_function("commitment_deserialization", |b| {
        b.iter(|| {
            let _point = PedersenCommitment::from_bytes(black_box(commitment_bytes)).unwrap();
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_key_operations,
    bench_signature_operations,
    bench_musig_operations,
    bench_hash_operations,
    bench_commitment_operations
);
criterion_main!(benches);
