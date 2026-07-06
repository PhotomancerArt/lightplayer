//! Error type for lpc-history operations.

use alloc::string::String;
use core::fmt;

use lpfs::FsError;

use crate::hash::content_hash::ContentHash;

/// Errors from lpc-history storage, log, and lineage operations.
#[derive(Debug, Clone)]
pub enum HistoryError {
    /// Underlying filesystem error.
    Fs(FsError),
    /// A blob referenced by a tree manifest is not in the store.
    MissingBlob(ContentHash),
    /// A stored blob's bytes no longer hash to its key.
    CorruptBlob(ContentHash),
    /// No tree manifest is stored for this package hash.
    MissingTree(ContentHash),
    /// A stored tree manifest failed to parse or does not hash to its key.
    MalformedTree(ContentHash),
    /// A tree manifest was built with two entries for the same path.
    DuplicateTreePath(String),
    /// A non-tail line of the event log failed to parse (1-based line number).
    MalformedEventLog { line: usize },
    /// Serialization failed (message from serde_json).
    Encode(String),
    /// The referenced version is not known to the history line.
    UnknownVersion(ContentHash),
    /// The event sequence violates history invariants.
    InvalidHistory(&'static str),
}

impl fmt::Display for HistoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HistoryError::Fs(e) => write!(f, "filesystem: {e}"),
            HistoryError::MissingBlob(h) => write!(f, "missing blob {}", h.short()),
            HistoryError::CorruptBlob(h) => write!(f, "corrupt blob {}", h.short()),
            HistoryError::MissingTree(h) => write!(f, "missing tree {}", h.short()),
            HistoryError::MalformedTree(h) => write!(f, "malformed tree {}", h.short()),
            HistoryError::DuplicateTreePath(p) => write!(f, "duplicate tree path {p}"),
            HistoryError::MalformedEventLog { line } => {
                write!(f, "malformed event log at line {line}")
            }
            HistoryError::Encode(msg) => write!(f, "encode: {msg}"),
            HistoryError::UnknownVersion(h) => write!(f, "unknown version {}", h.short()),
            HistoryError::InvalidHistory(msg) => write!(f, "invalid history: {msg}"),
        }
    }
}

impl From<FsError> for HistoryError {
    fn from(e: FsError) -> Self {
        HistoryError::Fs(e)
    }
}
