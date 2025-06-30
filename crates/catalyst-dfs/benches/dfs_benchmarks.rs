//! Benchmark tests for Catalyst DFS performance
//! 
//! Run with: cargo bench

use catalyst_dfs::{DfsConfig, DfsFactory, ContentCategory, CategorizedStorage, DistributedFileSystem};
use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use tempfile::TempDir;
use tokio::runtime::Runtime;
use std::sync::Arc;

// Helper function to create DFS instance for benchmarks
async fn create_test_dfs() -> Box<dyn DistributedFileSystem> {
    group.finish();
}

fn bench_content_addressing(c: &mut Criterion) {
    let mut group = c.benchmark_group("content_addressing");
    
    let sizes = vec![1024, 10240, 102400, 1048576];
    
    for size in sizes {
        group.benchmark_with_input(
            BenchmarkId::new("cid_generation", size),
            &size,
            |b, &size| {
                let data = vec![0u8; size];
                b.iter(|| {
                    black_box(catalyst_dfs::ContentId::from_data(&data).unwrap())
                });
            },
        );
    }
    
    group.finish();
}

fn bench_metadata_operations(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    c.bench_function("metadata_lookup", |b| {
        let dfs = rt.block_on(create_test_dfs());
        let data = vec![0u8; 1024];
        let cid = rt.block_on(dfs.put(data)).unwrap();
        
        b.to_async(&rt).iter(|| async {
            black_box(dfs.metadata(&cid).await.unwrap())
        });
    });
    
    c.bench_function("has_content", |b| {
        let dfs = rt.block_on(create_test_dfs());
        let data = vec![0u8; 1024];
        let cid = rt.block_on(dfs.put(data)).unwrap();
        
        b.to_async(&rt).iter(|| async {
            black_box(dfs.has(&cid).await.unwrap())
        });
    });
    
    c.bench_function("pin_unpin", |b| {
        let dfs = rt.block_on(create_test_dfs());
        let data = vec![0u8; 1024];
        let cid = rt.block_on(dfs.put(data)).unwrap();
        
        b.to_async(&rt).iter(|| async {
            dfs.pin(&cid).await.unwrap();
            black_box(dfs.unpin(&cid).await.unwrap())
        });
    });
}

fn bench_categorized_storage(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("categorized");
    
    let categories = vec![
        ContentCategory::Contract,
        ContentCategory::LedgerUpdate,
        ContentCategory::AppData,
        ContentCategory::Media,
        ContentCategory::File,
    ];
    
    for category in categories {
        group.benchmark_with_input(
            BenchmarkId::new("put_categorized", format!("{:?}", category)),
            &category,
            |b, category| {
                let dfs = rt.block_on(create_test_dfs());
                b.to_async(&rt).iter(|| async {
                    let data = vec![0u8; 1024];
                    black_box(dfs.put_categorized(data, category.clone()).await.unwrap())
                });
            },
        );
    }
    
    group.finish();
}

fn bench_concurrent_operations(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("concurrent");
    
    let thread_counts = vec![1, 2, 4, 8, 16];
    
    for thread_count in thread_counts {
        group.benchmark_with_input(
            BenchmarkId::new("concurrent_puts", thread_count),
            &thread_count,
            |b, &thread_count| {
                b.to_async(&rt).iter(|| async {
                    let dfs = Arc::new(create_test_dfs().await);
                    let mut handles = Vec::new();
                    
                    for i in 0..thread_count {
                        let dfs_clone = Arc::clone(&dfs);
                        let handle = tokio::spawn(async move {
                            let data = vec![i as u8; 1024];
                            dfs_clone.put(data).await.unwrap()
                        });
                        handles.push(handle);
                    }
                    
                    let results = futures::future::join_all(handles).await;
                    black_box(results)
                });
            },
        );
    }
    
    group.finish();
}

fn bench_garbage_collection(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    c.bench_function("gc_empty", |b| {
        let mut config = DfsConfig::test_config(TempDir::new().unwrap().path().to_path_buf());
        config.enable_gc = true;
        
        b.to_async(&rt).iter(|| async {
            let dfs = DfsFactory::create(&config).await.unwrap();
            black_box(dfs.gc().await.unwrap())
        });
    });
    
    c.bench_function("gc_with_content", |b| {
        b.to_async(&rt).iter(|| async {
            let mut config = DfsConfig::test_config(TempDir::new().unwrap().path().to_path_buf());
            config.enable_gc = true;
            let dfs = DfsFactory::create(&config).await.unwrap();
            
            // Add some content
            for i in 0..100 {
                let data = vec![i as u8; 1024];
                dfs.put(data).await.unwrap();
            }
            
            black_box(dfs.gc().await.unwrap())
        });
    });
}

