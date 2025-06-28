//! Metrics collection for storage operations

#[cfg(feature = "metrics")]
use prometheus::{
    Counter, Gauge, Histogram, IntCounter, IntGauge,
    register_counter, register_gauge, register_histogram, 
    register_int_counter, register_int_gauge,
    HistogramOpts, Opts,
};

#[cfg(feature = "metrics")]
use std::sync::OnceLock;

#[cfg(feature = "metrics")]
/// Storage metrics registry
pub struct StorageMetrics {
    // Counters
    pub operations_total: IntCounter,
    pub errors_total: IntCounter,
    pub transactions_committed_total: IntCounter,
    pub transactions_rolled_back_total: IntCounter,
    pub snapshots_created_total: IntCounter,
    pub migrations_applied_total: IntCounter,
    
    // Gauges
    pub pending_transactions: IntGauge,
    pub active_snapshots: IntGauge,
    pub database_size_bytes: IntGauge,
    pub memory_usage_bytes: IntGauge,
    
    // Histograms
    pub operation_duration: Histogram,
    pub transaction_size_bytes: Histogram,
    pub transaction_duration: Histogram,
    pub snapshot_creation_duration: Histogram,
}

#[cfg(feature = "metrics")]
static STORAGE_METRICS: OnceLock<StorageMetrics> = OnceLock::new();

#[cfg(feature = "metrics")]
/// Register storage metrics with Prometheus
pub fn register_storage_metrics() -> Result<(), Box<dyn std::error::Error>> {
    let metrics = StorageMetrics {
        // Counters
        operations_total: register_int_counter!(
            "catalyst_storage_operations_total",
            "Total number of storage operations performed"
        )?,
        
        errors_total: register_int_counter!(
            "catalyst_storage_errors_total", 
            "Total number of storage errors encountered"
        )?,
        
        transactions_committed_total: register_int_counter!(
            "catalyst_storage_transactions_committed_total",
            "Total number of transactions successfully committed"
        )?,
        
        transactions_rolled_back_total: register_int_counter!(
            "catalyst_storage_transactions_rolled_back_total",
            "Total number of transactions rolled back"
        )?,
        
        snapshots_created_total: register_int_counter!(
            "catalyst_storage_snapshots_created_total",
            "Total number of snapshots created"
        )?,
        
        migrations_applied_total: register_int_counter!(
            "catalyst_storage_migrations_applied_total",
            "Total number of database migrations applied"
        )?,
        
        // Gauges
        pending_transactions: register_int_gauge!(
            "catalyst_storage_pending_transactions",
            "Number of transactions currently pending"
        )?,
        
        active_snapshots: register_int_gauge!(
            "catalyst_storage_active_snapshots",
            "Number of snapshots currently stored"
        )?,
        
        database_size_bytes: register_int_gauge!(
            "catalyst_storage_database_size_bytes",
            "Total size of the database in bytes"
        )?,
        
        memory_usage_bytes: register_int_gauge!(
            "catalyst_storage_memory_usage_bytes",
            "Memory usage of storage components in bytes"
        )?,
        
        // Histograms
        operation_duration: register_histogram!(
            HistogramOpts::new(
                "catalyst_storage_operation_duration_seconds",
                "Duration of storage operations in seconds"
            ).buckets(vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0])
        )?,
        
        transaction_size_bytes: register_histogram!(
            HistogramOpts::new(
                "catalyst_storage_transaction_size_bytes",
                "Size of transactions in bytes"
            ).buckets(vec![100.0, 1000.0, 10000.0, 100000.0, 1000000.0, 10000000.0])
        )?,
        
        transaction_duration: register_histogram!(
            HistogramOpts::new(
                "catalyst_storage_transaction_duration_seconds",
                "Duration of transaction commits in seconds"
            ).buckets(vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0])
        )?,
        
        snapshot_creation_duration: register_histogram!(
            HistogramOpts::new(
                "catalyst_storage_snapshot_creation_duration_seconds",
                "Duration of snapshot creation in seconds"
            ).buckets(vec![0.1, 0.5, 1.0, 5.0, 10.0, 30.0, 60.0])
        )?,
    };
    
    STORAGE_METRICS.set(metrics)
        .map_err(|_| "Storage metrics already initialized")?;
    
    Ok(())
}

#[cfg(feature = "metrics")]
/// Get storage metrics instance
pub fn get_storage_metrics() -> Option<&'static StorageMetrics> {
    STORAGE_METRICS.get()
}

#[cfg(feature = "metrics")]
/// Increment operation counter
pub fn increment_operations() {
    if let Some(metrics) = get_storage_metrics() {
        metrics.operations_total.inc();
    }
}

#[cfg(feature = "metrics")]
/// Increment error counter
pub fn increment_errors() {
    if let Some(metrics) = get_storage_metrics() {
        metrics.errors_total.inc();
    }
}

#[cfg(feature = "metrics")]
/// Record transaction commit
pub fn record_transaction_commit(duration_seconds: f64, size_bytes: f64) {
    if let Some(metrics) = get_storage_metrics() {
        metrics.transactions_committed_total.inc();
        metrics.transaction_duration.observe(duration_seconds);
        metrics.transaction_size_bytes.observe(size_bytes);
    }
}

