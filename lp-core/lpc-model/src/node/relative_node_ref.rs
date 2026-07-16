use crate::{NodeName, NodeNameError};
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Parsed relative reference to another node in the runtime node tree.
///
/// `RelativeNodeRef` uses filesystem-style slash syntax: `/` separates
/// node-tree segments and `..` hops to the parent, mirroring how paths read
/// everywhere else. Dots are reserved for *field structure* (slot paths),
/// slashes for the node tree. Node references are relative-only in the
/// current source model:
///
/// ```text
/// .                  current node
/// child              child of the current node
/// child/grandchild   descendant of the current node
/// ..                 parent
/// ../sibling         sibling through the parent
/// ../../aunt/child   two hops up, then descent
/// ```
///
/// A leading `./` is accepted and normalizes away (`./child` == `child`).
/// Absolute (`/`-rooted) references are rejected until the source model
/// grows root-anchored resolution. Future value references may append a
/// value suffix, but this type only validates and parses the node-reference
/// portion.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct RelativeNodeRef {
    parent_hops: u8,
    segments: Vec<NodeName>,
}

/// Authored relative node-reference text.
///
/// Prefer [`RelativeNodeRef`] in parsed source/domain structures. This wrapper
/// exists for boundaries that need to preserve or report the exact source text.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RelativeNodeRefSrc(pub String);

impl RelativeNodeRefSrc {
    pub fn new(loc: String) -> Self {
        Self(loc)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn parse(&self) -> Result<RelativeNodeRef, RelativeNodeRefError> {
        RelativeNodeRef::parse(self.as_str())
    }
}

impl From<String> for RelativeNodeRefSrc {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for RelativeNodeRefSrc {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl RelativeNodeRef {
    pub fn current() -> Self {
        Self {
            parent_hops: 0,
            segments: Vec::new(),
        }
    }

    pub fn parse(input: &str) -> Result<Self, RelativeNodeRefError> {
        if input.is_empty() {
            return Err(RelativeNodeRefError::Empty);
        }
        if input.contains('#') {
            return Err(RelativeNodeRefError::ValueSuffix);
        }
        if input.starts_with('/') {
            return Err(RelativeNodeRefError::Absolute);
        }
        if input == "." {
            return Ok(Self::current());
        }

        let mut parent_hops: u8 = 0;
        let mut segments = Vec::new();
        for (index, raw) in input.split('/').enumerate() {
            match raw {
                "" => return Err(RelativeNodeRefError::MalformedPath),
                // A single leading `./` normalizes away.
                "." if index == 0 => {}
                "." => return Err(RelativeNodeRefError::MalformedPath),
                ".." if segments.is_empty() => {
                    parent_hops = parent_hops
                        .checked_add(1)
                        .ok_or(RelativeNodeRefError::MalformedPath)?;
                }
                // Hops after a name segment would mean re-ascending mid-path;
                // authors should write the normalized form instead.
                ".." => return Err(RelativeNodeRefError::MalformedPath),
                raw => {
                    let name =
                        NodeName::parse(raw).map_err(RelativeNodeRefError::InvalidSegment)?;
                    segments.push(name);
                }
            }
        }

        Ok(Self {
            parent_hops,
            segments,
        })
    }

    pub fn parent_hops(&self) -> u8 {
        self.parent_hops
    }

    pub fn segments(&self) -> &[NodeName] {
        &self.segments
    }
}

impl Serialize for RelativeNodeRef {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for RelativeNodeRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let input = String::deserialize(deserializer)?;
        Self::parse(&input).map_err(serde::de::Error::custom)
    }
}

impl fmt::Display for RelativeNodeRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.parent_hops == 0 && self.segments.is_empty() {
            return f.write_str(".");
        }
        let mut wrote_any = false;
        for _ in 0..self.parent_hops {
            if wrote_any {
                f.write_str("/")?;
            }
            f.write_str("..")?;
            wrote_any = true;
        }
        for segment in &self.segments {
            if wrote_any {
                f.write_str("/")?;
            }
            f.write_str(segment.as_str())?;
            wrote_any = true;
        }
        Ok(())
    }
}

/// Error returned when parsing a relative node reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelativeNodeRefError {
    Empty,
    ValueSuffix,
    Absolute,
    MalformedPath,
    InvalidSegment(NodeNameError),
}

impl fmt::Display for RelativeNodeRefError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("node location is empty"),
            Self::ValueSuffix => {
                f.write_str("relative node reference parser does not accept value suffixes")
            }
            Self::Absolute => {
                f.write_str("absolute (`/`-rooted) node references are not supported yet")
            }
            Self::MalformedPath => {
                f.write_str("node location has malformed path segments (`..` only leads, no empty or `.` segments)")
            }
            Self::InvalidSegment(err) => write!(f, "invalid node location segment: {err}"),
        }
    }
}

