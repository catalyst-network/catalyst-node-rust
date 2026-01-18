#![cfg(feature = "integration-tests")]

//! Integration tests for Catalyst DFS
//! 
//! These tests verify the complete system functionality including
//! storage, networking, and provider systems working together.

use catalyst_dfs::{
    DfsConfig, DfsFactory, DfsService, IpfsFactory,
    ContentCategory, CategorizedStorage, DistributedFileSystem,
    ContentProvider, DhtContentProvider, ContentReplicator,
    ReplicationLevel,
};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::time::{timeout, Duration};

#[tokio::test]
async fn test_full_dfs_service_integration() {
    let temp_dir = TempDir::new().unwrap();
    let config = DfsConfig::test_config(temp_dir.path().to_path_buf());
    
    // Create DFS service
    let service = DfsService::new(config).await.unwrap();
    
    // Test content storage with replication
    let data = b"integration test content".to_vec();
    let cid = service.store_with_replication(data.clone()).await.unwrap();
    
    // Verify content can be retrieved
    let retrieved = service.dfs().get(&cid).await.unwrap();
    assert_eq!(data, retrieved);
    
    // Test pinning
    service.dfs().pin(&cid).await.unwrap();
    let metadata = service.dfs().metadata(&cid).await.unwrap();
    assert!(metadata.pinned);
    
    // Test statistics
    let stats = service.comprehensive_stats().await.unwrap();
    assert!(stats.dfs.total_objects > 0);
    assert!(stats.dfs.total_bytes > 0);
}

#[tokio::test]
async fn test_categorized_storage_integration() {
    let temp_dir = TempDir::new().unwrap();
    let config = DfsConfig::test_config(temp_dir.path().to_path_buf());
    
    let dfs = DfsFactory::create(&config).await.unwrap();
    
    // Store different categories of content
    let contract_data = b"contract bytecode".to_vec();
    let media_data = b"media file content".to_vec();
    let app_data = b"application data".to_vec();
    
    let contract_cid = dfs.put_categorized(contract_data.clone(), ContentCategory::Contract).await.unwrap();
    let media_cid = dfs.put_categorized(media_data.clone(), ContentCategory::Media).await.unwrap();
    let app_cid = dfs.put_categorized(app_data.clone(), ContentCategory::AppData).await.unwrap();
    
    // Verify content by category
    let contracts = dfs.list_by_category(ContentCategory::Contract).await.unwrap();
    assert_eq!(contracts.len(), 1);
    assert_eq!(contracts[0].cid, contract_cid);
    
    let media_files = dfs.list_by_category(ContentCategory::Media).await.unwrap();
    assert_eq!(media_files.len(), 1);
    assert_eq!(media_files[0].cid, media_cid);
    
    let app_files = dfs.list_by_category(ContentCategory::AppData).await.unwrap();
    assert_eq!(app_files.len(), 1);
    assert_eq!(app_files[0].cid, app_cid);
    
    // Verify content retrieval
    let retrieved_contract = dfs.get(&contract_cid).await.unwrap();
    assert_eq!(contract_data, retrieved_contract);
}

#[tokio::test]
async fn test_provider_system_integration() {
    let provider = Arc::new(DhtContentProvider::new());
    let cid = catalyst_dfs::ContentId::from_data(b"provider test").unwrap();
    
    // Test providing content
    provider.provide(&cid).await.unwrap();
    
    // Test finding providers
    let providers = provider.find_providers(&cid).await.unwrap();
    assert!(!providers.is_empty());
    
    // Test replication monitoring
    let replicator = Arc::new(ContentReplicator::new(Arc::clone(&provider), 3));
    let status = replicator.check_replication(&cid).await.unwrap();
    
    // Should have insufficient replication initially (only 1 provider)
    assert_eq!(status.level, ReplicationLevel::Insufficient);
    assert_eq!(status.current_replication, 1);
    assert_eq!(status.target_replication, 3);
}

#[tokio::test]
async fn test_gc_integration() {
    let temp_dir = TempDir::new().unwrap();
    let mut config = DfsConfig::test_config(temp_dir.path().to_path_buf());
    config.enable_gc = true;
    
    let dfs = DfsFactory::create(&config).await.unwrap();
    
    // Store some content
    let data1 = b"content to be garbage collected".to_vec();
    let data2 = b"content to be pinned".to_vec();
    
    let cid1 = dfs.put(data1).await.unwrap();
    let cid2 = dfs.put(data2).await.unwrap();
    
    // Pin one piece of content
    dfs.pin(&cid2).await.unwrap();
    
    // Get initial stats
    let initial_stats = dfs.stats().await.unwrap();
    assert_eq!(initial_stats.total_objects, 2);
    assert_eq!(initial_stats.pinned_objects, 1);
    
    // Run garbage collection
    let gc_result = dfs.gc().await.unwrap();
    
    // Verify GC ran but didn't remove anything yet (content is too recent)
    assert_eq!(gc_result.objects_removed, 0);
    
    // Verify pinned content is still accessible
    let retrieved = dfs.get(&cid2).await.unwrap();
    assert_eq!(b"content to be pinned".to_vec(), retrieved);
}

