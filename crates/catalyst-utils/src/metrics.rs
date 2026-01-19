// catalyst-utils/src/metrics.rs

use crate::error::{CatalystResult, CatalystError};
use crate::logging::{LogCategory, LogLevel, get_logger, LogValue};
use serde::{Serialize, Deserialize};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

/// Types of metrics supported by Catalyst
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MetricType {
    /// Simple counter that only increases
    Counter,
    /// Gauge that can increase or decrease
    Gauge,
    /// Histogram for measuring distributions
    Histogram,
    /// Timer for measuring durations
    Timer,
    /// Rate meter for measuring events per time unit
    Rate,
}

/// Metric categories specific to Catalyst Network
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MetricCategory {
    /// Consensus protocol metrics
    Consensus,
    /// Network performance metrics
    Network,
    /// Transaction processing metrics
    Transaction,
    /// Storage operation metrics
    Storage,
    /// Cryptographic operation metrics
    Crypto,
    /// Runtime execution metrics
    Runtime,
    /// Service bus metrics
    ServiceBus,
    /// Node health metrics
    System,
    /// Custom application metrics
    Application,
}

impl std::fmt::Display for MetricCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MetricCategory::Consensus => write!(f, "consensus"),
            MetricCategory::Network => write!(f, "network"),
            MetricCategory::Transaction => write!(f, "transaction"),
            MetricCategory::Storage => write!(f, "storage"),
            MetricCategory::Crypto => write!(f, "crypto"),
            MetricCategory::Runtime => write!(f, "runtime"),
            MetricCategory::ServiceBus => write!(f, "service_bus"),
            MetricCategory::System => write!(f, "system"),
            MetricCategory::Application => write!(f, "application"),
        }
    }
}

/// Individual metric sample with timestamp
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricSample {
    pub timestamp: u64,
    pub value: f64,
    pub labels: HashMap<String, String>,
}

/// Configuration for histogram buckets
#[derive(Debug, Clone)]
pub struct HistogramConfig {
    pub buckets: Vec<f64>,
    pub max_samples: usize,
}

impl Default for HistogramConfig {
    fn default() -> Self {
        Self {
            // Default buckets: 1ms, 5ms, 10ms, 50ms, 100ms, 500ms, 1s, 5s, 10s, 30s, 60s
            buckets: vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0, 10.0, 30.0, 60.0],
            max_samples: 1000,
        }
    }
}

/// Histogram data structure for measuring distributions
#[derive(Debug, Clone)]
pub struct Histogram {
    config: HistogramConfig,
    buckets: HashMap<String, u64>, // bucket_label -> count
    samples: VecDeque<f64>,
    sum: f64,
    count: u64,
}

impl Histogram {

    // Add these public methods
    pub fn count(&self) -> u64 {
        self.count
    }

    pub fn sum(&self) -> f64 {
        self.sum
    }
    
    pub fn new(config: HistogramConfig) -> Self {
        let mut buckets = HashMap::new();
        for bucket in &config.buckets {
            buckets.insert(format!("{:.3}", bucket), 0);
        }
        buckets.insert("inf".to_string(), 0);

        Self {
            config,
            buckets,
            samples: VecDeque::new(),
            sum: 0.0,
            count: 0,
        }
    }

    pub fn observe(&mut self, value: f64) {
        self.sum += value;
        self.count += 1;

        // Add to samples (with size limit)
        if self.samples.len() >= self.config.max_samples {
            self.samples.pop_front();
        }
        self.samples.push_back(value);

        // Update buckets
        for bucket in &self.config.buckets {
            if value <= *bucket {
                let key = format!("{:.3}", bucket);
                *self.buckets.get_mut(&key).unwrap() += 1;
            }
        }
        *self.buckets.get_mut("inf").unwrap() += 1;
    }

