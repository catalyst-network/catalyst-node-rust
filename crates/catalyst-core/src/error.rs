use thiserror::Error;

/// Main error type for Catalyst operations
#[derive(Error, Debug)]
pub enum CatalystError {
    /// Network-related errors
    #[error("Network error: {0}")]
    Network(#[from] NetworkError),

    /// Storage-related errors
    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),

    /// Consensus-related errors
    #[error("Consensus error: {0}")]
    Consensus(#[from] ConsensusError),

    /// Runtime execution errors
    #[error("Runtime error: {0}")]
    Runtime(#[from] RuntimeError),

    /// Cryptographic errors
    #[error("Cryptographic error: {0}")]
    Cryptography(#[from] CryptographyError),

    /// Configuration errors
    #[error("Configuration error: {0}")]
    Configuration(#[from] ConfigurationError),

    /// Service bus errors
    #[error("Service bus error: {0}")]
    ServiceBus(#[from] ServiceBusError),

    /// DFS errors
    #[error("Distributed file system error: {0}")]
    Dfs(#[from] DfsError),

    /// Module errors
    #[error("Module error: {0}")]
    Module(#[from] ModuleError),

    /// Serialization errors
    #[error("Serialization error: {0}")]
    Serialization(#[from] SerializationError),

    /// Validation errors
    #[error("Validation error: {0}")]
    Validation(#[from] ValidationError),

    /// Generic I/O errors
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Generic errors
    #[error("Internal error: {message}")]
    Internal { message: String },

    /// Timeout errors
    #[error("Operation timed out: {operation}")]
    Timeout { operation: String },

    /// Resource exhaustion
    #[error("Resource exhausted: {resource}")]
    ResourceExhausted { resource: String },
}

/// Network-specific errors
#[derive(Error, Debug)]
pub enum NetworkError {
    #[error("Connection failed to peer {peer_id}: {reason}")]
    ConnectionFailed { peer_id: String, reason: String },

    #[error("Peer not found: {peer_id}")]
    PeerNotFound { peer_id: String },

    #[error("Message delivery failed: {reason}")]
    MessageDeliveryFailed { reason: String },

    #[error("Network partition detected")]
    NetworkPartition,

    #[error("Invalid message format: {details}")]
    InvalidMessage { details: String },

    #[error("Bandwidth limit exceeded")]
    BandwidthExceeded,

    #[error("Protocol version mismatch: expected {expected}, got {actual}")]
    ProtocolMismatch { expected: String, actual: String },
}

/// Storage-specific errors
#[derive(Error, Debug)]
pub enum StorageError {
    #[error("Key not found: {key}")]
    KeyNotFound { key: String },

    #[error("Database corruption detected")]
    DatabaseCorruption,

    #[error("Insufficient storage space: requested {requested}, available {available}")]
    InsufficientSpace { requested: u64, available: u64 },

    #[error("Write operation failed: {reason}")]
    WriteFailed { reason: String },

    #[error("Read operation failed: {reason}")]
    ReadFailed { reason: String },

    #[error("Database lock timeout")]
    LockTimeout,

    #[error("Backup operation failed: {reason}")]
    BackupFailed { reason: String },
}

/// Consensus-specific errors
#[derive(Error, Debug)]
pub enum ConsensusError {
    #[error("Not enough producers for consensus: required {required}, available {available}")]
    InsufficientProducers { required: u32, available: u32 },

    #[error("Consensus timeout in phase {phase}")]
    ConsensusTimeout { phase: String },

    #[error("Invalid consensus message: {reason}")]
    InvalidConsensusMessage { reason: String },

    #[error("Block validation failed: {reason}")]
    BlockValidationFailed { reason: String },

    #[error("Double voting detected from producer {producer_id}")]
    DoubleVoting { producer_id: String },

    #[error("Consensus finality not achieved")]
    FinalityNotAchieved,

    #[error("Fork detected at height {height}")]
    ForkDetected { height: u64 },

    #[error("Producer selection failed: {reason}")]
    ProducerSelectionFailed { reason: String },
}

/// Runtime execution errors
#[derive(Error, Debug)]
pub enum RuntimeError {
    #[error("Contract execution failed: {reason}")]
    ExecutionFailed { reason: String },

    #[error("Out of gas: used {used}, limit {limit}")]
    OutOfGas { used: u64, limit: u64 },

    #[error("Contract not found at address {address}")]
    ContractNotFound { address: String },

    #[error("Invalid contract code: {reason}")]
    InvalidCode { reason: String },

    #[error("Runtime not supported: {runtime_type}")]
    UnsupportedRuntime { runtime_type: String },

    #[error("Stack overflow in contract execution")]
    StackOverflow,

    #[error("Memory allocation failed: requested {requested} bytes")]
    MemoryAllocationFailed { requested: u64 },

    #[error("Invalid function signature: {signature}")]
    InvalidFunctionSignature { signature: String },
}

/// Cryptography-specific errors
#[derive(Error, Debug)]
pub enum CryptographyError {
    #[error("Invalid signature: {details}")]
    InvalidSignature { details: String },

    #[error("Invalid public key: {details}")]
    InvalidPublicKey { details: String },

    #[error("Invalid private key: {details}")]
    InvalidPrivateKey { details: String },

    #[error("Hash verification failed")]
    HashVerificationFailed,

    #[error("Encryption failed: {reason}")]
    EncryptionFailed { reason: String },

    #[error("Decryption failed: {reason}")]
    DecryptionFailed { reason: String },

    #[error("Random number generation failed")]
    RandomGenerationFailed,

    #[error("Key derivation failed: {reason}")]
    KeyDerivationFailed { reason: String },

    #[error("Range proof verification failed")]
    RangeProofFailed,

    #[error("Commitment verification failed")]
    CommitmentVerificationFailed,
}

/// Configuration-specific errors
#[derive(Error, Debug)]
pub enum ConfigurationError {
    #[error("Configuration file not found: {path}")]
    FileNotFound { path: String },

    #[error("Invalid configuration format: {details}")]
    InvalidFormat { details: String },

    #[error("Missing required configuration key: {key}")]
    MissingKey { key: String },

    #[error("Invalid configuration value for key {key}: {value}")]
    InvalidValue { key: String, value: String },

    #[error("Configuration validation failed: {reason}")]
    ValidationFailed { reason: String },

    #[error("Permission denied accessing configuration: {path}")]
    PermissionDenied { path: String },
}

/// Service bus errors
#[derive(Error, Debug)]
pub enum ServiceBusError {
    #[error("Subscription not found: {subscription_id}")]
    SubscriptionNotFound { subscription_id: String },

    #[error("Webhook delivery failed: {url}, reason: {reason}")]
    WebhookDeliveryFailed { url: String, reason: String },

    #[error("Invalid event filter: {details}")]
    InvalidEventFilter { details: String },

    #[error("Event serialization failed: {reason}")]
    EventSerializationFailed { reason: String },

    #[error("WebSocket connection failed: {reason}")]
    WebSocketConnectionFailed { reason: String },

    #[error("Rate limit exceeded for subscription {subscription_id}")]
    RateLimitExceeded { subscription_id: String },

    #[error("Authentication failed for webhook: {url}")]
    AuthenticationFailed { url: String },
}

/// Distributed File System errors
#[derive(Error, Debug)]
pub enum DfsError {
    #[error("File not found: {hash}")]
    FileNotFound { hash: String },

    #[error("File storage failed: {reason}")]
    StorageFailed { reason: String },

    #[error("File retrieval failed: {hash}, reason: {reason}")]
    RetrievalFailed { hash: String, reason: String },

    #[error("Invalid file hash: {hash}")]
    InvalidHash { hash: String },

    #[error("File too large: size {size}, limit {limit}")]
    FileTooLarge { size: u64, limit: u64 },

    #[error("Insufficient storage providers")]
    InsufficientProviders,

    #[error("Replication failed: {reason}")]
    ReplicationFailed { reason: String },

    #[error("Pin operation failed: {hash}, reason: {reason}")]
    PinFailed { hash: String, reason: String },
}

/// Module-specific errors
#[derive(Error, Debug)]
pub enum ModuleError {
    #[error("Module not found: {module_name}")]
    ModuleNotFound { module_name: String },

    #[error("Module initialization failed: {module_name}, reason: {reason}")]
    InitializationFailed { module_name: String, reason: String },

    #[error("Module start failed: {module_name}, reason: {reason}")]
    StartFailed { module_name: String, reason: String },

    #[error("Module stop failed: {module_name}, reason: {reason}")]
    StopFailed { module_name: String, reason: String },

    #[error("Module dependency not met: {module_name} requires {dependency}")]
    DependencyNotMet { module_name: String, dependency: String },

    #[error("Module version incompatible: {module_name}, required {required}, found {found}")]
    VersionIncompatible { module_name: String, required: String, found: String },

    #[error("Module health check failed: {module_name}, reason: {reason}")]
    HealthCheckFailed { module_name: String, reason: String },
}

/// Serialization errors
#[derive(Error, Debug)]
pub enum SerializationError {
    #[error("JSON serialization failed: {reason}")]
    JsonError { reason: String },

    #[error("Binary serialization failed: {reason}")]
    BinaryError { reason: String },

    #[error("Deserialization failed: {reason}")]
    DeserializationError { reason: String },

    #[error("Schema validation failed: {reason}")]
    SchemaValidationFailed { reason: String },

    #[error("Encoding not supported: {encoding}")]
    UnsupportedEncoding { encoding: String },
}

/// Validation errors
#[derive(Error, Debug)]
pub enum ValidationError {
    #[error("Transaction validation failed: {reason}")]
    TransactionValidation { reason: String },

    #[error("Block validation failed: {reason}")]
    BlockValidation { reason: String },

    #[error("Signature validation failed: {reason}")]
    SignatureValidation { reason: String },

    #[error("Address validation failed: {address}")]
    AddressValidation { address: String },

    #[error("Amount validation failed: {amount}")]
    AmountValidation { amount: String },

    #[error("Nonce validation failed: expected {expected}, got {actual}")]
    NonceValidation { expected: u64, actual: u64 },

    #[error("Gas validation failed: {reason}")]
    GasValidation { reason: String },

    #[error("Timestamp validation failed: {reason}")]
    TimestampValidation { reason: String },
}

/// Convenience macro for creating internal errors
#[macro_export]
macro_rules! internal_error {
    ($msg:expr) => {
        CatalystError::Internal {
            message: $msg.to_string(),
        }
    };
    ($fmt:expr, $($arg:tt)*) => {
        CatalystError::Internal {
            message: format!($fmt, $($arg)*),
        }
    };
}

/// Convenience macro for creating timeout errors
#[macro_export]
macro_rules! timeout_error {
    ($operation:expr) => {
        CatalystError::Timeout {
            operation: $operation.to_string(),
        }
    };
}

/// Convenience macro for creating resource exhausted errors
#[macro_export]
macro_rules! resource_exhausted {
    ($resource:expr) => {
        CatalystError::ResourceExhausted {
            resource: $resource.to_string(),
        }
    };
}