//! Transient effective asset bodies.

use alloc::string::String;
use alloc::vec::Vec;

use lpc_model::{AssetContentType, AssetLocation, Revision};

/// Effective asset bytes read for compilation, diagnostics, or runtime load.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetBytes {
    pub location: AssetLocation,
    pub content_type: AssetContentType,
    pub revision: Revision,
    pub bytes: Vec<u8>,
    pub diagnostic_name: String,
}

impl AssetBytes {
    pub fn changed_since(&self, revision: Revision) -> bool {
        self.revision > revision
    }
}

/// Effective UTF-8 asset text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetText {
    pub location: AssetLocation,
    pub content_type: AssetContentType,
    pub revision: Revision,
    pub text: String,
    pub diagnostic_name: String,
}

impl AssetText {
    pub fn changed_since(&self, revision: Revision) -> bool {
        self.revision > revision
    }
}
