//! Errors from effective asset materialization.

use alloc::string::String;

use lpc_model::{AssetSource, NodeDefLocation};

/// Failure reading the effective body of a referenced project asset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MaterializeAssetError {
    UnreferencedAsset {
        source: AssetSource,
    },
    NotFound {
        source: AssetSource,
    },
    Deleted {
        source: AssetSource,
    },
    ReadError {
        source: AssetSource,
        message: String,
    },
    Utf8 {
        source: AssetSource,
        message: String,
    },
    Unsupported {
        source: AssetSource,
        message: String,
    },
    OwnerDefUnavailable {
        source: AssetSource,
        owner: NodeDefLocation,
    },
}
