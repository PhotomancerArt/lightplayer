//! What a candidate store can fail with, as the harness sees it.

use serde::{Deserialize, Serialize};

/// A candidate's failure, coarse enough to compare candidates.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StoreError {
    /// The store is full (a result, not a bug).
    NoSpace,
    /// The flash lost power mid-operation (the cut the harness asked for).
    PowerLost,
    /// The store found its own on-flash state inconsistent.
    Corrupt(String),
    /// Anything else (a library error the adapter could not classify).
    Other(String),
}

impl StoreError {
    pub fn kind(&self) -> &'static str {
        match self {
            StoreError::NoSpace => "no_space",
            StoreError::PowerLost => "power_lost",
            StoreError::Corrupt(_) => "corrupt",
            StoreError::Other(_) => "other",
        }
    }
}

impl core::fmt::Display for StoreError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            StoreError::NoSpace => write!(f, "NoSpace"),
            StoreError::PowerLost => write!(f, "PowerLost"),
            StoreError::Corrupt(s) => write!(f, "Corrupt({s})"),
            StoreError::Other(s) => write!(f, "Other({s})"),
        }
    }
}
