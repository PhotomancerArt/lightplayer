//! Provenance: authorship metadata as a capability of any node (R14).
//!
//! The §8 field set (settled as Q7): `author`, `version`, `license`,
//! `created` — all optional strings, no semver semantics yet. Modules
//! normally carry it; extraction copies the host project's provenance onto
//! the copy unless the node already has its own (copy-on-extract, R14 —
//! mechanics land with the vendoring flows). The `project.json` container
//! manifest carries the same four fields at its top level.

use alloc::string::String;

use crate::{OptionSlot, Slotted, ValueSlot};

/// Authorship metadata block: optional on any node definition.
#[derive(Clone, Debug, Default, PartialEq, Slotted)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct ProvenanceDef {
    /// Author attribution (plain string).
    pub author: OptionSlot<ValueSlot<String>>,
    /// Authored version string; no semver semantics yet.
    pub version: OptionSlot<ValueSlot<String>>,
    /// License identifier (e.g. `"CC0-1.0"`).
    pub license: OptionSlot<ValueSlot<String>>,
    /// ISO date the work was created (e.g. `"2026-08-01"`).
    pub created: OptionSlot<ValueSlot<String>>,
}

impl ProvenanceDef {
    pub fn is_empty(&self) -> bool {
        self.author.data.is_none()
            && self.version.data.is_none()
            && self.license.data.is_none()
            && self.created.data.is_none()
    }
}
