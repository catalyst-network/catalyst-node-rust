//! Service Bus server example
//! 
//! This example demonstrates how to run the Catalyst Service Bus server
//! and inject sample blockchain events for testing.

use catalyst_service_bus::{
    ServiceBusConfig, ServiceBusServer,
    events::{events, EventType},
    server::EventProcessor,
};
use catalyst_utils::logging::{LogConfig, LogLevel, init_logger};
use std::{sync::Arc, time::Duration};
use tokio::{time::{interval, sleep}};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    let log_config = LogConfig {
        min_level: LogLevel::Info,
        json_format: false,
        console_output: true,
        ..Default::default()
    };
    init_logger(log_config)?;
    
    println!("🚀 Starting Catalyst Service Bus server");
    
    // Configure the server
    let mut config = ServiceBusConfig::default();
    config.port = 8080;
    config.websocket.enabled = true;
    config.rest_api.enabled = true;
    config.auth.enabled = false; // Disable auth for example
    config.event_filter.enable_replay = true;
    
    println!("📋 Server configuration:");
    println!("  - Host: {}", config.host);
    println!("  - Port: {}", config.port);
    println!("  - WebSocket: {}", config.websocket.enabled);
    println!("  - REST API: {}", config.rest_api.enabled);
    println!("  - Authentication: {}", config.auth.enabled);
    println!("  - Event Replay: {}", config.event_filter.enable_replay);
    
    // Create the server
    let server = Arc::new(ServiceBusServer::new(config).await?);
    
    // Create event processor for injecting test events
    let event_processor = EventProcessor::new(Arc::clone(&server));
    
    // Spawn background task to generate sample events
    let processor_clone = event_processor.clone();
    tokio::spawn(async move {
        generate_sample_events(processor_clone).await;
    });
    
    println!("✅ Server initialized, starting event generation");
    println!("📡 Server will be available at:");
    println!("  - WebSocket: ws://localhost:8080/ws");
    println!("  - REST API: http://localhost:8080/api/v1");
    println!("  - Health: http://localhost:8080/api/v1/health");
    println!("  - Docs: http://localhost:8080/api/v1/docs");
    println!("\n🔄 Starting server... (Press Ctrl+C to stop)");
    
    // Start the server (this will run indefinitely)
    server.start().await?;
    
    Ok(())
}

/// Generate sample blockchain events for testing
async fn generate_sample_events(processor: EventProcessor) {
    let mut block_number = 1u64;
    let mut transaction_counter = 1u64;
    
    // Generate events every 5 seconds
    let mut interval = interval(Duration::from_secs(5));
    
    loop {
        interval.tick().await;
        
        println!("📦 Generating sample events for block {}", block_number);
        
        // Generate block finalized event
        let block_event = events::block_finalized(
            block_number,
            [block_number as u8; 32], // Simple block hash
        );
        
        if let Err(e) = processor.process_event(block_event).await {
            eprintln!("❌ Failed to process block event: {}", e);
        }
        
        // Generate 2-5 transactions per block
        let tx_count = 2 + (block_number % 4);
        
        for i in 0..tx_count {
            // Generate transaction confirmed event
            let tx_hash = [transaction_counter as u8; 32];
            let from_addr = [(transaction_counter % 100) as u8; 21];
            let to_addr = [((transaction_counter + 50) % 100) as u8; 21];
            let amount = 1000 + (transaction_counter * 100);
            let gas_used = 21000 + (i * 5000);
            
            let tx_event = events::transaction_confirmed(
                block_number,
                tx_hash,
                from_addr,
                Some(to_addr),
                amount,
                gas_used,
            );
            
            if let Err(e) = processor.process_event(tx_event).await {
                eprintln!("❌ Failed to process transaction event: {}", e);
            }
            
            // Generate token transfer event (30% chance)
            if transaction_counter % 3 == 0 {
                let token_contract = [42u8; 21]; // Example token contract
                let token_amount = amount / 10; // Smaller token amounts
                
                let token_event = events::token_transfer(
                    block_number,
                    tx_hash,
                    token_contract,
                    from_addr,
                    to_addr,
                    token_amount,
                    Some("KAT".to_string()),
                );
                
                if let Err(e) = processor.process_event(token_event).await {
                    eprintln!("❌ Failed to process token event: {}", e);
                }
            }
            
            transaction_counter += 1;
            
            // Small delay between transactions
            sleep(Duration::from_millis(500)).await;
        }
        
        // Generate consensus phase events occasionally
        if block_number % 3 == 0 {
            let phases = ["Construction", "Campaigning", "Voting", "Synchronization"];
            
            for phase in phases {
                let consensus_event = events::consensus_phase(
                    phase,
                    block_number / 3, // Consensus cycle number
                    Some(format!("producer_{}", block_number % 5)),
                );
                
                if let Err(e) = processor.process_event(consensus_event).await {
                    eprintln!("❌ Failed to process consensus event: {}", e);
                }
                
                sleep(Duration::from_millis(200)).await;
            }
        }
        
        block_number += 1;
        
        // Print some statistics
        if block_number % 10 == 0 {
            let stats = Arc::clone(&processor.server).get_stats();
            println!("📊 Server Statistics:");
            println!("  - Total Subscriptions: {}", stats.total_subscriptions);
            println!("  - Active Connections: {}", stats.total_connections);
            println!("  - Replay Buffer Blocks: {}", stats.replay_buffer_blocks);
            println!("  - Replay Buffer Events: {}", stats.replay_buffer_events);
        }
    }
}

