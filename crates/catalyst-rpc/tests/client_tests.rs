//! Simple RPC client for testing
//! You can create this as a test file or separate binary

use serde_json::{json, Value};
use std::collections::HashMap;

pub async fn test_rpc_client(rpc_url: &str) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();

    println!("🧪 Testing Catalyst RPC server at: {}", rpc_url);

    // Test block number
    let response = client
        .post(rpc_url)
        .json(&json!({
            "jsonrpc": "2.0",
            "method": "catalyst_blockNumber",
            "params": [],
            "id": 1
        }))
        .send()
        .await?;

    if response.status().is_success() {
        let result: Value = response.json().await?;
        println!("✅ Block number: {}", result);
    } else {
        println!("❌ Failed to get block number: {}", response.status());
    }

    // Test node status
    let response = client
        .post(rpc_url)
        .json(&json!({
            "jsonrpc": "2.0",
            "method": "catalyst_status",
            "params": [],
            "id": 2
        }))
        .send()
        .await?;

    if response.status().is_success() {
        let result: Value = response.json().await?;
        println!("✅ Node status: {}", result);
    } else {
        println!("❌ Failed to get node status: {}", response.status());
    }

    // Test network info
    let response = client
        .post(rpc_url)
        .json(&json!({
            "jsonrpc": "2.0",
            "method": "catalyst_networkInfo",
            "params": [],
            "id": 3
        }))
        .send()
        .await?;

    if response.status().is_success() {
        let result: Value = response.json().await?;
        println!("✅ Network info: {}", result);
    } else {
        println!("❌ Failed to get network info: {}", response.status());
    }

    Ok(())
}

// You can test this by adding to your CLI or creating a test binary