    pub fn percentile(&self, p: f64) -> Option<f64> {
        if self.samples.is_empty() {
            return None;
        }

        let mut sorted_samples: Vec<f64> = self.samples.iter().cloned().collect();
        sorted_samples.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let index = (p / 100.0 * (sorted_samples.len() - 1) as f64) as usize;
        Some(sorted_samples[index])
    }

    pub fn mean(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            self.sum / self.count as f64
        }
    }
}

/// Rate meter for measuring events per time unit
#[derive(Debug, Clone)]
pub struct RateMeter {
    samples: VecDeque<(Instant, f64)>,
    window_duration: Duration,
}

impl RateMeter {
    pub fn new(window_duration: Duration) -> Self {
        Self {
            samples: VecDeque::new(),
            window_duration,
        }
    }

    pub fn mark(&mut self, value: f64) {
        let now = Instant::now();
        self.samples.push_back((now, value));
        self.cleanup_old_samples(now);
    }

    fn cleanup_old_samples(&mut self, now: Instant) {
        while let Some((timestamp, _)) = self.samples.front() {
            if now.duration_since(*timestamp) > self.window_duration {
                self.samples.pop_front();
            } else {
                break;
            }
        }
    }

    pub fn rate(&mut self) -> f64 {
        let now = Instant::now();
        self.cleanup_old_samples(now);

        if self.samples.is_empty() {
            return 0.0;
        }

        let total: f64 = self.samples.iter().map(|(_, value)| value).sum();
        let window_seconds = self.window_duration.as_secs_f64();
        total / window_seconds
    }
}

/// Core metric data
#[derive(Debug, Clone)]
pub enum MetricValue {
    Counter(u64),
    Gauge(f64),
    Histogram(Histogram),
    Timer(Histogram),
    Rate(RateMeter),
}

/// Complete metric with metadata
#[derive(Debug, Clone)]
pub struct Metric {
    pub name: String,
    pub category: MetricCategory,
    pub metric_type: MetricType,
    pub description: String,
    pub value: MetricValue,
    pub labels: HashMap<String, String>,
    pub created_at: SystemTime,
    pub updated_at: SystemTime,
}

