//! Errors from effective asset materialization.

use alloc::string::String;

use lpc_model::{AssetLocation, NodeDefLocation};

/// Failure reading the effective body of a referenced project asset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetReadError {
    UnreferencedAsset {
        location: AssetLocation,
    },
    NotFound {
        location: AssetLocation,
    },
    Deleted {
        location: AssetLocation,
    },
    ReadError {
        location: AssetLocation,
        message: String,
    },
    Utf8 {
        location: AssetLocation,
        message: String,
    },
    Unsupported {
        location: AssetLocation,
        message: String,
    },
    OwnerDefUnavailable {
        location: AssetLocation,
        owner: NodeDefLocation,
    },
}
