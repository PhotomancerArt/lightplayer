//! Authoring-side reference inside the `lib:` artifact scheme (`lib:core/...`).

use alloc::string::String;
use core::fmt;

/// Path segment after `lib:` in an authored artifact string (opaque to the filesystem).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct SrcArtifactLibRef(String);

impl SrcArtifactLibRef {
    /// Builds from the path after `lib:` (excluding that prefix).
    pub fn try_from_suffix(s: &str) -> Result<Self, &'static str> {
        let s = s.trim();
        if s.is_empty() {
            Err("`lib:` artifact reference requires a non-empty library path")
        } else {
            Ok(Self(String::from(s)))
        }
    }

    /// Library path after `lib:` (opaque; not normalized as a filesystem path).
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Display for SrcArtifactLibRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "lib:{}", self.0)
    }
}
