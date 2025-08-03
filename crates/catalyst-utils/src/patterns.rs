// catalyst-utils/src/patterns.rs

use crate::logging::{CatalystLogger, LogLevel, LogCategory, LogValue};
use crate::{CatalystResult};
use std::collections::HashMap;

/// Log consensus phase transitions
pub fn log_consensus_phase(
    logger: &CatalystLogger,
    phase: &str,
    cycle: u64,
    node_id: &str,
    additional_fields: Option<HashMap<String, LogValue>>,
) -> CatalystResult<()> {
    let mut fields = HashMap::new();
    fields.insert("phase".to_string(), LogValue::String(phase.to_string()));
    fields.insert("cycle".to_string(), LogValue::Integer(cycle as i64));
    fields.insert("node_id".to_string(), LogValue::String(node_id.to_string()));
    
    if let Some(extra) = additional_fields {
        fields.extend(extra);
    }
    
    logger.log(
        LogLevel::Info,
        LogCategory::Consensus,
        format!("Consensus phase transition: {}", phase),
        fields,
    )
}

/// Log transaction processing
pub fn log_transaction(
    logger: &CatalystLogger,
    tx_id: &str,
    status: &str,
    amount: Option<u64>,
) -> CatalystResult<()> {
    let mut fields = HashMap::new();
    fields.insert("transaction_id".to_string(), LogValue::String(tx_id.to_string()));
    fields.insert("status".to_string(), LogValue::String(status.to_string()));
    
    if let Some(amt) = amount {
        fields.insert("amount".to_string(), LogValue::Integer(amt as i64));
    }
    
    logger.log(
        LogLevel::Info,
        LogCategory::Transaction,
        format!("Transaction {}: {}", tx_id, status),
        fields,
    )
}

/// Log network events
pub fn log_network_event(
    logger: &CatalystLogger,
    event_type: &str,
    peer_id: Option<&str>,
    details: Option<HashMap<String, LogValue>>,
) -> CatalystResult<()> {
    let mut fields = HashMap::new();
    fields.insert("event_type".to_string(), LogValue::String(event_type.to_string()));
    
    if let Some(peer) = peer_id {
        fields.insert("peer_id".to_string(), LogValue::String(peer.to_string()));
    }
    
    if let Some(extra) = details {
        fields.extend(extra);
    }
    
    logger.log(
        LogLevel::Info,
        LogCategory::Network,
        format!("Network event: {}", event_type),
        fields,
    )
}