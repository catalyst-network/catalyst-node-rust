//! Basic usage example for Catalyst DFS
//!
//! Run with: cargo run --example basic_usage

use catalyst_dfs::{
    CategorizedStorage, ContentCategory, DfsConfig, DfsFactory, DfsService, DistributedFileSystem,
};
use std::path::PathBuf;
use tempfile::TempDir;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::init();

    println!("🚀 Catalyst DFS Basic Usage Example");
    println!("===================================");

    // Create a temporary directory for this example
    let temp_dir = TempDir::new()?;
    println!("📁 Storage directory: {}", temp_dir.path().display());

    // Create DFS configuration
    let config = DfsConfig {
        storage_dir: temp_dir.path().to_path_buf(),
        max_storage_size: 100 * 1024 * 1024, // 100MB
        enable_gc: true,
        gc_interval: 300,         // 5 minutes
        enable_networking: false, // Disable for local example
        ..Default::default()
    };

    println!("⚙️  Configuration: {:#?}", config);

    // Create DFS instance
    println!("\n🔧 Creating DFS instance...");
    let dfs = DfsFactory::create(&config).await?;

    // Example 1: Basic content storage and retrieval
    println!("\n📝 Example 1: Basic Storage");
    let content = b"Hello, Catalyst DFS! This is some example content.";
    println!("   Storing: {}", String::from_utf8_lossy(content));

    let cid = dfs.put(content.to_vec()).await?;
    println!("   ✅ Stored with CID: {}", cid.to_string());

    let retrieved = dfs.get(&cid).await?;
    println!("   ✅ Retrieved: {}", String::from_utf8_lossy(&retrieved));

    // Example 2: Content addressing verification
    println!("\n🔍 Example 2: Content Addressing");
    let same_content_cid = dfs.put(content.to_vec()).await?;
    println!("   Same content CID: {}", same_content_cid.to_string());
    println!("   ✅ CIDs match: {}", cid == same_content_cid);

    // Example 3: Metadata operations
    println!("\n📊 Example 3: Metadata");
    let metadata = dfs.metadata(&cid).await?;
    println!("   Size: {} bytes", metadata.size);
    println!("   Created: {}", metadata.created_at);
    println!("   Accessed: {} times", metadata.access_count);
    println!("   Pinned: {}", metadata.pinned);

    // Example 4: Pinning content
    println!("\n📌 Example 4: Pinning");
    println!("   Pinning content...");
    dfs.pin(&cid).await?;

    let metadata_after_pin = dfs.metadata(&cid).await?;
    println!("   ✅ Pinned: {}", metadata_after_pin.pinned);

    // Example 5: Categorized storage
    println!("\n🏷️  Example 5: Categorized Storage");
    let contract_code =
        b"contract MyContract { function hello() returns (string) { return 'Hello World'; } }";
    let media_file = b"This would be binary media data...";
    let app_data = b"{ \"user_id\": 123, \"settings\": { \"theme\": \"dark\" } }";

    let contract_cid = dfs
        .put_categorized(contract_code.to_vec(), ContentCategory::Contract)
        .await?;
    let media_cid = dfs
        .put_categorized(media_file.to_vec(), ContentCategory::Media)
        .await?;
    let app_cid = dfs
        .put_categorized(app_data.to_vec(), ContentCategory::AppData)
        .await?;

    println!("   📄 Contract CID: {}", contract_cid.to_string());
    println!("   🎬 Media CID: {}", media_cid.to_string());
    println!("   💾 App Data CID: {}", app_cid.to_string());

    // List content by category
    let contracts = dfs.list_by_category(ContentCategory::Contract).await?;
    println!("   ✅ Found {} contracts", contracts.len());

    // Example 6: Storage statistics
    println!("\n📈 Example 6: Storage Statistics");
    let stats = dfs.stats().await?;
    println!("   Total objects: {}", stats.total_objects);
    println!("   Total bytes: {}", stats.total_bytes);
    println!("   Pinned objects: {}", stats.pinned_objects);
    println!("   Available space: {} bytes", stats.available_space);

    // Example 7: List all content
    println!("\n📋 Example 7: Content Listing");
    let all_content = dfs.list().await?;
    println!("   Total content items: {}", all_content.len());
    for (i, item) in all_content.iter().enumerate() {
        println!(
            "   {}. {} ({} bytes, pinned: {})",
            i + 1,
            item.cid.to_string(),
            item.size,
            item.pinned
        );
    }

    // Example 8: Content existence check
    println!("\n🔎 Example 8: Content Existence");
    let exists = dfs.has(&cid).await?;
    println!("   Content exists: {}", exists);

    let fake_cid = catalyst_dfs::ContentId::from_data(b"non-existent")?;
    let fake_exists = dfs.has(&fake_cid).await?;
    println!("   Fake content exists: {}", fake_exists);

    // Example 9: Garbage collection
    println!("\n🗑️  Example 9: Garbage Collection");
    println!("   Running garbage collection...");
    let gc_result = dfs.gc().await?;
    println!("   ✅ GC completed:");
    println!("      Objects removed: {}", gc_result.objects_removed);
    println!("      Bytes freed: {}", gc_result.bytes_freed);
    println!("      Duration: {}ms", gc_result.duration_ms);

    // Example 10: Using DFS Service
    println!("\n🛠️  Example 10: DFS Service");
    let service = DfsService::new(config).await?;

    let service_data = b"Content stored via DFS Service";
    let service_cid = service
        .store_with_replication(service_data.to_vec())
        .await?;
    println!("   ✅ Stored via service: {}", service_cid.to_string());

    let comprehensive_stats = service.comprehensive_stats().await?;
    println!("   Service stats:");
    println!(
        "      DFS objects: {}",
        comprehensive_stats.dfs.total_objects
    );
    println!(
        "      Replication factor: {}",
        comprehensive_stats.config.replication_factor
    );

    println!("\n🎉 Example completed successfully!");
    println!("   The temporary directory will be cleaned up automatically.");

    Ok(())
}
