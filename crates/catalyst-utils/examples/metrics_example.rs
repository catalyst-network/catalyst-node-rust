// examples/metrics_example.rs

use catalyst_utils::*;
use catalyst_utils::metrics::*;
use std::time::Duration;
use std::thread;

fn main() -> CatalystResult<()> {
    println!("=== Catalyst Metrics System Example ===\n");

    // 1. Initialize metrics system
    println!("1. Initializing metrics system...");
    init_metrics()?;
    
    let registry = get_metrics_registry()
        .ok_or_else(|| CatalystError::Internal("Failed to get metrics registry".to_string()))?;
    
    {
        let mut reg = registry.lock().unwrap();
        reg.set_node_id("metrics_example_node".to_string());
        reg.set_export_interval(Duration::from_secs(5)); // Export every 5 seconds for demo
    }
    
    println!("   ✓ Metrics system initialized\n");

    // 2. Register standard Catalyst metrics
    println!("2. Registering standard Catalyst metrics...");
    {
        let mut reg = registry.lock().unwrap();
        catalyst_metrics::register_standard_metrics(&mut reg)?;
    }
    println!("   ✓ Standard metrics registered\n");

    // 3. Basic metric usage with macros
    println!("3. Basic metric usage:");
    
    // Counter metrics
    println!("   Using counters:");
    increment_counter!("transactions_processed_total", 1);
    increment_counter!("transactions_processed_total", 5);
    increment_counter!("network_messages_sent_total", 10);
    
    // Gauge metrics
    println!("   Using gauges:");
    set_gauge!("mempool_size", 150.0);
    set_gauge!("network_peers_connected", 8.0);
    set_gauge!("consensus_producers_count", 5.0);
    
    // Histogram metrics
    println!("   Using histograms:");
    observe_histogram!("network_message_latency", 0.025); // 25ms
    observe_histogram!("network_message_latency", 0.015); // 15ms
    observe_histogram!("network_message_latency", 0.045); // 45ms
    observe_histogram!("network_message_latency", 0.030); // 30ms
    
    println!();

    // 4. Timer usage
    println!("4. Timer usage:");
    
    // Manual timing
    {
        let reg = registry.lock().unwrap();
        let timer = reg.start_timer("consensus_phase_duration", registry.clone());
        
        // Simulate consensus phase work
        thread::sleep(Duration::from_millis(100));
        
        let duration = timer.stop();
        println!("   Manual timer recorded: {:?}", duration);
    }
    
    // Automatic timing with macro
    let result = time_operation!("transaction_validation_duration", {
        // Simulate transaction validation
        thread::sleep(Duration::from_millis(50));
        "validation_success"
    });
    println!("   Automatic timer result: {}", result);
    
    // Multiple timed operations
    for i in 0..5 {
        time_operation!("storage_operation_duration", {
            thread::sleep(Duration::from_millis(20 + i * 5));
        });
    }
    println!();

    // 5. Custom metrics
    println!("5. Custom metrics:");
    
    {
        let mut reg = registry.lock().unwrap();
        
        // Register a custom counter
        let custom_counter = Metric::new(
            "custom_operations_total".to_string(),
            MetricCategory::Application,
            MetricType::Counter,
            "Total number of custom operations".to_string(),
        );
        reg.register_metric(custom_counter)?;
        
        // Register a custom histogram
        let custom_histogram = Metric::new(
            "custom_request_size".to_string(),
            MetricCategory::Application,
            MetricType::Histogram,
            "Size distribution of custom requests".to_string(),
        );
        reg.register_metric(custom_histogram)?;
        
        // Use custom metrics
        reg.increment_counter("custom_operations_total", 3)?;
        reg.observe_histogram("custom_request_size", 1024.0)?;
        reg.observe_histogram("custom_request_size", 2048.0)?;
        reg.observe_histogram("custom_request_size", 512.0)?;
    }
    
    println!("   ✓ Custom metrics registered and used\n");

    // 6. Simulate Catalyst Network operations
    println!("6. Simulating Catalyst Network operations:");
    
    // Simulate consensus cycles
    for cycle in 1..=3 {
        println!("   Processing consensus cycle {}...", cycle);
        
        // Construction phase
        time_operation!("consensus_phase_duration", {
            thread::sleep(Duration::from_millis(80));
            increment_counter!("consensus_cycles_total", 1);
        });
        
        // Update producer count
        set_gauge!("consensus_producers_count", (3 + cycle) as f64);
        
        // Process transactions
        let tx_count = 15 + cycle * 5;
        for tx in 1..=tx_count {
            time_operation!("transaction_validation_duration", {
                thread::sleep(Duration::from_millis(2));
            });
            
            // Vary transaction sizes
            let tx_size = 200.0 + (tx as f64 * 50.0) % 800.0;
            observe_histogram!("transaction_size_bytes", tx_size);
        }
        
        increment_counter!("transactions_processed_total", tx_count);
        set_gauge!("mempool_size", (200 - tx_count * 3) as f64);
        
        // Network activity
        let messages = 20 + cycle * 3;
        increment_counter!("network_messages_sent_total", messages);
        
        for _ in 0..messages {
            let latency = 0.010 + (rand::random::<f64>() * 0.040); // 10-50ms
            observe_histogram!("network_message_latency", latency);
        }
        
        thread::sleep(Duration::from_millis(100));
    }
    println!();

    // 7. Advanced metric analysis
    println!("7. Advanced metric analysis:");
    
    {
        let reg = registry.lock().unwrap();
        let metrics = reg.get_metrics();
        
        for (name, metric) in metrics {
            match &metric.value {
                MetricValue::Counter(value) => {
                    println!("   📊 {}: {} (Counter)", name, value);
                }
                MetricValue::Gauge(value) => {
                    println!("   📈 {}: {:.2} (Gauge)", name, value);
                }
                MetricValue::Histogram(hist) => {
                    println!("   📉 {}: count={}, mean={:.4}, p99={:.4} (Histogram)", 
                        name, 
                        hist.count(),
                        hist.mean(),
                        hist.percentile(99.0).unwrap_or(0.0)
                    );
                }
                MetricValue::Timer(timer) => {
                    println!("   ⏱️  {}: count={}, mean={:.4}s, p99={:.4}s (Timer)", 
                        name, 
                        timer.count(),
                        timer.mean(),
                        timer.percentile(99.0).unwrap_or(0.0)
                    );
                }
                MetricValue::Rate(_) => {
                    println!("   📊 {}: (Rate meter)", name);
                }
            }
        }
    }
    println!();

    // 8. Rate meter example
    println!("8. Rate meter example:");
    
    {
        let mut reg = registry.lock().unwrap();
        
        // Register a rate meter
        let rate_metric = Metric::new(
            "requests_per_second".to_string(),
            MetricCategory::Application,
            MetricType::Rate,
            "Request rate over time".to_string(),
        );
        reg.register_metric(rate_metric)?;
        
        // Simulate varying request rates
        for burst in 1..=3 {
            println!("   Burst {}: Generating requests...", burst);
            
            for _ in 0..10 {
                reg.mark_rate("requests_per_second", 1.0)?;
                thread::sleep(Duration::from_millis(50));
            }
            
            thread::sleep(Duration::from_millis(200));
        }
    }
    println!();

    // 9. Export metrics
    println!("9. Exporting metrics:");
    {
        let mut reg = registry.lock().unwrap();
        reg.export_metrics()?;
    }
    println!("   ✓ Metrics exported to logs\n");

    // 10. Memory and performance considerations
    println!("10. Performance demonstration:");
    
    let start = std::time::Instant::now();
    
    // High-frequency metric updates
    for i in 0..1000 {
        increment_counter!("transactions_processed_total", 1);
        if i % 100 == 0 {
            set_gauge!("mempool_size", (i / 10) as f64);
        }
        if i % 50 == 0 {
            observe_histogram!("network_message_latency", 0.020 + (i as f64 / 50000.0));
        }
    }
    
    let duration = start.elapsed();
    println!("   1000 metric operations completed in: {:?}", duration);
    println!("   Average time per operation: {:?}", duration / 1000);
    println!();

    println!("=== Metrics example completed successfully! ===");
    Ok(())
}

