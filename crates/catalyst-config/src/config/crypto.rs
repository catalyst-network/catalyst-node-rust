use crate::error::{ConfigError, ConfigResult};
use serde::{Deserialize, Serialize};

/// Cryptographic configuration for Catalyst Network
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CryptoConfig {
    /// Elliptic curve specification
    pub curve: CurveType,

    /// Hash algorithm specification
    pub hash_algorithm: HashAlgorithm,

    /// Signature scheme specification
    pub signature_scheme: SignatureScheme,

    /// Key derivation settings
    pub key_derivation: KeyDerivationConfig,

    /// Signature aggregation settings
    pub signature_aggregation: SignatureAggregationConfig,

    /// Confidential transaction settings
    pub confidential_transactions: ConfidentialTransactionConfig,

    /// Random number generation settings
    pub random_generation: RandomGenerationConfig,

    /// Security parameters
    pub security: CryptoSecurityConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CurveType {
    /// Twisted Edwards Curve25519 (as specified in the project)
    Curve25519,
    /// For future compatibility
    Secp256k1,
    Ed25519,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HashAlgorithm {
    /// Blake2b-256 (as specified in the project)
    Blake2b256,
    /// For compatibility
    Sha256,
    Sha3_256,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SignatureScheme {
    /// MuSig aggregated signatures (as specified)
    MuSig,
    /// Schnorr signatures
    Schnorr,
    /// ECDSA for compatibility
    Ecdsa,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyDerivationConfig {
    /// Key size in bits (256-bit as specified)
    pub key_size_bits: u32,

    /// Use deterministic key generation
    pub deterministic: bool,

    /// Key derivation function
    pub kdf: KeyDerivationFunction,

    /// Number of iterations for key stretching
    pub iterations: u32,

    /// Salt size for key derivation
    pub salt_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum KeyDerivationFunction {
    /// PBKDF2 with HMAC
    Pbkdf2,
    /// Argon2 for stronger security
    Argon2,
    /// scrypt
    Scrypt,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureAggregationConfig {
    /// Enable signature aggregation for transactions
    pub enabled: bool,

    /// Maximum number of signatures to aggregate
    pub max_signatures_per_aggregation: usize,

    /// Aggregation timeout in milliseconds
    pub aggregation_timeout_ms: u64,

    /// Batch aggregation for efficiency
    pub batch_aggregation: bool,

    /// Verification parallelization
    pub parallel_verification: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfidentialTransactionConfig {
    /// Enable confidential transactions using Pedersen Commitments
    pub enabled: bool,

    /// Range proof system
    pub range_proof_system: RangeProofSystem,

    /// Commitment blinding factor size
    pub blinding_factor_size: usize,

    /// Range proof size optimization
    pub optimize_proof_size: bool,

    /// Batch verification for range proofs
    pub batch_verification: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RangeProofSystem {
    /// Bulletproofs for efficient range proofs
    Bulletproofs,
    /// Bulletproofs+ for improved efficiency
    BulletproofsPlus,
    /// PLONK-based range proofs
    Plonk,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RandomGenerationConfig {
    /// Random number generator type
    pub rng_type: RandomGeneratorType,

    /// Entropy source configuration
    pub entropy_source: EntropySource,

    /// Seed size for deterministic randomness
    pub seed_size: usize,

    /// Use hardware random number generator if available
    pub use_hardware_rng: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RandomGeneratorType {
    /// ChaCha20 PRNG
    ChaCha20,
    /// System random number generator
    SystemRng,
    /// Hardware RNG
    HardwareRng,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EntropySource {
    /// System entropy source
    System,
    /// Hardware entropy source
    Hardware,
    /// User-provided entropy
    UserProvided,
    /// Combined entropy sources
    Combined,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CryptoSecurityConfig {
    /// Constant-time operations enforcement
    pub constant_time_operations: bool,

    /// Memory cleanup after operations
    pub secure_memory_cleanup: bool,

    /// Side-channel attack resistance
    pub side_channel_resistance: bool,

    /// Key refresh interval in milliseconds
    pub key_refresh_interval_ms: Option<u64>,

    /// Maximum key usage count before refresh
    pub max_key_usage_count: Option<u64>,

    /// Enable additional cryptographic checks
    pub additional_checks: bool,
}

impl CryptoConfig {
    /// Create development crypto configuration
    pub fn devnet() -> Self {
        Self {
            curve: CurveType::Curve25519,
            hash_algorithm: HashAlgorithm::Blake2b256,
            signature_scheme: SignatureScheme::MuSig,
            key_derivation: KeyDerivationConfig {
                key_size_bits: 256,
                deterministic: false,
                kdf: KeyDerivationFunction::Pbkdf2,
                iterations: 1000, // Lower for development
                salt_size: 32,
            },
            signature_aggregation: SignatureAggregationConfig {
                enabled: true,
                max_signatures_per_aggregation: 100,
                aggregation_timeout_ms: 1000,
                batch_aggregation: true,
                parallel_verification: true,
            },
            confidential_transactions: ConfidentialTransactionConfig {
                enabled: false, // Disabled for simplicity in dev
                range_proof_system: RangeProofSystem::Bulletproofs,
                blinding_factor_size: 32,
                optimize_proof_size: false,
                batch_verification: false,
            },
            random_generation: RandomGenerationConfig {
                rng_type: RandomGeneratorType::SystemRng,
                entropy_source: EntropySource::System,
                seed_size: 32,
                use_hardware_rng: false,
            },
            security: CryptoSecurityConfig {
                constant_time_operations: false, // Relaxed for dev
                secure_memory_cleanup: false,
                side_channel_resistance: false,
                key_refresh_interval_ms: None,
                max_key_usage_count: None,
                additional_checks: false,
            },
        }
    }

    /// Create test network crypto configuration
    pub fn testnet() -> Self {
        Self {
            curve: CurveType::Curve25519,
            hash_algorithm: HashAlgorithm::Blake2b256,
            signature_scheme: SignatureScheme::MuSig,
            key_derivation: KeyDerivationConfig {
                key_size_bits: 256,
                deterministic: false,
                kdf: KeyDerivationFunction::Argon2,
                iterations: 10000,
                salt_size: 32,
            },
            signature_aggregation: SignatureAggregationConfig {
                enabled: true,
                max_signatures_per_aggregation: 500,
                aggregation_timeout_ms: 2000,
                batch_aggregation: true,
                parallel_verification: true,
            },
            confidential_transactions: ConfidentialTransactionConfig {
                enabled: true,
                range_proof_system: RangeProofSystem::Bulletproofs,
                blinding_factor_size: 32,
                optimize_proof_size: true,
                batch_verification: true,
            },
            random_generation: RandomGenerationConfig {
                rng_type: RandomGeneratorType::ChaCha20,
                entropy_source: EntropySource::Combined,
                seed_size: 32,
                use_hardware_rng: true,
            },
            security: CryptoSecurityConfig {
                constant_time_operations: true,
                secure_memory_cleanup: true,
                side_channel_resistance: true,
                key_refresh_interval_ms: Some(3600000), // 1 hour
                max_key_usage_count: Some(10000),
                additional_checks: true,
            },
        }
    }

    /// Create main network crypto configuration (production)
    pub fn mainnet() -> Self {
        Self {
            curve: CurveType::Curve25519,
            hash_algorithm: HashAlgorithm::Blake2b256,
            signature_scheme: SignatureScheme::MuSig,
            key_derivation: KeyDerivationConfig {
                key_size_bits: 256,
                deterministic: false,
                kdf: KeyDerivationFunction::Argon2,
                iterations: 100000, // High security
                salt_size: 32,
            },
            signature_aggregation: SignatureAggregationConfig {
                enabled: true,
                max_signatures_per_aggregation: 1000,
                aggregation_timeout_ms: 5000,
                batch_aggregation: true,
                parallel_verification: true,
            },
            confidential_transactions: ConfidentialTransactionConfig {
                enabled: true,
                range_proof_system: RangeProofSystem::BulletproofsPlus,
                blinding_factor_size: 32,
                optimize_proof_size: true,
                batch_verification: true,
            },
            random_generation: RandomGenerationConfig {
                rng_type: RandomGeneratorType::HardwareRng,
                entropy_source: EntropySource::Combined,
                seed_size: 32,
                use_hardware_rng: true,
            },
            security: CryptoSecurityConfig {
                constant_time_operations: true,
                secure_memory_cleanup: true,
                side_channel_resistance: true,
                key_refresh_interval_ms: Some(1800000), // 30 minutes
                max_key_usage_count: Some(50000),
                additional_checks: true,
            },
        }
    }

    /// Validate crypto configuration
    pub fn validate(&self) -> ConfigResult<()> {
        // Validate key size
        if self.key_derivation.key_size_bits != 256 {
            return Err(ConfigError::ValidationFailed(
                "Key size must be 256 bits as per Catalyst specification".to_string(),
            ));
        }

        // Validate curve compatibility
        match (&self.curve, &self.signature_scheme) {
            (CurveType::Curve25519, SignatureScheme::MuSig) => {} // Valid combination
            (CurveType::Curve25519, SignatureScheme::Schnorr) => {} // Valid combination
            _ => {
                return Err(ConfigError::ValidationFailed(
                    "Invalid curve and signature scheme combination".to_string(),
                ))
            }
        }

        // Validate signature aggregation settings
        if self.signature_aggregation.enabled {
            if self.signature_aggregation.max_signatures_per_aggregation == 0 {
                return Err(ConfigError::ValidationFailed(
                    "Max signatures per aggregation must be greater than 0".to_string(),
                ));
            }

            if self.signature_aggregation.aggregation_timeout_ms == 0 {
                return Err(ConfigError::ValidationFailed(
                    "Aggregation timeout must be greater than 0".to_string(),
                ));
            }
        }

        // Validate key derivation settings
        if self.key_derivation.iterations == 0 {
            return Err(ConfigError::ValidationFailed(
                "KDF iterations must be greater than 0".to_string(),
            ));
        }

        if self.key_derivation.salt_size == 0 {
            return Err(ConfigError::ValidationFailed(
                "Salt size must be greater than 0".to_string(),
            ));
        }

        // Validate confidential transaction settings
        if self.confidential_transactions.enabled {
            if self.confidential_transactions.blinding_factor_size == 0 {
                return Err(ConfigError::ValidationFailed(
                    "Blinding factor size must be greater than 0".to_string(),
                ));
            }
        }

        // Validate random generation settings
        if self.random_generation.seed_size == 0 {
            return Err(ConfigError::ValidationFailed(
                "Random seed size must be greater than 0".to_string(),
            ));
        }

        // Validate security settings for production
        if self.security.constant_time_operations {
            // If constant time operations are enabled, certain other security features should be too
            if !self.security.secure_memory_cleanup {
                return Err(ConfigError::ValidationFailed(
                    "Secure memory cleanup should be enabled when using constant time operations"
                        .to_string(),
                ));
            }
        }

        // Validate key refresh settings
        if let Some(refresh_interval) = self.security.key_refresh_interval_ms {
            if refresh_interval < 60000 {
                // Less than 1 minute
                return Err(ConfigError::ValidationFailed(
                    "Key refresh interval must be at least 1 minute".to_string(),
                ));
            }
        }

        if let Some(max_usage) = self.security.max_key_usage_count {
            if max_usage == 0 {
                return Err(ConfigError::ValidationFailed(
                    "Max key usage count must be greater than 0 if specified".to_string(),
                ));
            }
        }

        // Validate KDF iterations based on security level
        let min_iterations = match self.key_derivation.kdf {
            KeyDerivationFunction::Pbkdf2 => 10000,
            KeyDerivationFunction::Argon2 => 3,
            KeyDerivationFunction::Scrypt => 16384,
        };

        if self.key_derivation.iterations < min_iterations {
            return Err(ConfigError::ValidationFailed(format!(
                "KDF iterations too low for {:?}, minimum: {}",
                self.key_derivation.kdf, min_iterations
            )));
        }

        Ok(())
    }

    /// Check if the configuration meets production security standards
    pub fn is_production_ready(&self) -> bool {
        // Must use strong curve
        if !matches!(self.curve, CurveType::Curve25519) {
            return false;
        }

        // Must use secure hash algorithm
        if !matches!(self.hash_algorithm, HashAlgorithm::Blake2b256) {
            return false;
        }

        // Must use MuSig signature scheme
        if !matches!(self.signature_scheme, SignatureScheme::MuSig) {
            return false;
        }

        // Must have high iteration count for key derivation
        let min_iterations = match self.key_derivation.kdf {
            KeyDerivationFunction::Pbkdf2 => 100000,
            KeyDerivationFunction::Argon2 => 10,
            KeyDerivationFunction::Scrypt => 32768,
        };

        if self.key_derivation.iterations < min_iterations {
            return false;
        }

        // Must have security features enabled
        if !self.security.constant_time_operations {
            return false;
        }

        if !self.security.secure_memory_cleanup {
            return false;
        }

        if !self.security.side_channel_resistance {
            return false;
        }

        // Should have key refresh enabled
        if self.security.key_refresh_interval_ms.is_none() {
            return false;
        }

        true
    }

    /// Get cryptographic strength assessment
    pub fn get_crypto_strength(&self) -> CryptoStrength {
        let mut score = 0u32;
        let mut issues = Vec::new();

        // Curve strength (max 25 points)
        match self.curve {
            CurveType::Curve25519 => score += 25,
            CurveType::Ed25519 => score += 20,
            CurveType::Secp256k1 => score += 15,
        }

        // Hash algorithm strength (max 25 points)
        match self.hash_algorithm {
            HashAlgorithm::Blake2b256 => score += 25,
            HashAlgorithm::Sha3_256 => score += 20,
            HashAlgorithm::Sha256 => score += 15,
        }

        // Signature scheme strength (max 20 points)
        match self.signature_scheme {
            SignatureScheme::MuSig => score += 20,
            SignatureScheme::Schnorr => score += 15,
            SignatureScheme::Ecdsa => score += 10,
        }

        // KDF strength (max 15 points)
        let kdf_score = match self.key_derivation.kdf {
            KeyDerivationFunction::Argon2 => {
                if self.key_derivation.iterations >= 10 {
                    15
                } else if self.key_derivation.iterations >= 5 {
                    10
                } else {
                    5
                }
            }
            KeyDerivationFunction::Scrypt => {
                if self.key_derivation.iterations >= 32768 {
                    12
                } else if self.key_derivation.iterations >= 16384 {
                    8
                } else {
                    4
                }
            }
            KeyDerivationFunction::Pbkdf2 => {
                if self.key_derivation.iterations >= 100000 {
                    10
                } else if self.key_derivation.iterations >= 10000 {
                    6
                } else {
                    2
                }
            }
        };
        score += kdf_score;

        // Security features (max 15 points)
        if self.security.constant_time_operations {
            score += 5;
        } else {
            issues.push("Constant-time operations disabled".to_string());
        }
        if self.security.secure_memory_cleanup {
            score += 5;
        } else {
            issues.push("Secure memory cleanup disabled".to_string());
        }
        if self.security.side_channel_resistance {
            score += 5;
        } else {
            issues.push("Side-channel resistance disabled".to_string());
        }

        // Determine overall strength
        let strength = if score >= 90 {
            CryptoStrengthLevel::Excellent
        } else if score >= 75 {
            CryptoStrengthLevel::Good
        } else if score >= 60 {
            CryptoStrengthLevel::Adequate
        } else if score >= 40 {
            CryptoStrengthLevel::Weak
        } else {
            CryptoStrengthLevel::Inadequate
        };

        CryptoStrength {
            level: strength,
            score,
            max_score: 100,
            issues,
        }
    }

    /// Get recommended configuration for a network type
    pub fn recommended_for_network(network: &str) -> Self {
        match network {
            "mainnet" => Self::mainnet(),
            "testnet" => Self::testnet(),
            "devnet" => Self::devnet(),
            _ => Self::devnet(),
        }
    }

    /// Estimate computational overhead of cryptographic operations
    pub fn estimate_performance_impact(&self) -> CryptoPerformanceEstimate {
        let mut signing_overhead = 1.0;
        let mut verification_overhead = 1.0;
        let mut key_derivation_overhead = 1.0;

        // Signature scheme impact
        match self.signature_scheme {
            SignatureScheme::MuSig => {
                signing_overhead *= if self.signature_aggregation.enabled {
                    1.5
                } else {
                    1.2
                };
                verification_overhead *= if self.signature_aggregation.batch_aggregation {
                    0.8
                } else {
                    1.1
                };
            }
            SignatureScheme::Schnorr => {
                signing_overhead *= 1.1;
                verification_overhead *= 1.0;
            }
            SignatureScheme::Ecdsa => {
                signing_overhead *= 1.3;
                verification_overhead *= 1.4;
            }
        }

        // Security features impact
        if self.security.constant_time_operations {
            signing_overhead *= 1.1;
            verification_overhead *= 1.1;
        }

        if self.security.side_channel_resistance {
            signing_overhead *= 1.05;
            verification_overhead *= 1.05;
        }

        // KDF impact
        key_derivation_overhead = match self.key_derivation.kdf {
            KeyDerivationFunction::Argon2 => self.key_derivation.iterations as f64 / 3.0,
            KeyDerivationFunction::Scrypt => self.key_derivation.iterations as f64 / 16384.0,
            KeyDerivationFunction::Pbkdf2 => self.key_derivation.iterations as f64 / 10000.0,
        };

        CryptoPerformanceEstimate {
            signing_relative_cost: signing_overhead,
            verification_relative_cost: verification_overhead,
            key_derivation_relative_cost: key_derivation_overhead,
            memory_overhead_mb: if self.security.secure_memory_cleanup {
                2.0
            } else {
                1.0
            },
        }
    }
}

/// Cryptographic strength assessment
#[derive(Debug, Clone)]
pub struct CryptoStrength {
    pub level: CryptoStrengthLevel,
    pub score: u32,
    pub max_score: u32,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CryptoStrengthLevel {
    Excellent,  // 90-100 points
    Good,       // 75-89 points
    Adequate,   // 60-74 points
    Weak,       // 40-59 points
    Inadequate, // 0-39 points
}

impl std::fmt::Display for CryptoStrength {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Crypto Strength: {:?} ({}/{})",
            self.level, self.score, self.max_score
        )?;
        if !self.issues.is_empty() {
            write!(f, "\nIssues: {}", self.issues.join(", "))?;
        }
        Ok(())
    }
}

/// Performance impact estimate for cryptographic operations
#[derive(Debug, Clone)]
pub struct CryptoPerformanceEstimate {
    /// Relative cost of signing operations (1.0 = baseline)
    pub signing_relative_cost: f64,
    /// Relative cost of verification operations (1.0 = baseline)
    pub verification_relative_cost: f64,
    /// Relative cost of key derivation (1.0 = baseline)
    pub key_derivation_relative_cost: f64,
    /// Additional memory overhead in MB
    pub memory_overhead_mb: f64,
}

impl std::fmt::Display for CryptoPerformanceEstimate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Crypto Performance Impact:\n\
             - Signing: {:.1}x baseline cost\n\
             - Verification: {:.1}x baseline cost\n\
             - Key Derivation: {:.1}x baseline cost\n\
             - Memory Overhead: {:.1} MB",
            self.signing_relative_cost,
            self.verification_relative_cost,
            self.key_derivation_relative_cost,
            self.memory_overhead_mb
        )
    }
}
