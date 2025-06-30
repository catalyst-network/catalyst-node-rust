use catalyst_utils::metrics::*;
use prometheus::{Registry, opts};

pub fn register_network_metrics(registry: &mut Registry) -> catalyst_utils::CatalystResult<()> {
    use prometheus::{register_counter_with_registry, register_gauge_with_registry, register_histogram_with_registry};
    
    // Connection metrics
    register_counter_with_registry!(
        opts!("network_connections_established_total", "Total connections established"),
        registry
    )?;
    
    register_counter_with_registry!(
        opts!("network_connections_closed_total", "Total connections closed"),
        registry
    )?;
    
    register_gauge_with_registry!(
        opts!("network_connected_peers", "Number of connected peers"),
        registry
    )?;
    
    // Message metrics
    register_counter_with_registry!(
        opts!("network_messages_sent_total", "Total messages sent"),
        registry
    )?;
    
    register_counter_with_registry!(
        opts!("network_messages_received_total", "Total messages received"),
        registry
    )?;
    
    register_counter_with_registry!(
        opts!("network_bytes_sent_total", "Total bytes sent"),
        registry
    )?;
    
    register_counter_with_registry!(
        opts!("network_bytes_received_total", "Total bytes received"),
        registry
    )?;
    
    // Gossip metrics
    register_counter_with_registry!(
        opts!("network_gossip_published_total", "Total gossip messages published"),
        registry
    )?;
    
    register_counter_with_registry!(
        opts!("network_gossip_received_total", "Total gossip messages received"),
        registry
    )?;
    
    // Discovery metrics
    register_counter_with_registry!(
        opts!("network_peers_discovered_total", "Total peers discovered"),
        registry
    )?;
    
    register_counter_with_registry!(
        opts!("network_bootstrap_attempts_total", "Total bootstrap attempts"),
        registry
    )?;
    
    register_histogram_with_registry!(
        prometheus::histogram_opts!("network_bootstrap_duration_seconds", "Bootstrap duration"),
        registry
    )?;
    
    // Health metrics
    register_gauge_with_registry!(
        opts!("network_health_connection", "Connection health score"),
        registry
    )?;
    
    register_gauge_with_registry!(
        opts!("network_health_bandwidth", "Bandwidth health score"),
        registry
    )?;
    
    register_gauge_with_registry!(
        opts!("network_health_latency", "Latency health score"),
        registry
    )?;
    
    // Security metrics
    register_counter_with_registry!(
        opts!("network_security_blocked_connections_total", "Total blocked connections"),
        registry
    )?;
    
    register_counter_with_registry!(
        opts!("network_security_rejected_messages_total", "Total rejected messages"),
        registry
    )?;
    
    // Bandwidth metrics
    register_gauge_with_registry!(
        opts!("network_bandwidth_upload_rate", "Upload rate in bytes/sec"),
        registry
    )?;
    
    register_gauge_with_registry!(
        opts!("network_bandwidth_download_rate", "Download rate in bytes/sec"),
        registry
    )?;
    
    // Reputation metrics
    register_gauge_with_registry!(
        opts!("network_reputation_average_score", "Average reputation score"),
        registry
    )?;
    
    register_gauge_with_registry!(
        opts!("network_reputation_tracked_peers", "Number of tracked peers"),
        registry
    )?;
    
    Ok(())
}