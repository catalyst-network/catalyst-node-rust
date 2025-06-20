//! Consensus configuration structures

use serde::{Deserialize, Serialize};
use std::time::Duration;
use crate::ConfigError;

/// Consensus mechanism configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusConfig {
    /// Number of producers selected per ledger cycle
    pub producer_count: usize,
    /// Total number of nodes in worker pool
    pub worker_pool_size: usize,
    /// Ledger cycle duration
    pub cycle_duration: Duration,
    /// Phase timing configuration
    pub phase_timing: PhaseTimingConfig,
    /// Security thresholds
    pub security: SecurityConfig,
    /// Reward distribution
    pub rewards: RewardConfig,
}

impl Default for ConsensusConfig {
    fn default() -> Self {
        Self {
            producer_count: 100,
            worker_pool_size: 1000,
            cycle_duration: Duration::from_secs(60), // 1 minute cycles
            phase_timing: PhaseTimingConfig::default(),
            security: SecurityConfig::default(),
            rewards: RewardConfig::default(),
        }
    }
}

/// Phase timing configuration for consensus
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseTimingConfig {
    /// Construction phase duration
    pub construction_duration: Duration,
    /// Campaigning phase duration  
    pub campaigning_duration: Duration,
    /// Voting phase duration
    pub voting_duration: Duration,
    /// Synchronization phase duration
    pub synchronization_duration: Duration,
    /// Transaction freeze window
    pub freeze_window: Duration,
}

impl Default for PhaseTimingConfig {
    fn default() -> Self {
        Self {
            construction_duration: Duration::from_secs(15),
            campaigning_duration: Duration::from_secs(10),
            voting_duration: Duration::from_secs(10),
            synchronization_duration: Duration::from_secs(15),
            freeze_window: Duration::from_secs(5),
        }
    }
}

/// Security configuration for consensus
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Majority threshold for consensus decisions (0.0-1.0)
    pub majority_threshold: f64,
    /// Confidence level for statistical security (0.0-1.0)
    pub confidence_level: f64,
    /// Minimum number of producers to collect data from
    pub min_producer_responses: usize,
    /// Maximum malicious node percentage before security warning
    pub max_malicious_percentage: f64,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            majority_threshold: 0.67, // 67% supermajority
            confidence_level: 0.99999, // 99.999% confidence
            min_producer_responses: 10,
            max_malicious_percentage: 0.33, // 33% maximum
        }
    }
}

/// Reward distribution configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewardConfig {
    /// Base reward per cycle in smallest token unit
    pub base_reward: u64,
    /// Fraction of rewards for producer work (0.0-1.0)
    pub producer_fraction: f64,
    /// Fraction of rewards for voting work (0.0-1.0)
    pub voter_fraction: f64,
    /// Annual inflation rate (0.0-1.0)
    pub inflation_rate: f64,
    /// Enable dynamic reward adjustment
    pub dynamic_adjustment: bool,
}

impl Default for RewardConfig {
    fn default() -> Self {
        Self {
            base_reward: 1_000_000, // 1 KAT in smallest units
            producer_fraction: 0.8,
            voter_fraction: 0.2,
            inflation_rate: 0.02, // 2% annual inflation
            dynamic_adjustment: true,
        }
    }
}

impl ConsensusConfig {
    /// Validate consensus configuration
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.producer_count == 0 {
            return Err(ConfigError::Invalid("Producer count cannot be zero".to_string()));
        }

        if self.producer_count > self.worker_pool_size {
            return Err(ConfigError::Invalid(
                "Producer count cannot exceed worker pool size".to_string()
            ));
        }

        if self.cycle_duration.as_secs() == 0 {
            return Err(ConfigError::Invalid("Cycle duration cannot be zero".to_string()));
        }

        self.phase_timing.validate()?;
        self.security.validate()?;
        self.rewards.validate()?;

        Ok(())
    }

    /// Calculate total phase duration
    pub fn total_phase_duration(&self) -> Duration {
        self.phase_timing.construction_duration
            + self.phase_timing.campaigning_duration
            + self.phase_timing.voting_duration
            + self.phase_timing.synchronization_duration
    }
}

impl PhaseTimingConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        let phases = [
            ("construction", self.construction_duration),
            ("campaigning", self.campaigning_duration),
            ("voting", self.voting_duration),
            ("synchronization", self.synchronization_duration),
        ];

        for (name, duration) in phases {
            if duration.as_secs() == 0 {
                return Err(ConfigError::Invalid(
                    format!("{} phase duration cannot be zero", name)
                ));
            }
        }

        Ok(())
    }
}

impl SecurityConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        if !(0.5..=1.0).contains(&self.majority_threshold) {
            return Err(ConfigError::Invalid(
                "Majority threshold must be between 0.5 and 1.0".to_string()
            ));
        }

        if !(0.0..=1.0).contains(&self.confidence_level) {
            return Err(ConfigError::Invalid(
                "Confidence level must be between 0.0 and 1.0".to_string()
            ));
        }

        if self.min_producer_responses == 0 {
            return Err(ConfigError::Invalid(
                "Minimum producer responses cannot be zero".to_string()
            ));
        }

        if !(0.0..=1.0).contains(&self.max_malicious_percentage) {
            return Err(ConfigError::Invalid(
                "Max malicious percentage must be between 0.0 and 1.0".to_string()
            ));
        }

        Ok(())
    }
}

impl RewardConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        if self.base_reward == 0 {
            return Err(ConfigError::Invalid("Base reward cannot be zero".to_string()));
        }

        if !(0.0..=1.0).contains(&self.producer_fraction) {
            return Err(ConfigError::Invalid(
                "Producer fraction must be between 0.0 and 1.0".to_string()
            ));
        }

        if !(0.0..=1.0).contains(&self.voter_fraction) {
            return Err(ConfigError::Invalid(
                "Voter fraction must be between 0.0 and 1.0".to_string()
            ));
        }

        if (self.producer_fraction + self.voter_fraction - 1.0).abs() > f64::EPSILON {
            return Err(ConfigError::Invalid(
                "Producer and voter fractions must sum to 1.0".to_string()
            ));
        }

        if !(0.0..=0.1).contains(&self.inflation_rate) {
            return Err(ConfigError::Invalid(
                "Inflation rate must be between 0.0 and 0.1 (10%)".to_string()
            ));
        }

        Ok(())
    }
}