#[tokio::test]
async fn test_ipfs_factory_integration() {
    let temp_dir = TempDir::new().unwrap();
    let config = DfsConfig::test_config(temp_dir.path().to_path_buf());
    
    // Test IPFS factory
    let dfs = IpfsFactory::create_ipfs_node(config).await.unwrap();
    
    // Test basic IPFS-like operations
    let data = b"IPFS compatible test".to_vec();
    let cid = dfs.put(data.clone()).await.unwrap();
    
    // Verify CID is deterministic
    let cid2 = catalyst_dfs::ContentId::from_data(&data).unwrap();
    assert_eq!(cid, cid2);
    
    // Verify content retrieval
    let retrieved = dfs.get(&cid).await.unwrap();
    assert_eq!(data, retrieved);
    
    // Test has operation
    assert!(dfs.has(&cid).await.unwrap());
    
    // Test metadata
    let metadata = dfs.metadata(&cid).await.unwrap();
    assert_eq!(metadata.cid, cid);
    assert_eq!(metadata.size, data.len() as u64);
}

#[tokio::test]
async fn test_multiple_dfs_instances() {
    // Create two separate DFS instances
    let temp_dir1 = TempDir::new().unwrap();
    let temp_dir2 = TempDir::new().unwrap();
    
    let config1 = DfsConfig::test_config(temp_dir1.path().to_path_buf());
    let config2 = DfsConfig::test_config(temp_dir2.path().to_path_buf());
    
    let dfs1 = DfsFactory::create(&config1).await.unwrap();
    let dfs2 = DfsFactory::create(&config2).await.unwrap();
    
    // Store content in first instance
    let data = b"shared content".to_vec();
    let cid = dfs1.put(data.clone()).await.unwrap();
    
    // Verify it exists in first instance
    assert!(dfs1.has(&cid).await.unwrap());
    
    // Verify it doesn't exist in second instance (different storage)
    assert!(!dfs2.has(&cid).await.unwrap());
    
    // But we can store the same content in second instance
    let cid2 = dfs2.put(data.clone()).await.unwrap();
    
    // Should get same CID (content addressing)
    assert_eq!(cid, cid2);
    
    // Now both instances should have it
    assert!(dfs1.has(&cid).await.unwrap());
    assert!(dfs2.has(&cid).await.unwrap());
}

#[tokio::test]
async fn test_error_handling_integration() {
    let temp_dir = TempDir::new().unwrap();
    let config = DfsConfig::test_config(temp_dir.path().to_path_buf());
    
    let dfs = DfsFactory::create(&config).await.unwrap();
    
    // Test getting non-existent content
    let fake_cid = catalyst_dfs::ContentId::from_data(b"fake").unwrap();
    let result = dfs.get(&fake_cid).await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), catalyst_dfs::DfsError::NotFound(_)));
    
    // Test metadata for non-existent content
    let result = dfs.metadata(&fake_cid).await;
    assert!(result.is_err());
    
    // Test unpinning non-existent content
    let result = dfs.unpin(&fake_cid).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_concurrent_operations() {
    let temp_dir = TempDir::new().unwrap();
    let config = DfsConfig::test_config(temp_dir.path().to_path_buf());
    
    let dfs = Arc::new(DfsFactory::create(&config).await.unwrap());
    
    // Spawn multiple concurrent tasks
    let mut handles = Vec::new();
    
    for i in 0..10 {
        let dfs_clone = Arc::clone(&dfs);
        let handle = tokio::spawn(async move {
            let data = format!("concurrent test data {}", i).into_bytes();
            let cid = dfs_clone.put(data.clone()).await.unwrap();
            let retrieved = dfs_clone.get(&cid).await.unwrap();
            assert_eq!(data, retrieved);
            cid
        });
        handles.push(handle);
    }
    
    // Wait for all tasks to complete
    let cids: Vec<_> = futures::future::join_all(handles)
        .await
        .into_iter()
        .map(|result| result.unwrap())
        .collect();
    
    // Verify all content exists
    for cid in cids {
        assert!(dfs.has(&cid).await.unwrap());
    }
    
    // Verify stats
    let stats = dfs.stats().await.unwrap();
    assert_eq!(stats.total_objects, 10);
}

#[tokio::test]
async fn test_configuration_validation() {
    let temp_dir = TempDir::new().unwrap();
    let mut config = DfsConfig::test_config(temp_dir.path().to_path_buf());
    
    // Valid configuration should work
    config.validate().unwrap();
    let _dfs = DfsFactory::create(&config).await.unwrap();
    
    // Invalid storage size should fail
    config.max_storage_size = 0;
    assert!(config.validate().is_err());
    
    // Invalid replication factor should fail
    config.max_storage_size = 1024;
    config.replication_factor = 0;
    assert!(config.validate().is_err());
}

#[tokio::test]
async fn test_storage_limits() {
    let temp_dir = TempDir::new().unwrap();
    let mut config = DfsConfig::test_config(temp_dir.path().to_path_buf());
    
    // Set very small storage limit
    config.max_storage_size = 100; // 100 bytes
    
    let dfs = DfsFactory::create(&config).await.unwrap();
    
    // Store small content (should work)
    let small_data = b"small".to_vec(); // 5 bytes
    let _cid = dfs.put(small_data).await.unwrap();
    
    // Try to store large content (should fail)
    let large_data = vec![0u8; 200]; // 200 bytes
    let result = dfs.put(large_data).await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), catalyst_dfs::DfsError::Storage(_)));
}