//! Identity and addressing types: Uid, Name, NodePath, PropPath, NodePropSpec, ArtifactSpec, ChannelName.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use core::hash::{Hash, Hasher};

    #[test]
    fn uid_display_decimal() {
        assert_eq!(Uid(0).to_string(), "0");
        assert_eq!(Uid(7).to_string(), "7");
        assert_eq!(Uid(u32::MAX).to_string(), u32::MAX.to_string());
    }

    #[test]
    fn uid_equality_and_hashing() {
        #[derive(Default)]
        struct TestHasher(u64);
        impl Hasher for TestHasher {
            fn finish(&self) -> u64 {
                self.0
            }
            fn write(&mut self, bytes: &[u8]) {
                for &b in bytes {
                    self.0 = self.0.wrapping_mul(31).wrapping_add(u64::from(b));
                }
            }
        }

        let a = Uid(5);
        let b = Uid(5);
        let c = Uid(7);
        assert_eq!(a, b);
        assert_ne!(a, c);

        let mut ha = TestHasher::default();
        let mut hb = TestHasher::default();
        let mut hc = TestHasher::default();
        a.hash(&mut ha);
        b.hash(&mut hb);
        c.hash(&mut hc);
        assert_eq!(ha.finish(), hb.finish());
        assert_ne!(ha.finish(), hc.finish());
    }

    #[test]
    fn name_parse_accepts_valid() {
        for s in ["foo", "foo_bar_42", "_x", "X1"] {
            Name::parse(s).unwrap_or_else(|e| panic!("rejected {s:?}: {e}"));
        }
    }

    #[test]
    fn name_parse_rejects_invalid() {
        for s in ["", "1foo", "foo-bar", "foo bar", "foo.bar"] {
            assert!(Name::parse(s).is_err(), "should have rejected {s:?}");
        }
    }

    #[test]
    fn node_path_round_trips() {
        for s in [
            "/main.show",
            "/main.show/fluid.vis",
            "/dome.rig/main.layout/sector4.fixture",
        ] {
            let parsed = NodePath::parse(s).unwrap();
            assert_eq!(parsed.to_string(), s);
        }
    }

    #[test]
    fn node_path_rejects_malformed() {
        for s in ["", "main.show", "/", "//", "/main", "/main.show//x.y"] {
            assert!(NodePath::parse(s).is_err(), "should have rejected {s:?}");
        }
    }

    #[test]
    fn prop_path_via_reexport_speed() {
        let segs = prop_path::parse_path("speed").unwrap();
        assert_eq!(segs.len(), 1);
    }

    #[test]
    fn prop_path_via_reexport_nested() {
        let segs = prop_path::parse_path("config.spacing").unwrap();
        assert_eq!(segs.len(), 2);
    }

    #[test]
    fn node_prop_spec_round_trips() {
        let s = "/main.show/fluid.vis#speed";
        let parsed = NodePropSpec::parse(s).unwrap();
        assert_eq!(parsed.to_string(), s);
    }

    #[test]
    fn node_prop_spec_with_indexing_round_trips() {
        let s = "/x.y#a.b[0]";
        let parsed = NodePropSpec::parse(s).unwrap();
        assert_eq!(parsed.to_string(), s);
    }

    #[test]
    fn node_prop_spec_rejects_missing_hash() {
        assert!(NodePropSpec::parse("/main.show").is_err());
    }

    #[test]
    fn node_prop_spec_rejects_double_hash() {
        assert!(NodePropSpec::parse("/main.show#a#b").is_err());
    }

    #[test]
    fn artifact_spec_display_round_trips() {
        assert_eq!(
            ArtifactSpec(String::from("./fluid.vis")).to_string(),
            "./fluid.vis",
        );
    }

    #[test]
    fn channel_name_display_round_trips() {
        assert_eq!(
            ChannelName(String::from("audio/in/0")).to_string(),
            "audio/in/0",
        );
    }
}

// --- Uid ----------------------------------------------------------------

#[derive(
    Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct Uid(pub u32);

impl fmt::Display for Uid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// --- Name ---------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NameError {
    Empty,
    LeadingDigit,
    InvalidChar(char),
}

impl fmt::Display for NameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("name is empty"),
            Self::LeadingDigit => f.write_str("name must not start with a digit"),
            Self::InvalidChar(c) => write!(f, "invalid character in name: {c:?}"),
        }
    }
}

impl core::error::Error for NameError {}

