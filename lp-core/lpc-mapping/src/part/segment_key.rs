//! The key a part gives a segment it generates: a circle's ring (`r3`), a
//! split path's subpath (`s2`).
//!
//! A key is minted by its part and kept with what it names (a ring carries
//! its key; a subpath's key is its place among the path's drawn subpaths),
//! so editing one ring never renames another (F17).

use alloc::format;
use alloc::string::String;

/// A generated segment's key: `[a-z][a-z0-9]*`, at most 24 bytes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SegmentKey(String);

impl SegmentKey {
    /// The longest key, in bytes.
    pub const MAX_LEN: usize = 24;

    /// Validate and wrap a key.
    pub fn new(key: &str) -> Result<Self, String> {
        let mut chars = key.chars();
        let Some(first) = chars.next() else {
            return Err(String::from("segment key is empty"));
        };
        if key.len() > Self::MAX_LEN {
            return Err(format!(
                "segment key {key:?} is longer than {} bytes",
                Self::MAX_LEN
            ));
        }
        if !first.is_ascii_lowercase()
            || !chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        {
            return Err(format!("segment key {key:?} must be [a-z][a-z0-9]*"));
        }
        Ok(Self(String::from(key)))
    }

    /// A circle's ring `n`: `r<n>`.
    #[must_use]
    pub fn ring(n: u32) -> Self {
        Self(format!("r{n}"))
    }

    /// A split path's subpath `n` (1-based, in drawn order): `s<n>`.
    #[must_use]
    pub fn subpath(n: u32) -> Self {
        Self(format!("s{n}"))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl core::fmt::Display for SegmentKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}