// Additional helper function to create more diverse events
#[allow(dead_code)]
async fn generate_diverse_events(processor: &EventProcessor, block: u64) -> Result<(), Box<dyn std::error::Error>> {
    use catalyst_service_bus::events::{BlockchainEvent, EventType, EventMetadata, TransactionStatus};
    use catalyst_utils::utils;
    use uuid::Uuid;
    
    // Network statistics event
    let network_stats = BlockchainEvent {
        id: Uuid::new_v4(),
        event_type: EventType::NetworkStats,
        block_number: block,
        transaction_hash: None,
        contract_address: None,
        addresses: Vec::new(),
        data: serde_json::json!({
            "peer_count": 150 + (block % 50),
            "total_supply": 1000000 + (block * 100),
            "circulating_supply": 800000 + (block * 90),
            "network_hash_rate": 1500000 + (block * 1000),
        }),
        metadata: EventMetadata::default(),
        timestamp: utils::current_timestamp(),
        topics: vec!["network".to_string(), "statistics".to_string()],
    };
    
    processor.process_event(network_stats).await?;
    
    // Peer connection events
    if block % 20 == 0 {
        let peer_connected = BlockchainEvent {
            id: Uuid::new_v4(),
            event_type: EventType::PeerConnected,
            block_number: block,
            transaction_hash: None,
            contract_address: None,
            addresses: Vec::new(),
            data: serde_json::json!({
                "peer_id": format!("peer_{}", block % 100),
                "ip_address": format!("192.168.1.{}", block % 255),
                "port": 8333,
                "user_agent": "catalyst-node/0.1.0",
            }),
            metadata: EventMetadata::default(),
            timestamp: utils::current_timestamp(),
            topics: vec!["network".to_string(), "peer".to_string()],
        };
        
        processor.process_event(peer_connected).await?;
    }
    
    // Contract events
    if block % 15 == 0 {
        let contract_event = BlockchainEvent {
            id: Uuid::new_v4(),
            event_type: EventType::ContractEvent,
            block_number: block,
            transaction_hash: Some([block as u8; 32]),
            contract_address: Some([99u8; 21]), // Example DeFi contract
            addresses: vec![[1u8; 21], [2u8; 21]], // Involved addresses
            data: serde_json::json!({
                "event_name": "Swap",
                "token_in": "KAT",
                "token_out": "USDC",
                "amount_in": 1000,
                "amount_out": 1950,
                "fee": 30,
            }),
            metadata: EventMetadata {
                gas_used: Some(150000),
                gas_price: Some(20),
                status: Some(TransactionStatus::Success),
                log_index: Some(0),
                custom: std::collections::HashMap::new(),
            },
            timestamp: utils::current_timestamp(),
            topics: vec!["defi".to_string(), "swap".to_string()],
        };
        
        processor.process_event(contract_event).await?;
    }
    
    Ok(())
}