impl core::error::Error for RelativeNodeRefError {}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn relative_node_ref_src_creation() {
        let loc = RelativeNodeRefSrc::new("../texture".to_string());
        assert_eq!(loc.as_str(), "../texture");
    }

    #[test]
    fn relative_node_ref_src_from_string() {
        let loc = RelativeNodeRefSrc::from("../shader".to_string());
        assert_eq!(loc.as_str(), "../shader");
    }

    #[test]
    fn relative_node_ref_src_from_str() {
        let loc = RelativeNodeRefSrc::from("../output");
        assert_eq!(loc.as_str(), "../output");
    }

    #[test]
    fn parse_current_node() {
        let parsed = RelativeNodeRefSrc::from(".").parse().unwrap();
        assert_eq!(parsed.parent_hops(), 0);
        assert!(parsed.segments().is_empty());
    }

    #[test]
    fn parse_child_descendant() {
        let parsed = RelativeNodeRefSrc::from("child/grandchild")
            .parse()
            .unwrap();
        assert_eq!(parsed.parent_hops(), 0);
        assert_eq!(parsed.segments().len(), 2);
        assert_eq!(parsed.segments()[0].as_str(), "child");
        assert_eq!(parsed.segments()[1].as_str(), "grandchild");
    }

    #[test]
    fn parse_leading_dot_slash_normalizes_to_child() {
        let parsed = RelativeNodeRefSrc::from("./child").parse().unwrap();
        assert_eq!(parsed.parent_hops(), 0);
        assert_eq!(parsed.segments().len(), 1);
        assert_eq!(parsed.to_string(), "child");
    }

    #[test]
    fn parse_parent_sibling() {
        let parsed = RelativeNodeRefSrc::from("../sibling/child")
            .parse()
            .unwrap();
        assert_eq!(parsed.parent_hops(), 1);
        assert_eq!(parsed.segments().len(), 2);
        assert_eq!(parsed.segments()[0].as_str(), "sibling");
        assert_eq!(parsed.segments()[1].as_str(), "child");
    }

    #[test]
    fn parse_multi_hop_parents() {
        let parsed = RelativeNodeRefSrc::from("../..").parse().unwrap();
        assert_eq!(parsed.parent_hops(), 2);
        assert!(parsed.segments().is_empty());

        let parsed = RelativeNodeRefSrc::from("../../aunt/child")
            .parse()
            .unwrap();
        assert_eq!(parsed.parent_hops(), 2);
        assert_eq!(parsed.segments().len(), 2);
    }

    #[test]
    fn rejects_absolute_paths() {
        assert!(matches!(
            RelativeNodeRefSrc::from("/fixture").parse(),
            Err(RelativeNodeRefError::Absolute)
        ));
    }

    #[test]
    fn rejects_empty() {
        assert!(matches!(
            RelativeNodeRefSrc::from("").parse(),
            Err(RelativeNodeRefError::Empty)
        ));
    }

    #[test]
    fn rejects_malformed_path_segments() {
        // Empty segments, mid-path `.`, and re-ascending after a name.
        assert!(matches!(
            RelativeNodeRefSrc::from("a//b").parse(),
            Err(RelativeNodeRefError::MalformedPath)
        ));
        assert!(matches!(
            RelativeNodeRefSrc::from("child/").parse(),
            Err(RelativeNodeRefError::MalformedPath)
        ));
        assert!(matches!(
            RelativeNodeRefSrc::from("a/./b").parse(),
            Err(RelativeNodeRefError::MalformedPath)
        ));
        assert!(matches!(
            RelativeNodeRefSrc::from("a/../b").parse(),
            Err(RelativeNodeRefError::MalformedPath)
        ));
    }

    #[test]
    fn rejects_retired_dot_syntax() {
        // The pre-2026-07-08 dot syntax must fail loudly, not silently
        // resolve to something else.
        assert!(RelativeNodeRefSrc::from("..shader").parse().is_err());
        assert!(
            RelativeNodeRefSrc::from(".child.grandchild")
                .parse()
                .is_err()
        );
    }

    #[test]
    fn rejects_property_suffixes_for_now() {
        assert!(matches!(
            RelativeNodeRefSrc::from("../shader#state.output").parse(),
            Err(RelativeNodeRefError::ValueSuffix)
        ));
    }

    #[test]
    fn relative_node_ref_display_round_trips() {
        for input in [
            ".",
            "..",
            "../..",
            "child/grandchild",
            "../sibling/child",
            "../../aunt",
        ] {
            let parsed = RelativeNodeRef::parse(input).unwrap();
            assert_eq!(parsed.to_string(), input);
        }
    }

    #[test]
    fn relative_node_ref_deserializes_from_string() {
        let parsed: RelativeNodeRef = serde_json::from_str(r#""../texture""#).unwrap();
        assert_eq!(parsed.to_string(), "../texture");
    }
}
