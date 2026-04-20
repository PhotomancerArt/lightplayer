//! Helper for seeding per-frame inputs from manifest defaults.

use alloc::string::String;
use alloc::vec::Vec;

use crate::input::FxValue;
use crate::manifest::FxManifest;

/// Collect every input from `manifest` that has a `default` value.
///
/// Inputs without a default are skipped; the caller supplies them
/// (or relies on the shader's own uniform initial value).
#[must_use]
pub fn defaults_from_manifest(manifest: &FxManifest) -> Vec<(String, FxValue)> {
    manifest
        .inputs
        .iter()
        .filter_map(|(name, def)| def.default.clone().map(|v| (name.clone(), v)))
        .collect()
}
