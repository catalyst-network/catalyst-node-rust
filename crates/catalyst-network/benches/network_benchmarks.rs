//! Network performance benchmarks (message creation/serialization).
//!
//! Run with:
//! - `cargo bench -p catalyst-network`

use catalyst_network::messaging::{NetworkMessage, RoutingStrategy};
use catalyst_utils::{
    network::{BasicMessage, MessageType},
    serialization::{CatalystDeserialize, CatalystSerialize},
};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};

fn benchmark_message_creation(c: &mut Criterion) {
    c.bench_function("network_message_creation", |b| {
        b.iter(|| {
            let msg = BasicMessage {
                content: "hello".to_string(),
                message_type: MessageType::ConsensusSync,
            };
            let nm = NetworkMessage::new(&msg, "sender".to_string(), None, RoutingStrategy::Broadcast)
                .unwrap();
            black_box(nm);
        });
    });
}

fn benchmark_message_serialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("network_message_serialization");
    for size in [32usize, 1024, 10 * 1024, 100 * 1024] {
        group.bench_with_input(BenchmarkId::new("serialize", size), &size, |b, &size| {
            let msg = BasicMessage {
                content: "x".repeat(size),
                message_type: MessageType::ConsensusSync,
            };
            let nm = NetworkMessage::new(&msg, "sender".to_string(), None, RoutingStrategy::Broadcast)
                .unwrap();
            b.iter(|| {
                let bytes = CatalystSerialize::serialize(black_box(&nm)).unwrap();
                black_box(bytes);
            });
        });

        group.bench_with_input(BenchmarkId::new("deserialize", size), &size, |b, &size| {
            let msg = BasicMessage {
                content: "x".repeat(size),
                message_type: MessageType::ConsensusSync,
            };
            let nm = NetworkMessage::new(&msg, "sender".to_string(), None, RoutingStrategy::Broadcast)
                .unwrap();
            let bytes = CatalystSerialize::serialize(&nm).unwrap();
            b.iter(|| {
                let decoded = NetworkMessage::deserialize(black_box(&bytes)).unwrap();
                black_box(decoded);
            });
        });
    }
    group.finish();
}

criterion_group!(benches, benchmark_message_creation, benchmark_message_serialization);
criterion_main!(benches);

