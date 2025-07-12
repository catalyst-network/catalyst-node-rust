// database.rs - Re-export from lib.rs
pub use crate::{InMemoryDatabase};

// For compatibility, create aliases
pub type CatalystDatabase = InMemoryDatabase;