impl Metric {
    pub fn new(
        name: String,
        category: MetricCategory,
        metric_type: MetricType,
        description: String,
    ) -> Self {
        let now = SystemTime::now();
        let value = match metric_type {
            MetricType::Counter => MetricValue::Counter(0),
            MetricType::Gauge => MetricValue::Gauge(0.0),
            MetricType::Histogram => MetricValue::Histogram(Histogram::new(HistogramConfig::default())),
            MetricType::Timer => MetricValue::Timer(Histogram::new(HistogramConfig::default())),
            MetricType::Rate => MetricValue::Rate(RateMeter::new(Duration::from_secs(60))),
        };

        Self {
            name,
            category,
            metric_type,
            description,
            value,
            labels: HashMap::new(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn with_labels(mut self, labels: HashMap<String, String>) -> Self {
        self.labels = labels;
        self
    }

    pub fn increment_counter(&mut self, value: u64) -> CatalystResult<()> {
        match &mut self.value {
            MetricValue::Counter(ref mut counter) => {
                *counter += value;
                self.updated_at = SystemTime::now();
                Ok(())
            }
            _ => Err(CatalystError::Invalid(
                "Cannot increment non-counter metric".to_string(),
            )),
        }
    }

    pub fn set_gauge(&mut self, value: f64) -> CatalystResult<()> {
        match &mut self.value {
            MetricValue::Gauge(ref mut gauge) => {
                *gauge = value;
                self.updated_at = SystemTime::now();
                Ok(())
            }
            _ => Err(CatalystError::Invalid(
                "Cannot set gauge on non-gauge metric".to_string(),
            )),
        }
    }

    pub fn observe_histogram(&mut self, value: f64) -> CatalystResult<()> {
        match &mut self.value {
            MetricValue::Histogram(ref mut hist) => {
                hist.observe(value);
                self.updated_at = SystemTime::now();
                Ok(())
            }
            MetricValue::Timer(ref mut timer) => {
                timer.observe(value);
                self.updated_at = SystemTime::now();
                Ok(())
            }
            _ => Err(CatalystError::Invalid(
                "Cannot observe non-histogram metric".to_string(),
            )),
        }
    }

    pub fn mark_rate(&mut self, value: f64) -> CatalystResult<()> {
        match &mut self.value {
            MetricValue::Rate(ref mut rate) => {
                rate.mark(value);
                self.updated_at = SystemTime::now();
                Ok(())
            }
            _ => Err(CatalystError::Invalid(
                "Cannot mark rate on non-rate metric".to_string(),
            )),
        }
    }
}

/// Timer utility for measuring code execution time
pub struct Timer {
    start: Instant,
    metric_name: String,
    registry: Arc<Mutex<MetricsRegistry>>,
}

impl Timer {
    pub fn new(metric_name: String, registry: Arc<Mutex<MetricsRegistry>>) -> Self {
        Self {
            start: Instant::now(),
            metric_name,
            registry,
        }
    }

    pub fn stop(self) -> Duration {
        let duration = self.start.elapsed();
        
        if let Ok(mut registry) = self.registry.lock() {
            let _ = registry.observe_timer(&self.metric_name, duration.as_secs_f64());
        }

        duration
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        let duration = self.start.elapsed();
        
        if let Ok(mut registry) = self.registry.lock() {
            let _ = registry.observe_timer(&self.metric_name, duration.as_secs_f64());
        }
    }
}

/// Central metrics registry
pub struct MetricsRegistry {
    metrics: HashMap<String, Metric>,
    node_id: Option<String>,
    export_interval: Duration,
    last_export: Instant,
}

impl MetricsRegistry {
    pub fn new() -> Self {
        Self {
            metrics: HashMap::new(),
            node_id: None,
            export_interval: Duration::from_secs(60), // Export every minute
            last_export: Instant::now(),
        }
    }

    pub fn set_node_id(&mut self, node_id: String) {
        self.node_id = Some(node_id);
    }

    pub fn set_export_interval(&mut self, interval: Duration) {
        self.export_interval = interval;
    }

    /// Register a new metric
    pub fn register_metric(&mut self, metric: Metric) -> CatalystResult<()> {
        if self.metrics.contains_key(&metric.name) {
            return Err(CatalystError::Invalid(
                format!("Metric '{}' already registered", metric.name),
            ));
        }

        self.metrics.insert(metric.name.clone(), metric);
        Ok(())
    }

    /// Increment a counter metric
    pub fn increment_counter(&mut self, name: &str, value: u64) -> CatalystResult<()> {
        if let Some(metric) = self.metrics.get_mut(name) {
            metric.increment_counter(value)
        } else {
            Err(CatalystError::NotFound(format!("Metric '{}' not found", name)))
        }
    }

    /// Set a gauge metric
    pub fn set_gauge(&mut self, name: &str, value: f64) -> CatalystResult<()> {
        if let Some(metric) = self.metrics.get_mut(name) {
            metric.set_gauge(value)
        } else {
            Err(CatalystError::NotFound(format!("Metric '{}' not found", name)))
        }
    }

    /// Observe a histogram value
    pub fn observe_histogram(&mut self, name: &str, value: f64) -> CatalystResult<()> {
        if let Some(metric) = self.metrics.get_mut(name) {
            metric.observe_histogram(value)
        } else {
            Err(CatalystError::NotFound(format!("Metric '{}' not found", name)))
        }
    }

    /// Observe a timer value
    pub fn observe_timer(&mut self, name: &str, duration_seconds: f64) -> CatalystResult<()> {
        if let Some(metric) = self.metrics.get_mut(name) {
            metric.observe_histogram(duration_seconds)
        } else {
            Err(CatalystError::NotFound(format!("Metric '{}' not found", name)))
        }
    }

    /// Mark a rate metric
    pub fn mark_rate(&mut self, name: &str, value: f64) -> CatalystResult<()> {
        if let Some(metric) = self.metrics.get_mut(name) {
            metric.mark_rate(value)
        } else {
            Err(CatalystError::NotFound(format!("Metric '{}' not found", name)))
        }
    }

    /// Start a timer for a metric
    pub fn start_timer(&self, name: &str, registry: Arc<Mutex<MetricsRegistry>>) -> Timer {
        Timer::new(name.to_string(), registry)
    }

    /// Get all metrics
    pub fn get_metrics(&self) -> &HashMap<String, Metric> {
        &self.metrics
    }

    /// Export metrics (e.g., to logs or external systems)
    pub fn export_metrics(&mut self) -> CatalystResult<()> {
        let now = Instant::now();
        if now.duration_since(self.last_export) < self.export_interval {
            return Ok(());
        }

        if let Some(logger) = get_logger() {
            for (name, metric) in &self.metrics {
                let mut fields = vec![
                    ("metric_name", LogValue::String(name.clone())),
                    ("metric_type", LogValue::String(format!("{:?}", metric.metric_type))),
                    ("category", LogValue::String(metric.category.to_string())),
                ];

                // Add node ID if available
                if let Some(ref node_id) = self.node_id {
                    fields.push(("node_id", LogValue::String(node_id.clone())));
                }

                // Add metric-specific values
                match &metric.value {
                    MetricValue::Counter(value) => {
                        fields.push(("value", LogValue::Integer(*value as i64)));
                    }
                    MetricValue::Gauge(value) => {
                        fields.push(("value", LogValue::Float(*value)));
                    }
                    MetricValue::Histogram(hist) => {
                        fields.push(("count", LogValue::Integer(hist.count as i64)));
                        fields.push(("sum", LogValue::Float(hist.sum)));
                        fields.push(("mean", LogValue::Float(hist.mean())));
                        if let Some(p99) = hist.percentile(99.0) {
                            fields.push(("p99", LogValue::Float(p99)));
                        }
                    }
                    MetricValue::Timer(timer) => {
                        fields.push(("count", LogValue::Integer(timer.count as i64)));
                        fields.push(("mean_duration", LogValue::Float(timer.mean())));
                        if let Some(p99) = timer.percentile(99.0) {
                            fields.push(("p99_duration", LogValue::Float(p99)));
                        }
                    }
                    MetricValue::Rate(_rate) => {
                        // Note: We need mutable access to get rate, but this is read-only export
                        fields.push(("rate_info", LogValue::String("rate_metric".to_string())));
                    }
                }

                let field_refs: Vec<(&str, LogValue)> = fields.into_iter().collect();
                let _ = logger.log_with_fields(
                    LogLevel::Info,
                    LogCategory::Metrics,
                    &format!("Metric export: {}", name),
                    &field_refs,
                );
            }
        }

        self.last_export = now;
        Ok(())
    }
}

/// Global metrics registry
static GLOBAL_METRICS: std::sync::Mutex<Option<Arc<Mutex<MetricsRegistry>>>> = std::sync::Mutex::new(None);

/// Initialize the global metrics registry
pub fn init_metrics() -> CatalystResult<()> {
    let mut registry = GLOBAL_METRICS.lock()
        .map_err(|_| CatalystError::Runtime("Failed to acquire metrics lock".to_string()))?;
    
    if registry.is_some() {
        return Err(CatalystError::Runtime("Metrics registry already initialized".to_string()));
    }
    
    *registry = Some(Arc::new(Mutex::new(MetricsRegistry::new())));
    Ok(())
}

/// Get reference to global metrics registry
pub fn get_metrics_registry() -> Option<Arc<Mutex<MetricsRegistry>>> {
    GLOBAL_METRICS.lock().ok()?.clone()
}

/// Catalyst-specific metric definitions
pub mod catalyst_metrics {
    use super::*;

    /// Register all standard Catalyst metrics
    pub fn register_standard_metrics(registry: &mut MetricsRegistry) -> CatalystResult<()> {
        // Consensus metrics
        registry.register_metric(Metric::new(
            "consensus_cycles_total".to_string(),
            MetricCategory::Consensus,
            MetricType::Counter,
            "Total number of consensus cycles completed".to_string(),
        ))?;

        registry.register_metric(Metric::new(
            "consensus_phase_duration".to_string(),
            MetricCategory::Consensus,
            MetricType::Timer,
            "Duration of each consensus phase".to_string(),
        ))?;

        registry.register_metric(Metric::new(
            "consensus_producers_count".to_string(),
            MetricCategory::Consensus,
            MetricType::Gauge,
            "Number of producers in current cycle".to_string(),
        ))?;

        // Transaction metrics
        registry.register_metric(Metric::new(
            "transactions_processed_total".to_string(),
            MetricCategory::Transaction,
            MetricType::Counter,
            "Total number of transactions processed".to_string(),
        ))?;

        registry.register_metric(Metric::new(
            "transaction_validation_duration".to_string(),
            MetricCategory::Transaction,
            MetricType::Timer,
            "Time spent validating transactions".to_string(),
        ))?;

        registry.register_metric(Metric::new(
            "mempool_size".to_string(),
            MetricCategory::Transaction,
            MetricType::Gauge,
            "Current number of transactions in mempool".to_string(),
        ))?;

        // Network metrics
        registry.register_metric(Metric::new(
            "network_peers_connected".to_string(),
            MetricCategory::Network,
            MetricType::Gauge,
            "Number of connected peers".to_string(),
        ))?;

        registry.register_metric(Metric::new(
            "network_messages_sent_total".to_string(),
            MetricCategory::Network,
            MetricType::Counter,
            "Total number of network messages sent".to_string(),
        ))?;

        registry.register_metric(Metric::new(
            "network_message_latency".to_string(),
            MetricCategory::Network,
            MetricType::Histogram,
            "Network message round-trip latency".to_string(),
        ))?;

        // Storage metrics
        registry.register_metric(Metric::new(
            "storage_operations_total".to_string(),
            MetricCategory::Storage,
            MetricType::Counter,
            "Total number of storage operations".to_string(),
        ))?;

        registry.register_metric(Metric::new(
            "storage_operation_duration".to_string(),
            MetricCategory::Storage,
            MetricType::Timer,
            "Duration of storage operations".to_string(),
        ))?;

        // Crypto metrics
        registry.register_metric(Metric::new(
            "crypto_operations_total".to_string(),
            MetricCategory::Crypto,
            MetricType::Counter,
            "Total number of cryptographic operations".to_string(),
        ))?;

        registry.register_metric(Metric::new(
            "signature_verification_duration".to_string(),
            MetricCategory::Crypto,
            MetricType::Timer,
            "Time spent verifying signatures".to_string(),
        ))?;

        // System metrics
        registry.register_metric(Metric::new(
            "system_memory_usage".to_string(),
            MetricCategory::System,
            MetricType::Gauge,
            "Current memory usage in bytes".to_string(),
        ))?;

        registry.register_metric(Metric::new(
            "system_cpu_usage".to_string(),
            MetricCategory::System,
            MetricType::Gauge,
            "Current CPU usage percentage".to_string(),
        ))?;

        Ok(())
    }
}

/// Convenience macros for metrics
#[macro_export]
macro_rules! increment_counter {
    ($name:expr, $value:expr) => {
        if let Some(registry) = $crate::metrics::get_metrics_registry() {
            if let Ok(mut reg) = registry.lock() {
                let _ = reg.increment_counter($name, $value);
            }
        }
    };
}

#[macro_export]
macro_rules! set_gauge {
    ($name:expr, $value:expr) => {
        if let Some(registry) = $crate::metrics::get_metrics_registry() {
            if let Ok(mut reg) = registry.lock() {
                let _ = reg.set_gauge($name, $value);
            }
        }
    };
}

#[macro_export]
macro_rules! observe_histogram {
    ($name:expr, $value:expr) => {
        if let Some(registry) = $crate::metrics::get_metrics_registry() {
            if let Ok(mut reg) = registry.lock() {
                let _ = reg.observe_histogram($name, $value);
            }
        }
    };
}

#[macro_export]
macro_rules! time_operation {
    ($name:expr, $operation:expr) => {{
        let start = std::time::Instant::now();
        let result = $operation;
        let duration = start.elapsed().as_secs_f64();
        
        if let Some(registry) = $crate::metrics::get_metrics_registry() {
            if let Ok(mut reg) = registry.lock() {
                let _ = reg.observe_timer($name, duration);
            }
        }
        
        result
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_counter_metric() {
        let mut metric = Metric::new(
            "test_counter".to_string(),
            MetricCategory::System,
            MetricType::Counter,
            "Test counter".to_string(),
        );

        assert!(metric.increment_counter(5).is_ok());
        assert!(metric.increment_counter(3).is_ok());

        if let MetricValue::Counter(value) = metric.value {
            assert_eq!(value, 8);
        } else {
            panic!("Expected counter value");
        }
    }

    #[test]
    fn test_gauge_metric() {
        let mut metric = Metric::new(
            "test_gauge".to_string(),
            MetricCategory::System,
            MetricType::Gauge,
            "Test gauge".to_string(),
        );

        assert!(metric.set_gauge(42.5).is_ok());

        if let MetricValue::Gauge(value) = metric.value {
            assert_eq!(value, 42.5);
        } else {
            panic!("Expected gauge value");
        }
    }

    #[test]
    fn test_histogram_metric() {
        let mut metric = Metric::new(
            "test_histogram".to_string(),
            MetricCategory::System,
            MetricType::Histogram,
            "Test histogram".to_string(),
        );

        assert!(metric.observe_histogram(1.0).is_ok());
        assert!(metric.observe_histogram(2.0).is_ok());
        assert!(metric.observe_histogram(3.0).is_ok());

        if let MetricValue::Histogram(ref hist) = metric.value {
            assert_eq!(hist.count, 3);
            assert_eq!(hist.sum, 6.0);
            assert_eq!(hist.mean(), 2.0);
        } else {
            panic!("Expected histogram value");
        }
    }

    #[test]
    fn test_rate_meter() {
        let mut rate = RateMeter::new(Duration::from_secs(60));
        
        rate.mark(10.0);
        rate.mark(20.0);
        
        let current_rate = rate.rate();
        assert!(current_rate > 0.0);
    }

    #[test]
    fn test_metrics_registry() {
        let mut registry = MetricsRegistry::new();
        
        let metric = Metric::new(
            "test_metric".to_string(),
            MetricCategory::System,
            MetricType::Counter,
            "Test metric".to_string(),
        );

        assert!(registry.register_metric(metric).is_ok());
        assert!(registry.increment_counter("test_metric", 5).is_ok());
        
        // Test duplicate registration
        let duplicate = Metric::new(
            "test_metric".to_string(),
            MetricCategory::System,
            MetricType::Counter,
            "Duplicate".to_string(),
        );
        assert!(registry.register_metric(duplicate).is_err());
    }

    #[test]
    fn test_histogram_percentiles() {
        let mut hist = Histogram::new(HistogramConfig::default());
        
        // Add some test data
        for i in 1..=100 {
            hist.observe(i as f64);
        }
        
        assert_eq!(hist.percentile(50.0), Some(50.0));
        assert_eq!(hist.percentile(99.0), Some(99.0));
        assert_eq!(hist.percentile(0.0), Some(1.0));
    }
}