fn bench_list_operations(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("list_operations");
    
    let content_counts = vec![10, 100, 1000];
    
    for count in content_counts {
        group.benchmark_with_input(
            BenchmarkId::new("list_all", count),
            &count,
            |b, &count| {
                b.to_async(&rt).iter_with_setup(
                    || {
                        rt.block_on(async {
                            let dfs = create_test_dfs().await;
                            
                            // Populate with content
                            for i in 0..count {
                                let data = vec![i as u8; 100];
                                dfs.put(data).await.unwrap();
                            }
                            
                            dfs
                        })
                    },
                    |dfs| async move {
                        black_box(dfs.list().await.unwrap())
                    },
                );
            },
        );
        
        group.benchmark_with_input(
            BenchmarkId::new("list_by_category", count),
            &count,
            |b, &count| {
                b.to_async(&rt).iter_with_setup(
                    || {
                        rt.block_on(async {
                            let dfs = create_test_dfs().await;
                            
                            // Populate with categorized content
                            for i in 0..count {
                                let data = vec![i as u8; 100];
                                dfs.put_categorized(data, ContentCategory::AppData).await.unwrap();
                            }
                            
                            dfs
                        })
                    },
                    |dfs| async move {
                        black_box(dfs.list_by_category(ContentCategory::AppData).await.unwrap())
                    },
                );
            },
        );
    }
    
    group.finish();
}

fn bench_stats_operations(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    c.bench_function("stats_empty", |b| {
        let dfs = rt.block_on(create_test_dfs());
        b.to_async(&rt).iter(|| async {
            black_box(dfs.stats().await.unwrap())
        });
    });
    
    c.bench_function("stats_with_content", |b| {
        b.to_async(&rt).iter_with_setup(
            || {
                rt.block_on(async {
                    let dfs = create_test_dfs().await;
                    
                    // Add content
                    for i in 0..100 {
                        let data = vec![i as u8; 1024];
                        dfs.put(data).await.unwrap();
                    }
                    
                    dfs
                })
            },
            |dfs| async move {
                black_box(dfs.stats().await.unwrap())
            },
        );
    });
}

fn bench_provider_operations(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    c.bench_function("provider_announce", |b| {
        let provider = catalyst_dfs::DfsContentProvider::new();
        let cid = catalyst_dfs::ContentId::from_data(b"test").unwrap();
        
        b.to_async(&rt).iter(|| async {
            black_box(provider.provide(&cid).await.unwrap())
        });
    });
    
    c.bench_function("provider_find", |b| {
        let provider = catalyst_dfs::DfsContentProvider::new();
        let cid = catalyst_dfs::ContentId::from_data(b"test").unwrap();
        rt.block_on(provider.provide(&cid)).unwrap();
        
        b.to_async(&rt).iter(|| async {
            black_box(provider.find_providers(&cid).await.unwrap())
        });
    });
}

criterion_group!(
    benches,
    bench_content_storage,
    bench_content_retrieval,
    bench_content_addressing,
    bench_metadata_operations,
    bench_categorized_storage,
    bench_concurrent_operations,
    bench_garbage_collection,
    bench_list_operations,
    bench_stats_operations,
    bench_provider_operations
);

criterion_main!(benches);let temp_dir = TempDir::new().unwrap();
    let config = DfsConfig::test_config(temp_dir.path().to_path_buf());
    DfsFactory::create(&config).await.unwrap()
}

fn bench_content_storage(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("storage");
    
    // Test different content sizes
    let sizes = vec![1024, 10240, 102400, 1048576]; // 1KB, 10KB, 100KB, 1MB
    
    for size in sizes {
        group.benchmark_with_input(
            BenchmarkId::new("put", size),
            &size,
            |b, &size| {
                let dfs = rt.block_on(create_test_dfs());
                b.to_async(&rt).iter(|| async {
                    let data = vec![0u8; size];
                    black_box(dfs.put(data).await.unwrap())
                });
            },
        );
    }
    
    group.finish();
}

fn bench_content_retrieval(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("retrieval");
    
    let sizes = vec![1024, 10240, 102400, 1048576];
    
    for size in sizes {
        group.benchmark_with_input(
            BenchmarkId::new("get", size),
            &size,
            |b, &size| {
                let dfs = rt.block_on(create_test_dfs());
                let data = vec![0u8; size];
                let cid = rt.block_on(dfs.put(data)).unwrap();
                
                b.to_async(&rt).iter(|| async {
                    black_box(dfs.get(&cid).await.unwrap())
                });
            },
        );
    }