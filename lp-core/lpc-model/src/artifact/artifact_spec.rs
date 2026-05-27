use alloc::string::String;
use core::fmt;

use lpfs::LpPathBuf;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::artifact::src_artifact_lib_ref::SrcArtifactLibRef;

/// Author-facing specifier for a loadable artifact carried in source as a string.
///
/// - `./effects/tint.effect.toml` parses as [`ArtifactSpec::Path`].
/// - `lib:core/visual/checkerboard` parses as [`ArtifactSpec::Lib`].
///
/// Path specifiers are contextual: relative paths resolve relative to the file
/// that contains the specifier. Resolved catalog identity is `ArtifactLocation`
/// in `lpc-node-registry`; this type stays authored and contextual.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ArtifactSpec {
    Path(LpPathBuf),
    Lib(SrcArtifactLibRef),
}

impl ArtifactSpec {
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

impl fmt::Display for ArtifactSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Path(path) => f.write_str(path.as_str()),
            Self::Lib(lib) => fmt::Display::fmt(lib, f),
        }
    }
}

impl Serialize for ArtifactSpec {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for ArtifactSpec {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::parse(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(feature = "schema-gen")]
impl schemars::JsonSchema for ArtifactSpec {
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

    use super::ArtifactSpec;
    use crate::artifact::src_artifact_lib_ref::SrcArtifactLibRef;

    #[test]
    fn display_normalizes_path() {
        assert_eq!(
            ArtifactSpec::path("./fluid.vis").to_string(),
            "fluid.vis",
        );
    }

    #[test]
    fn display_lib_form() {
        let s = ArtifactSpec::lib_ref(SrcArtifactLibRef::try_from_suffix("core/x").unwrap());
        assert_eq!(s.to_string(), "lib:core/x");
    }

    #[test]
    fn serde_json_round_trip_path_and_lib() {
        let path = ArtifactSpec::path("effects/tint.effect.toml");
        let j = serde_json::to_string(&path).unwrap();
        assert_eq!(j, "\"effects/tint.effect.toml\"");
        let back: ArtifactSpec = serde_json::from_str(&j).unwrap();
        assert_eq!(back, path);

        let lib = ArtifactSpec::parse("lib:core/visual/checkerboard").unwrap();
        let j = serde_json::to_string(&lib).unwrap();
        assert_eq!(j, "\"lib:core/visual/checkerboard\"");
        let back: ArtifactSpec = serde_json::from_str(&j).unwrap();
        assert_eq!(back, lib);
    }

    #[test]
    fn parse_rejects_empty_lib_suffix() {
        assert!(ArtifactSpec::parse("lib:").is_err());
        assert!(ArtifactSpec::parse("lib:   ").is_err());
    }
}