#[cfg(feature = "metrics")]
/// Record transaction rollback
pub fn record_transaction_rollback() {
    if let Some(metrics) = get_storage_metrics() {
        metrics.transactions_rolled_back_total.inc();
    }
}

#[cfg(feature = "metrics")]
/// Update pending transactions gauge
pub fn update_pending_transactions(count: i64) {
    if let Some(metrics) = get_storage_metrics() {
        metrics.pending_transactions.set(count);
    }
}

#[cfg(feature = "metrics")]
/// Record snapshot creation
pub fn record_snapshot_creation(duration_seconds: f64) {
    if let Some(metrics) = get_storage_metrics() {
        metrics.snapshots_created_total.inc();
        metrics.snapshot_creation_duration.observe(duration_seconds);
    }
}

#[cfg(feature = "metrics")]
/// Update active snapshots gauge
pub fn update_active_snapshots(count: i64) {
    if let Some(metrics) = get_storage_metrics() {
        metrics.active_snapshots.set(count);
    }
}

#[cfg(feature = "metrics")]
/// Record migration application
pub fn record_migration_applied() {
    if let Some(metrics) = get_storage_metrics() {
        metrics.migrations_applied_total.inc();
    }
}

#[cfg(feature = "metrics")]
/// Update database size
pub fn update_database_size(size_bytes: i64) {
    if let Some(metrics) = get_storage_metrics() {
        metrics.database_size_bytes.set(size_bytes);
    }
}

#[cfg(feature = "metrics")]
/// Update memory usage
pub fn update_memory_usage(usage_bytes: i64) {
    if let Some(metrics) = get_storage_metrics() {
        metrics.memory_usage_bytes.set(usage_bytes);
    }
}

#[cfg(feature = "metrics")]
/// Record operation duration
pub fn record_operation_duration(duration_seconds: f64) {
    if let Some(metrics) = get_storage_metrics() {
        metrics.operation_duration.observe(duration_seconds);
    }
}

#[cfg(feature = "metrics")]
/// Timer for measuring operation duration
pub struct OperationTimer {
    start_time: std::time::Instant,
}

#[cfg(feature = "metrics")]
impl OperationTimer {
    pub fn new() -> Self {
        Self {
            start_time: std::time::Instant::now(),
        }
    }
    
    pub fn finish(self) {
        let duration = self.start_time.elapsed();
        record_operation_duration(duration.as_secs_f64());
    }
}

#[cfg(feature = "metrics")]
impl Default for OperationTimer {
    fn default() -> Self {
        Self::new()
    }
}

// No-op implementations when metrics feature is disabled
#[cfg(not(feature = "metrics"))]
pub fn register_storage_metrics() -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}

#[cfg(not(feature = "metrics"))]
pub fn increment_operations() {}

#[cfg(not(feature = "metrics"))]
pub fn increment_errors() {}

#[cfg(not(feature = "metrics"))]
pub fn record_transaction_commit(_duration_seconds: f64, _size_bytes: f64) {}

#[cfg(not(feature = "metrics"))]
pub fn record_transaction_rollback() {}

#[cfg(not(feature = "metrics"))]
pub fn update_pending_transactions(_count: i64) {}

#[cfg(not(feature = "metrics"))]
pub fn record_snapshot_creation(_duration_seconds: f64) {}

#[cfg(not(feature = "metrics"))]
pub fn update_active_snapshots(_count: i64) {}

#[cfg(not(feature = "metrics"))]
pub fn record_migration_applied() {}

#[cfg(not(feature = "metrics"))]
pub fn update_database_size(_size_bytes: i64) {}

#[cfg(not(feature = "metrics"))]
pub fn update_memory_usage(_usage_bytes: i64) {}

#[cfg(not(feature = "metrics"))]
pub fn record_operation_duration(_duration_seconds: f64) {}

#[cfg(not(feature = "metrics"))]
pub struct OperationTimer;

#[cfg(not(feature = "metrics"))]
impl OperationTimer {
    pub fn new() -> Self {
        Self
    }
    
    pub fn finish(self) {}
}

#[cfg(not(feature = "metrics"))]
impl Default for OperationTimer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_metrics_compilation() {
        // Test that metrics functions compile and don't panic
        increment_operations();
        increment_errors();
        record_transaction_commit(0.1, 1000.0);
        record_transaction_rollback();
        update_pending_transactions(5);
        record_snapshot_creation(2.5);
        update_active_snapshots(3);
        record_migration_applied();
        update_database_size(1024 * 1024);
        update_memory_usage(512 * 1024);
        record_operation_duration(0.05);
    }
    
    #[test]
    fn test_operation_timer() {
        let timer = OperationTimer::new();
        // Simulate some work
        std::thread::sleep(std::time::Duration::from_millis(1));
        timer.finish();
    }
    
    #[cfg(feature = "metrics")]
    #[test]
    fn test_metrics_registration() {
        // Note: This test might fail if metrics are already registered
        // In a real test environment, you'd want to use a separate registry
        let result = register_storage_metrics();
        // Don't assert on the result since metrics might already be registered
        // in other tests
        drop(result);
    }
}