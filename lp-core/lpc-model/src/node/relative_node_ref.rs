use crate::{NodeName, NodeNameError};
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Parsed relative reference to another node in the runtime node tree.
///
/// `RelativeNodeRef` intentionally uses a dot-based syntax, not filesystem
/// slash syntax. Slash paths are reserved for artifacts and files. Node
/// references are relative-only in the current source model:
///
/// ```text
/// .                  current node
/// .child             child of current node
/// .child.grandchild  descendant of current node
/// ..                 parent
/// ..sibling          sibling through parent
/// ..sibling.child    sibling's child
/// ```
///
/// Future value references may append a value suffix, but this type only
/// validates and parses the node-reference portion.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
        if input.contains('/') {
            return Err(RelativeNodeRefError::SlashSyntax);
        }
        if input.contains('#') {
            return Err(RelativeNodeRefError::ValueSuffix);
        }

        let (parent_hops, rest) = if let Some(rest) = input.strip_prefix("..") {
            (1, rest)
        } else if let Some(rest) = input.strip_prefix('.') {
            (0, rest)
        } else {
            return Err(RelativeNodeRefError::MustBeRelative);
        };

        if rest.is_empty() {
            return Ok(Self {
                parent_hops,
                segments: Vec::new(),
            });
        }
        if rest.starts_with('.') || rest.ends_with('.') || rest.contains("..") {
            return Err(RelativeNodeRefError::MalformedDots);
        }

        let mut segments = Vec::new();
        for raw in rest.split('.') {
            let name = NodeName::parse(raw).map_err(RelativeNodeRefError::InvalidSegment)?;
            segments.push(name);
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
        match self.parent_hops {
            0 => f.write_str(".")?,
            1 => f.write_str("..")?,
            n => {
                for _ in 0..n {
                    f.write_str("..")?;
                }
            }
        }
        for (index, segment) in self.segments.iter().enumerate() {
            if index > 0 {
                f.write_str(".")?;
            }
            f.write_str(segment.as_str())?;
        }
        Ok(())
    }
}

/// Error returned when parsing a relative node reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelativeNodeRefError {
    Empty,
    SlashSyntax,
    ValueSuffix,
    MustBeRelative,
    MalformedDots,
    InvalidSegment(NodeNameError),
}

impl fmt::Display for RelativeNodeRefError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("node location is empty"),
            Self::SlashSyntax => f.write_str("node locations use dot syntax, not slash syntax"),
            Self::ValueSuffix => {
                f.write_str("relative node reference parser does not accept value suffixes")
            }
            Self::MustBeRelative => f.write_str("node location must start with `.` or `..`"),
            Self::MalformedDots => f.write_str("node location has malformed dot separators"),
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
        let loc = RelativeNodeRefSrc::new("..texture".to_string());
        assert_eq!(loc.as_str(), "..texture");
    }

    #[test]
    fn relative_node_ref_src_from_string() {
        let loc = RelativeNodeRefSrc::from("..shader".to_string());
        assert_eq!(loc.as_str(), "..shader");
    }

    #[test]
    fn relative_node_ref_src_from_str() {
        let loc = RelativeNodeRefSrc::from("..output");
        assert_eq!(loc.as_str(), "..output");
    }

    #[test]
    fn parse_current_node() {
        let parsed = RelativeNodeRefSrc::from(".").parse().unwrap();
        assert_eq!(parsed.parent_hops(), 0);
        assert!(parsed.segments().is_empty());
    }

    #[test]
    fn parse_child_descendant() {
        let parsed = RelativeNodeRefSrc::from(".child.grandchild")
            .parse()
            .unwrap();
        assert_eq!(parsed.parent_hops(), 0);
        assert_eq!(parsed.segments().len(), 2);
        assert_eq!(parsed.segments()[0].as_str(), "child");
        assert_eq!(parsed.segments()[1].as_str(), "grandchild");
    }

    #[test]
    fn parse_parent_sibling() {
        let parsed = RelativeNodeRefSrc::from("..sibling.child").parse().unwrap();
        assert_eq!(parsed.parent_hops(), 1);
        assert_eq!(parsed.segments().len(), 2);
        assert_eq!(parsed.segments()[0].as_str(), "sibling");
        assert_eq!(parsed.segments()[1].as_str(), "child");
    }

    #[test]
    fn rejects_slash_paths() {
        assert!(matches!(
            RelativeNodeRefSrc::from("/src/test.texture").parse(),
            Err(RelativeNodeRefError::SlashSyntax)
        ));
        assert!(matches!(
            RelativeNodeRefSrc::from("./texture").parse(),
            Err(RelativeNodeRefError::SlashSyntax)
        ));
    }

    #[test]
    fn rejects_empty_and_absolute_like_names() {
        assert!(matches!(
            RelativeNodeRefSrc::from("").parse(),
            Err(RelativeNodeRefError::Empty)
        ));
        assert!(matches!(
            RelativeNodeRefSrc::from("texture").parse(),
            Err(RelativeNodeRefError::MustBeRelative)
        ));
    }

    #[test]
    fn rejects_malformed_dot_sequences() {
        assert!(matches!(
            RelativeNodeRefSrc::from("...child").parse(),
            Err(RelativeNodeRefError::MalformedDots)
        ));
        assert!(matches!(
            RelativeNodeRefSrc::from(".child.").parse(),
            Err(RelativeNodeRefError::MalformedDots)
        ));
    }

    #[test]
    fn rejects_property_suffixes_for_now() {
        assert!(matches!(
            RelativeNodeRefSrc::from("..shader#state.output").parse(),
            Err(RelativeNodeRefError::ValueSuffix)
        ));
    }

    #[test]
    fn relative_node_ref_display_round_trips() {
        for input in [".", ".child.grandchild", "..sibling.child"] {
            let parsed = RelativeNodeRef::parse(input).unwrap();
            assert_eq!(parsed.to_string(), input);
        }
    }

    #[test]
    fn relative_node_ref_deserializes_from_string() {
        let parsed: RelativeNodeRef = serde_json::from_str(r#""..texture""#).unwrap();
        assert_eq!(parsed.to_string(), "..texture");
    }
}
