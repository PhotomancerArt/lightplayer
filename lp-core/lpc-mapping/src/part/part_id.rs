//! A part's id: flat, unique in its fixture, never reused.
//!
//! A new part's id is minted from its kind and a number (`path1`,
//! `circle2`; see [`crate::part::IdCounters`]); an existing slug (`sector`,
//! `front`) is an id too. The rule is today's object-id rule
//! ([`crate::Map2dObjectId`]): `[a-z][a-z0-9_-]*`, at most 24 bytes. So
//! `@` and `/`, which a node id uses to add repeat copies and generated keys
//! (`circle2/r1@2`), can never occur in one.

use alloc::string::{String, ToString};

use crate::map2d_object_id::Map2dObjectId;

/// A part's id: `[a-z][a-z0-9_-]*`, at most
/// [`crate::MAP2D_OBJECT_ID_MAX_LEN`] bytes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PartId(String);

impl PartId {
    /// Validate and wrap an id, by today's object-id rule. The error names
    /// the rule it breaks.
    pub fn new(id: &str) -> Result<Self, String> {
        Map2dObjectId::new(id).map(|id| Self(id.as_str().to_string()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl core::fmt::Display for PartId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}