#[derive(
    Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct Name(pub String);

impl Name {
    pub fn parse(s: &str) -> Result<Self, NameError> {
        if s.is_empty() {
            return Err(NameError::Empty);
        }
        for c in s.chars() {
            if !(c.is_ascii_alphanumeric() || c == '_') {
                return Err(NameError::InvalidChar(c));
            }
        }
        if let Some(first) = s.chars().next() {
            if first.is_ascii_digit() {
                return Err(NameError::LeadingDigit);
            }
        }
        Ok(Name(String::from(s)))
    }
}

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

// --- NodePath -----------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PathError {
    Empty,
    MissingLeadingSlash,
    EmptySegment,
    SegmentMissingType(String),
    InvalidName(NameError),
}

impl fmt::Display for PathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("node path is empty"),
            Self::MissingLeadingSlash => f.write_str("node path must start with `/`"),
            Self::EmptySegment => f.write_str("node path has an empty segment"),
            Self::SegmentMissingType(s) => {
                write!(f, "segment `{s}` is missing the `.<type>` suffix")
            }
            Self::InvalidName(e) => write!(f, "{e}"),
        }
    }
}

impl core::error::Error for PathError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::InvalidName(e) => Some(e),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct NodePathSegment {
    pub name: Name,
    pub ty: Name,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct NodePath(pub Vec<NodePathSegment>);

impl NodePath {
    pub fn parse(s: &str) -> Result<Self, PathError> {
        let s = s.strip_prefix('/').ok_or(PathError::MissingLeadingSlash)?;
        if s.is_empty() {
            return Err(PathError::Empty);
        }
        let mut segments = Vec::new();
        for raw in s.split('/') {
            if raw.is_empty() {
                return Err(PathError::EmptySegment);
            }
            let (name, ty) = raw
                .split_once('.')
                .ok_or_else(|| PathError::SegmentMissingType(String::from(raw)))?;
            let name = Name::parse(name).map_err(PathError::InvalidName)?;
            let ty = Name::parse(ty).map_err(PathError::InvalidName)?;
            segments.push(NodePathSegment { name, ty });
        }
        Ok(NodePath(segments))
    }
}

impl fmt::Display for NodePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for seg in &self.0 {
            write!(f, "/{}.{}", seg.name, seg.ty)?;
        }
        Ok(())
    }
}

// --- PropPath -----------------------------------------------------------

pub mod prop_path {
    pub use lps_shared::path::{LpsPathSeg as Segment, PathParseError, parse_path};
}

pub type PropPath = Vec<prop_path::Segment>;

// --- NodePropSpec -------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NodePropSpecError {
    MissingHash,
    ExtraHash,
    Path(PathError),
    Prop(prop_path::PathParseError),
}

impl fmt::Display for NodePropSpecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingHash => f.write_str("node property spec must contain `#`"),
            Self::ExtraHash => f.write_str("node property spec must contain at most one `#`"),
            Self::Path(e) => write!(f, "{e}"),
            Self::Prop(e) => write!(f, "{e}"),
        }
    }
}

impl core::error::Error for NodePropSpecError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::Path(e) => Some(e),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct NodePropSpec {
    pub node: NodePath,
    pub prop: PropPath,
}

impl NodePropSpec {
    pub fn parse(s: &str) -> Result<Self, NodePropSpecError> {
        let (node_part, prop_part) = s.split_once('#').ok_or(NodePropSpecError::MissingHash)?;
        if prop_part.contains('#') {
            return Err(NodePropSpecError::ExtraHash);
        }
        let node = NodePath::parse(node_part).map_err(NodePropSpecError::Path)?;
        let prop = prop_path::parse_path(prop_part).map_err(NodePropSpecError::Prop)?;
        Ok(NodePropSpec { node, prop })
    }
}

impl fmt::Display for NodePropSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}#", self.node)?;
        for (i, seg) in self.prop.iter().enumerate() {
            if i > 0 && matches!(seg, prop_path::Segment::Field(_)) {
                f.write_str(".")?;
            }
            match seg {
                prop_path::Segment::Field(name) => f.write_str(name)?,
                prop_path::Segment::Index(idx) => write!(f, "[{idx}]")?,
            }
        }
        Ok(())
    }
}

// --- ArtifactSpec -------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct ArtifactSpec(pub String);

impl fmt::Display for ArtifactSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

// --- ChannelName --------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct ChannelName(pub String);

impl fmt::Display for ChannelName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
