//! Parsed effect manifest (`fx.toml`).

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use crate::input::FxInputDef;

/// Suggested render resolution for previews and defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FxResolution {
    pub width: u32,
    pub height: u32,
}

/// Human-readable metadata from `[meta]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FxMeta {
    pub name: String,
    pub description: Option<String>,
    pub author: Option<String>,
    pub tags: Vec<String>,
}

/// Validated manifest: meta, resolution, and all `[input.*]` entries.
#[derive(Debug, Clone, PartialEq)]
pub struct FxManifest {
    pub meta: FxMeta,
    pub resolution: FxResolution,
    pub inputs: BTreeMap<String, FxInputDef>,
}
