//! Benchmark tests for Catalyst DFS performance.
//!
//! Run with:
//! - `cargo bench -p catalyst-dfs`

use catalyst_dfs::{ContentId, DfsConfig, DfsFactory, DistributedFileSystem};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use tempfile::TempDir;
use tokio::runtime::Runtime;

async fn create_test_dfs() -> (TempDir, Box<dyn DistributedFileSystem>) {
    let temp_dir = TempDir::new().unwrap();
    let config = DfsConfig::test_config(temp_dir.path().to_path_buf());
    let dfs = DfsFactory::create(&config).await.unwrap();
    (temp_dir, dfs)
}

fn bench_put(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("dfs_put");

    for size in [1024usize, 10 * 1024, 100 * 1024, 1024 * 1024] {
        group.bench_with_input(BenchmarkId::new("put", size), &size, |b, &size| {
            b.to_async(&rt).iter_with_setup(
                || rt.block_on(create_test_dfs()),
                |(_td, dfs)| async move {
                    let data = vec![0u8; size];
                    black_box(dfs.put(data).await.unwrap())
                },
            );
        });
    }

    group.finish();
}

fn bench_get(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("dfs_get");

    for size in [1024usize, 10 * 1024, 100 * 1024] {
        group.bench_with_input(BenchmarkId::new("get", size), &size, |b, &size| {
            b.to_async(&rt).iter_with_setup(
                || {
                    rt.block_on(async {
                        let (td, dfs) = create_test_dfs().await;
                        let data = vec![0u8; size];
                        let cid: ContentId = dfs.put(data).await.unwrap();
                        (td, dfs, cid)
                    })
                },
                |(_td, dfs, cid)| async move { black_box(dfs.get(&cid).await.unwrap()) },
            );
        });
    }

    group.finish();
}

fn bench_has(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("dfs_has", |b| {
        b.to_async(&rt).iter_with_setup(
            || {
                rt.block_on(async {
                    let (td, dfs) = create_test_dfs().await;
                    let cid = dfs.put(vec![1u8; 1024]).await.unwrap();
                    (td, dfs, cid)
                })
            },
            |(_td, dfs, cid)| async move { black_box(dfs.has(&cid).await.unwrap()) },
        );
    });
}

fn bench_cid_from_data(c: &mut Criterion) {
    let mut group = c.benchmark_group("dfs_cid");

    for size in [32usize, 1024, 10 * 1024, 100 * 1024] {
        group.bench_with_input(BenchmarkId::new("from_data", size), &size, |b, &size| {
            let data = vec![0u8; size];
            b.iter(|| black_box(ContentId::from_data(&data).unwrap()));
        });
    }

    group.finish();
}

criterion_group!(benches, bench_put, bench_get, bench_has, bench_cid_from_data);
criterion_main!(benches);