// Example of a custom metrics collector for Catalyst components
struct ConsensusMetricsCollector {
    registry: std::sync::Arc<std::sync::Mutex<MetricsRegistry>>,
    cycle_count: u64,
}

impl ConsensusMetricsCollector {
    pub fn new(registry: std::sync::Arc<std::sync::Mutex<MetricsRegistry>>) -> Self {
        Self {
            registry,
            cycle_count: 0,
        }
    }
    
    pub fn record_consensus_cycle(&mut self, phase_durations: &[f64]) -> CatalystResult<()> {
        self.cycle_count += 1;
        
        increment_counter!("consensus_cycles_total", 1);
        
        let phase_names = ["construction", "campaigning", "voting", "synchronization"];
        for (i, &duration) in phase_durations.iter().enumerate() {
            if let Some(phase) = phase_names.get(i) {
                let mut reg = self.registry.lock().unwrap();
                reg.observe_timer(&format!("consensus_{}_phase_duration", phase), duration)?;
            }
        }
        
        Ok(())
    }
    
    pub fn record_producer_selection(&self, producer_count: usize, selection_time: f64) -> CatalystResult<()> {
        set_gauge!("consensus_producers_count", producer_count as f64);
        
        let mut reg = self.registry.lock().unwrap();
        reg.observe_timer("producer_selection_duration", selection_time)?;
        
        Ok(())
    }
}

// Example of network metrics integration
fn simulate_network_activity() -> CatalystResult<()> {
    println!("   Simulating network activity...");
    
    // Simulate peer connections
    for peer_id in 1..=5 {
        increment_counter!("network_connections_total", 1);
        set_gauge!("network_peers_connected", peer_id as f64);
        
        // Simulate message exchange with varying latency
        for _ in 0..10 {
            let latency = 0.005 + (rand::random::<f64>() * 0.030); // 5-35ms
            observe_histogram!("network_message_latency", latency);
            
            increment_counter!("network_messages_sent_total", 1);
            
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_examples() {
        // Test that our metrics examples work
        assert!(main().is_ok());
    }
    
    #[test]
    fn test_consensus_metrics_collector() {
        init_metrics().ok(); // May already be initialized
        let registry = get_metrics_registry().unwrap();
        
        let mut collector = ConsensusMetricsCollector::new(registry);
        let phase_durations = [0.080, 0.120, 0.090, 0.050]; // Construction, campaigning, voting, sync
        
        assert!(collector.record_consensus_cycle(&phase_durations).is_ok());
        assert!(collector.record_producer_selection(5, 0.025).is_ok());
    }
    
    #[test]
    fn test_network_simulation() {
        init_metrics().ok(); // May already be initialized
        assert!(simulate_network_activity().is_ok());
    }
}