use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use catalyst_storage::{StorageConfig, StorageManager, TransactionBatch};
use catalyst_utils::state::AccountState;
use tempfile::TempDir;
use tokio::runtime::Runtime;

fn create_test_storage() -> (StorageManager, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let mut config = StorageConfig::default();
    config.data_dir = temp_dir.path().to_path_buf();
    
    let rt = Runtime::new().unwrap();
    let storage = rt.block_on(StorageManager::new(config)).unwrap();
    
    (storage, temp_dir)
}

fn bench_account_operations(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let (storage, _temp_dir) = create_test_storage();
    
    let mut group = c.benchmark_group("account_operations");
    
    // Benchmark single account write
    group.bench_function("single_account_write", |b| {
        b.iter(|| {
            let address = [black_box(1u8); 21];
            let account = AccountState::new_non_confidential(address, 1000);
            rt.block_on(storage.set_account(&address, &account)).unwrap();
        })
    });
    
    // Benchmark single account read
    let address = [2u8; 21];
    let account = AccountState::new_non_confidential(address, 1000);
    rt.block_on(storage.set_account(&address, &account)).unwrap();
    
    group.bench_function("single_account_read", |b| {
        b.iter(|| {
            let result = rt.block_on(storage.get_account(&black_box(address))).unwrap();
            black_box(result);
        })
    });
    
    // Benchmark batch account writes
    for batch_size in [10, 100, 1000].iter() {
        group.bench_with_input(
            BenchmarkId::new("batch_account_writes", batch_size),
            batch_size,
            |b, &size| {
                b.iter(|| {
                    let tx_batch = storage.create_transaction(format!("bench_{}", size)).unwrap();
                    
                    for i in 0..size {
                        let address = [i as u8; 21];
                        let account = AccountState::new_non_confidential(address, i as u64 * 100);
                        rt.block_on(tx_batch.set_account(&address, &account)).unwrap();
                    }
                    
                    rt.block_on(storage.commit_transaction(&format!("bench_{}", size))).unwrap();
                });
            },
        );
    }
    
    group.finish();
}

fn bench_transaction_operations(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let (storage, _temp_dir) = create_test_storage();
    
    let mut group = c.benchmark_group("transaction_operations");
    
    // Benchmark transaction creation
    group.bench_function("transaction_creation", |b| {
        let mut counter = 0;
        b.iter(|| {
            let tx_id = format!("tx_{}", counter);
            counter += 1;
            let tx_batch = storage.create_transaction(black_box(tx_id)).unwrap();
            black_box(tx_batch);
        })
    });
    
    // Benchmark empty transaction commit
    group.bench_function("empty_transaction_commit", |b| {
        let mut counter = 0;
        b.iter(|| {
            let tx_id = format!("empty_tx_{}", counter);
            counter += 1;
            let tx_batch = storage.create_transaction(tx_id.clone()).unwrap();
            
            // Add at least one operation to avoid empty transaction error
            let address = [counter as u8; 21];
            let account = AccountState::new_non_confidential(address, 100);
            rt.block_on(tx_batch.set_account(&address, &account)).unwrap();
            
            rt.block_on(storage.commit_transaction(&tx_id)).unwrap();
        })
    });
    
    group.finish();
}

fn bench_metadata_operations(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let (storage, _temp_dir) = create_test_storage();
    
    let mut group = c.benchmark_group("metadata_operations");
    
    // Benchmark metadata write
    group.bench_function("metadata_write", |b| {
        let mut counter = 0;
        b.iter(|| {
            let key = format!("key_{}", counter);
            let value = format!("value_{}", counter);
            counter += 1;
            rt.block_on(storage.set_metadata(&key, value.as_bytes())).unwrap();
        })
    });
    
    // Benchmark metadata read
    rt.block_on(storage.set_metadata("read_key", b"read_value")).unwrap();
    
    group.bench_function("metadata_read", |b| {
        b.iter(|| {
            let result = rt.block_on(storage.get_metadata(black_box("read_key"))).unwrap();
            black_box(result);
        })
    });
    
    group.finish();
}

fn bench_snapshot_operations(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let (storage, _temp_dir) = create_test_storage();
    
    // Add some data first
    for i in 0..100 {
        let address = [i as u8; 21];
        let account = AccountState::new_non_confidential(address, i as u64 * 100);
        rt.block_on(storage.set_account(&address, &account)).unwrap();
    }
    
    let mut group = c.benchmark_group("snapshot_operations");
    
    // Benchmark snapshot creation
    group.bench_function("snapshot_creation", |b| {
        let mut counter = 0;
        b.iter(|| {
            let name = format!("bench_snapshot_{}", counter);
            counter += 1;
            rt.block_on(storage.create_snapshot(&name)).unwrap();
        })
    });
    
    group.finish();
}

fn bench_serialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("serialization");
    
    let address = [1u8; 21];
    let account = AccountState::new_non_confidential(address, 1000);
    
    // Benchmark account serialization
    group.bench_function("account_serialize", |b| {
        b.iter(|| {
            let serialized = black_box(&account).serialize().unwrap();
            black_box(serialized);
        })
    });
    
    // Benchmark account deserialization
    let serialized = account.serialize().unwrap();
    group.bench_function("account_deserialize", |b| {
        b.iter(|| {
            let deserialized = AccountState::deserialize(black_box(&serialized)).unwrap();
            black_box(deserialized);
        })
    });
    
    group.finish();
}

fn bench_concurrent_operations(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let (storage, _temp_dir) = create_test_storage();
    
    let mut group = c.benchmark_group("concurrent_operations");
    
    // Benchmark concurrent account reads
    // Pre-populate with some data
    for i in 0..1000 {
        let address = [i as u8; 21];
        let account = AccountState::new_non_confidential(address, i as u64 * 100);
        rt.block_on(storage.set_account(&address, &account)).unwrap();
    }
    
    group.bench_function("concurrent_reads", |b| {
        b.iter(|| {
            let tasks: Vec<_> = (0..10).map(|i| {
                let storage = &storage;
                async move {
                    let address = [(i * 100) as u8; 21];
                    storage.get_account(&address).await.unwrap()
                }
            }).collect();
            
            rt.block_on(async {
                let results = futures::future::join_all(tasks).await;
                black_box(results);
            });
        })
    });
    
    group.finish();
}

criterion_group!(
    benches,
    bench_account_operations,
    bench_transaction_operations,
    bench_metadata_operations,
    bench_snapshot_operations,
    bench_serialization,
    bench_concurrent_operations
);

criterion_main!(benches);