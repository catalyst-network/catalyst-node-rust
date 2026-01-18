//! Basic Service Bus client example
//! 
//! This example demonstrates how to connect to the Catalyst Service Bus
//! and subscribe to blockchain events using the Rust client SDK.

use catalyst_service_bus::{
    client::{ServiceBusClient, ClientConfig},
    events::{EventFilter, EventType},
};
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::init();
    
    println!("🚀 Starting Catalyst Service Bus client example");
    
    // Configure the client
    let config = ClientConfig {
        url: "ws://localhost:8080/ws".to_string(),
        auth_token: None, // No authentication for this example
        api_key: None,
        connect_timeout: Duration::from_secs(10),
        ..Default::default()
    };
    
    // Create and connect the client
    let client = ServiceBusClient::new(config);
    
    println!("📡 Connecting to Service Bus...");
    client.connect().await?;
    println!("✅ Connected successfully!");
    
    // Subscribe to all transaction events
    let tx_filter = EventFilter::new()
        .with_event_types(vec![
            EventType::TransactionConfirmed,
            EventType::TransactionPending,
        ]);
    
    let tx_subscription_id = client.subscribe(tx_filter, |event| {
        println!("💰 Transaction Event: {:?} (Block: {})", 
                 event.event_type, event.block_number);
        
        if let Some(amount) = event.data.get("amount") {
            println!("   Amount: {}", amount);
        }
        
        if let Some(from) = event.data.get("from") {
            println!("   From: {}", from);
        }
        
        if let Some(to) = event.data.get("to") {
            println!("   To: {}", to);
        }
    }).await?;
    
    println!("📋 Subscribed to transactions (ID: {})", tx_subscription_id);
    
    // Subscribe to token transfer events
    let token_filter = EventFilter::new()
        .with_event_types(vec![EventType::TokenTransfer]);
    
    let token_subscription_id = client.subscribe(token_filter, |event| {
        println!("🪙 Token Transfer: Block {}", event.block_number);
        
        if let Some(symbol) = event.data.get("symbol") {
            println!("   Token: {}", symbol);
        }
        
        if let Some(amount) = event.data.get("amount") {
            println!("   Amount: {}", amount);
        }
    }).await?;
    
    println!("📋 Subscribed to token transfers (ID: {})", token_subscription_id);
    
    // Subscribe to block finalization events
    let block_filter = EventFilter::new()
        .with_event_types(vec![EventType::BlockFinalized]);
    
    let block_subscription_id = client.subscribe(block_filter, |event| {
        println!("🧱 New Block: {}", event.block_number);
        
        if let Some(block_hash) = event.data.get("block_hash") {
            println!("   Hash: {}", block_hash);
        }
    }).await?;
    
    println!("📋 Subscribed to block events (ID: {})", block_subscription_id);
    
    // Get some historical events
    println!("\n📜 Fetching recent transaction history...");
    let history_filter = EventFilter::new()
        .with_event_types(vec![EventType::TransactionConfirmed])
        .with_limit(10);
    
    match client.get_history(history_filter, Some(10)).await {
        Ok(events) => {
            println!("📊 Found {} recent transactions:", events.len());
            for event in events.iter().take(5) {
                println!("   Block {}: {:?}", event.block_number, event.event_type);
            }
        }
        Err(e) => {
            println!("⚠️  Failed to get history: {}", e);
        }
    }
    
    println!("\n🔄 Listening for events... (Press Ctrl+C to exit)");
    
    // Keep the client running and listening for events
    loop {
        let state = client.get_connection_state().await;
        println!("📊 Connection state: {:?}", state);
        
        sleep(Duration::from_secs(30)).await;
    }
}

// Example of a more advanced subscription with custom filtering
#[allow(dead_code)]
async fn advanced_subscription_example(client: &ServiceBusClient) -> Result<(), Box<dyn std::error::Error>> {
    use catalyst_utils::Address;
    
    // Watch a specific address for activity
    let watched_address: Address = [0x12; 21]; // Example address
    
    let address_filter = EventFilter::new()
        .with_event_types(vec![
            EventType::TransactionConfirmed,
            EventType::TokenTransfer,
            EventType::BalanceUpdate,
        ])
        .with_addresses(vec![watched_address]);
    
    let subscription_id = client.subscribe(address_filter, move |event| {
        println!("🎯 Activity detected for watched address!");
        println!("   Event: {:?}", event.event_type);
        println!("   Block: {}", event.block_number);
        println!("   Event ID: {}", event.id);
        
        // Check if this address is involved
        if event.addresses.contains(&watched_address) {
            println!("   ✅ Watched address is involved in this event");
        }
        
        // Log any metadata
        if let Some(gas_used) = event.metadata.gas_used {
            println!("   Gas used: {}", gas_used);
        }
        
        if let Some(status) = &event.metadata.status {
            println!("   Status: {:?}", status);
        }
    }).await?;
    
    println!("👀 Watching address for activity (ID: {})", subscription_id);
    Ok(())
}

// Example of using the simple client interface
#[allow(dead_code)]
async fn simple_client_example() -> Result<(), Box<dyn std::error::Error>> {
    use catalyst_service_bus::client::SimpleClient;
    
    let client = SimpleClient::new("ws://localhost:8080/ws");
    client.connect().await?;
    
    // Subscribe to token transfers using the simple interface
    let mut token_events = client.subscribe_to_token_transfers().await?;
    
    // Subscribe to transaction confirmations
    let mut tx_events = client.subscribe_to_transactions().await?;
    
    loop {
        tokio::select! {
            Ok(token_event) = token_events.recv() => {
                println!("🪙 Token transfer: {:?}", token_event.data);
            }
            Ok(tx_event) = tx_events.recv() => {
                println!("💰 Transaction: {:?}", tx_event.data);
            }
            else => break,
        }
    }
    
    Ok(())
}