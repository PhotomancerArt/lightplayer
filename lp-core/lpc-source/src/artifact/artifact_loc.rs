use alloc::string::String;
use core::fmt;

use lpc_model::LpPathBuf;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::src_artifact_lib_ref::SrcArtifactLibRef;

/// Author-facing locator for a loadable artifact carried in source as a string.
///
/// - `./effects/tint.effect.toml` parses as [`ArtifactLocator::Path`].
/// - `lib:core/visual/checkerboard` parses as [`ArtifactLocator::Lib`].
///
/// Path locators are contextual: relative paths resolve relative to the file
/// that contains the locator. Engine-side resolved identity is
/// `ArtifactLocation` in `lpc-engine`; this type stays authored and contextual.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ArtifactLocator {
    Path(LpPathBuf),
    Lib(SrcArtifactLibRef),
}

impl ArtifactLocator {
    /// Path reference (possibly relative).
    #[must_use]
    pub fn path(p: impl Into<LpPathBuf>) -> Self {
        Self::Path(p.into())
    }

    #[must_use]
    pub fn lib_ref(lib: SrcArtifactLibRef) -> Self {
        Self::Lib(lib)
    }

    pub fn parse(s: &str) -> Result<Self, &'static str> {
        let s = s.trim();
        if let Some(rest) = s.strip_prefix("lib:") {
            Ok(Self::Lib(SrcArtifactLibRef::try_from_suffix(rest)?))
        } else {
            Ok(Self::Path(LpPathBuf::from(s)))
        }
    }
}

impl fmt::Display for ArtifactLocator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Path(path) => f.write_str(path.as_str()),
            Self::Lib(lib) => fmt::Display::fmt(lib, f),
        }
    }
}

impl Serialize for ArtifactLocator {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for ArtifactLocator {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::parse(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(feature = "schema-gen")]
impl schemars::JsonSchema for ArtifactLocator {
    fn schema_name() -> alloc::borrow::Cow<'static, str> {
        <String as schemars::JsonSchema>::schema_name()
    }

    fn schema_id() -> alloc::borrow::Cow<'static, str> {
        <String as schemars::JsonSchema>::schema_id()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        <String as schemars::JsonSchema>::json_schema(generator)
    }
}

#[cfg(test)]
mod tests {
    use alloc::string::ToString;

    use super::ArtifactLocator;
    use crate::artifact::SrcArtifactLibRef;

    #[test]
    fn display_normalizes_path() {
        assert_eq!(
            ArtifactLocator::path("./fluid.vis").to_string(),
            "fluid.vis",
        );
    }

    #[test]
    fn display_lib_form() {
        let s = ArtifactLocator::lib_ref(SrcArtifactLibRef::try_from_suffix("core/x").unwrap());
        assert_eq!(s.to_string(), "lib:core/x");
    }

    #[test]
    fn serde_json_round_trip_path_and_lib() {
        let path = ArtifactLocator::path("effects/tint.effect.toml");
        let j = serde_json::to_string(&path).unwrap();
        assert_eq!(j, "\"effects/tint.effect.toml\"");
        let back: ArtifactLocator = serde_json::from_str(&j).unwrap();
        assert_eq!(back, path);

        let lib = ArtifactLocator::parse("lib:core/visual/checkerboard").unwrap();
        let j = serde_json::to_string(&lib).unwrap();
        assert_eq!(j, "\"lib:core/visual/checkerboard\"");
        let back: ArtifactLocator = serde_json::from_str(&j).unwrap();
        assert_eq!(back, lib);
    }

    #[test]
    fn parse_rejects_empty_lib_suffix() {
        assert!(ArtifactLocator::parse("lib:").is_err());
        assert!(ArtifactLocator::parse("lib:   ").is_err());
    }
}
