use crate::error::{ConfigError, ConfigResult};
use serde::{Deserialize, Serialize};

/// Consensus algorithm configuration for the 4-phase collaborative process
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusConfig {
    /// Total cycle duration in milliseconds (Δtp + Δtc + Δtv + Δts)
    pub cycle_duration_ms: u64,

    /// Construction phase duration in milliseconds (Δtp)
    pub construction_phase_ms: u64,

    /// Campaigning phase duration in milliseconds (Δtc)
    pub campaigning_phase_ms: u64,

    /// Voting phase duration in milliseconds (Δtv)
    pub voting_phase_ms: u64,

    /// Synchronization phase duration in milliseconds (Δts)
    pub synchronization_phase_ms: u64,

    /// Number of producer nodes selected per cycle (P)
    pub producer_count: usize,

    /// Supermajority threshold for voting (typically 0.67 for >67%)
    pub supermajority_threshold: f64,

    /// Statistical confidence threshold (typically 0.99999 for 99.999%)
    pub statistical_confidence: f64,

    /// Minimum number of producers required for consensus
    pub min_producers: usize,

    /// Maximum transaction batch size per producer
    pub max_transaction_batch_size: usize,

    /// Worker pool management settings
    pub worker_pool: WorkerPoolConfig,

    /// Producer selection settings
    pub producer_selection: ProducerSelectionConfig,

    /// Consensus security settings
    pub security: ConsensusSecurityConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerPoolConfig {
    /// Maximum size of the worker pool
    pub max_worker_pool_size: usize,

    /// Minimum worker score to be eligible for producer selection
    pub min_worker_score: f64,

    /// Worker pass duration in milliseconds
    pub worker_pass_duration_ms: u64,

    /// Resource proof requirements
    pub resource_proof_required: bool,

    /// Scoring algorithm parameters
    pub scoring: ScoringConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoringConfig {
    /// Weight for network uptime in scoring (0.0 to 1.0)
    pub uptime_weight: f64,

    /// Weight for resource availability in scoring (0.0 to 1.0)
    pub resource_weight: f64,

    /// Weight for participation history in scoring (0.0 to 1.0)
    pub participation_weight: f64,

    /// Decay factor for historical performance (0.0 to 1.0)
    pub history_decay_factor: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProducerSelectionConfig {
    /// Use pseudo-random selection based on previous cycle Merkle root
    pub use_pseudo_random_selection: bool,

    /// Rotation strategy for producer selection
    pub rotation_strategy: RotationStrategy,

    /// Minimum time between producer selections for same node (ms)
    pub min_selection_interval_ms: u64,

    /// Geographic distribution preference
    pub geographic_distribution_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RotationStrategy {
    /// Purely random based on XOR with cycle seed
    PureRandom,
    /// Weighted random considering node performance
    WeightedRandom,
    /// Round-robin with randomization
    RoundRobinRandom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusSecurityConfig {
    /// Maximum allowed Byzantine nodes percentage (0.0 to 1.0)
    pub max_byzantine_ratio: f64,

    /// Enable additional validation checks
    pub strict_validation: bool,

    /// Timeout for receiving producer quantities (ms)
    pub producer_quantity_timeout_ms: u64,

    /// Timeout for receiving candidate updates (ms)
    pub candidate_timeout_ms: u64,

    /// Timeout for receiving votes (ms)
    pub vote_timeout_ms: u64,

    /// Maximum allowed clock drift in milliseconds
    pub max_clock_drift_ms: u64,

    /// Enable anti-spam measures
    pub anti_spam_enabled: bool,
}

impl ConsensusConfig {
    /// Create development consensus configuration (fast cycles)
    pub fn devnet() -> Self {
        Self {
            cycle_duration_ms: 15000, // 15 seconds
            construction_phase_ms: 3750,
            campaigning_phase_ms: 3750,
            voting_phase_ms: 3750,
            synchronization_phase_ms: 3750,
            producer_count: 5,
            supermajority_threshold: 0.67,
            statistical_confidence: 0.99,
            min_producers: 3,
            max_transaction_batch_size: 100,
            worker_pool: WorkerPoolConfig {
                max_worker_pool_size: 20,
                min_worker_score: 0.5,
                worker_pass_duration_ms: 3600000, // 1 hour
                resource_proof_required: false,
                scoring: ScoringConfig {
                    uptime_weight: 0.3,
                    resource_weight: 0.3,
                    participation_weight: 0.4,
                    history_decay_factor: 0.95,
                },
            },
            producer_selection: ProducerSelectionConfig {
                use_pseudo_random_selection: true,
                rotation_strategy: RotationStrategy::WeightedRandom,
                min_selection_interval_ms: 60000, // 1 minute
                geographic_distribution_enabled: false,
            },
            security: ConsensusSecurityConfig {
                max_byzantine_ratio: 0.33,
                strict_validation: false,
                producer_quantity_timeout_ms: 2000,
                candidate_timeout_ms: 2000,
                vote_timeout_ms: 2000,
                max_clock_drift_ms: 1000,
                anti_spam_enabled: false,
            },
        }
    }

    /// Create test network consensus configuration
    pub fn testnet() -> Self {
        Self {
            cycle_duration_ms: 30000, // 30 seconds
            construction_phase_ms: 7500,
            campaigning_phase_ms: 7500,
            voting_phase_ms: 7500,
            synchronization_phase_ms: 7500,
            producer_count: 50,
            supermajority_threshold: 0.67,
            statistical_confidence: 0.999,
            min_producers: 10,
            max_transaction_batch_size: 1000,
            worker_pool: WorkerPoolConfig {
                max_worker_pool_size: 200,
                min_worker_score: 0.7,
                worker_pass_duration_ms: 7200000, // 2 hours
                resource_proof_required: true,
                scoring: ScoringConfig {
                    uptime_weight: 0.3,
                    resource_weight: 0.3,
                    participation_weight: 0.4,
                    history_decay_factor: 0.98,
                },
            },
            producer_selection: ProducerSelectionConfig {
                use_pseudo_random_selection: true,
                rotation_strategy: RotationStrategy::WeightedRandom,
                min_selection_interval_ms: 300000, // 5 minutes
                geographic_distribution_enabled: true,
            },
            security: ConsensusSecurityConfig {
                max_byzantine_ratio: 0.33,
                strict_validation: true,
                producer_quantity_timeout_ms: 5000,
                candidate_timeout_ms: 5000,
                vote_timeout_ms: 5000,
                max_clock_drift_ms: 500,
                anti_spam_enabled: true,
            },
        }
    }

    /// Create main network consensus configuration (production)
    pub fn mainnet() -> Self {
        Self {
            cycle_duration_ms: 60000, // 60 seconds
            construction_phase_ms: 15000,
            campaigning_phase_ms: 15000,
            voting_phase_ms: 15000,
            synchronization_phase_ms: 15000,
            producer_count: 200,
            supermajority_threshold: 0.67,
            statistical_confidence: 0.99999,
            min_producers: 50,
            max_transaction_batch_size: 5000,
            worker_pool: WorkerPoolConfig {
                max_worker_pool_size: 1000,
                min_worker_score: 0.8,
                worker_pass_duration_ms: 14400000, // 4 hours
                resource_proof_required: true,
                scoring: ScoringConfig {
                    uptime_weight: 0.4,
                    resource_weight: 0.3,
                    participation_weight: 0.3,
                    history_decay_factor: 0.99,
                },
            },
            producer_selection: ProducerSelectionConfig {
                use_pseudo_random_selection: true,
                rotation_strategy: RotationStrategy::WeightedRandom,
                min_selection_interval_ms: 1800000, // 30 minutes
                geographic_distribution_enabled: true,
            },
            security: ConsensusSecurityConfig {
                max_byzantine_ratio: 0.33,
                strict_validation: true,
                producer_quantity_timeout_ms: 10000,
                candidate_timeout_ms: 10000,
                vote_timeout_ms: 10000,
                max_clock_drift_ms: 200,
                anti_spam_enabled: true,
            },
        }
    }

    /// Validate consensus configuration
    pub fn validate(&self) -> ConfigResult<()> {
        // Validate phase durations sum to cycle duration
        let total_phases = self.construction_phase_ms
            + self.campaigning_phase_ms
            + self.voting_phase_ms
            + self.synchronization_phase_ms;

        if total_phases != self.cycle_duration_ms {
            return Err(ConfigError::ValidationFailed(format!(
                "Phase durations ({}) don't sum to cycle duration ({})",
                total_phases, self.cycle_duration_ms
            )));
        }

        // Validate producer count
        if self.producer_count == 0 {
            return Err(ConfigError::ValidationFailed(
                "Producer count must be greater than 0".to_string(),
            ));
        }

        if self.producer_count < self.min_producers {
            return Err(ConfigError::ValidationFailed(
                "Producer count cannot be less than min_producers".to_string(),
            ));
        }

        // Validate thresholds
        if self.supermajority_threshold <= 0.5 || self.supermajority_threshold > 1.0 {
            return Err(ConfigError::ValidationFailed(
                "Supermajority threshold must be between 0.5 and 1.0".to_string(),
            ));
        }

        if self.statistical_confidence <= 0.0 || self.statistical_confidence >= 1.0 {
            return Err(ConfigError::ValidationFailed(
                "Statistical confidence must be between 0.0 and 1.0".to_string(),
            ));
        }

        // Validate security settings
        if self.security.max_byzantine_ratio >= 0.5 {
            return Err(ConfigError::ValidationFailed(
                "Byzantine ratio must be less than 0.5 for security".to_string(),
            ));
        }

        // Validate worker pool settings
        if self.worker_pool.max_worker_pool_size < self.producer_count {
            return Err(ConfigError::ValidationFailed(
                "Worker pool size must be at least as large as producer count".to_string(),
            ));
        }

        // Validate scoring weights sum to 1.0
        let scoring = &self.worker_pool.scoring;
        let weight_sum =
            scoring.uptime_weight + scoring.resource_weight + scoring.participation_weight;
        if (weight_sum - 1.0).abs() > 0.001 {
            return Err(ConfigError::ValidationFailed(
                "Scoring weights must sum to 1.0".to_string(),
            ));
        }

        Ok(())
    }
}
