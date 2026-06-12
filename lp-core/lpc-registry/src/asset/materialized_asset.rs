//! Transient effective asset bodies.

use alloc::string::String;
use alloc::vec::Vec;

use lpc_model::{AssetKind, AssetSource, Revision};

/// Effective asset bytes read for compilation, diagnostics, or runtime load.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterializedAsset {
    pub source: AssetSource,
    pub kind: AssetKind,
    pub revision: Revision,
    pub bytes: Vec<u8>,
    pub diagnostic_name: String,
}

/// Effective UTF-8 asset text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterializedTextAsset {
    pub source: AssetSource,
    pub kind: AssetKind,
    pub revision: Revision,
    pub text: String,
    pub diagnostic_name: String,
}
