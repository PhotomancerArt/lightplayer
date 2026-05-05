use crate::{NodeName, NodeNameError};
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;
use serde::{Deserialize, Serialize};

/// Authored locator for another node in the runtime node tree.
///
/// `NodeLoc` intentionally uses a dot-based syntax, not filesystem slash
/// syntax. Slash paths are reserved for artifacts and files. Node locations are
/// relative-only in the current source model:
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
/// Future property references may append a `#...` suffix, but this type only
/// validates and parses the node-location portion before any property path.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeLoc(pub String);

impl NodeLoc {
    pub fn new(loc: String) -> Self {
        Self(loc)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn parse(&self) -> Result<ParsedNodeLoc, NodeLocError> {
        ParsedNodeLoc::parse(self.as_str())
    }
}

impl From<String> for NodeLoc {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for NodeLoc {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// Parsed form of a relative [`NodeLoc`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedNodeLoc {
    parent_hops: u8,
    segments: Vec<NodeName>,
}

impl ParsedNodeLoc {
    pub fn parse(input: &str) -> Result<Self, NodeLocError> {
        if input.is_empty() {
            return Err(NodeLocError::Empty);
        }
        if input.contains('/') {
            return Err(NodeLocError::SlashSyntax);
        }
        if input.contains('#') {
            return Err(NodeLocError::PropertySuffix);
        }

        let (parent_hops, rest) = if let Some(rest) = input.strip_prefix("..") {
            (1, rest)
        } else if let Some(rest) = input.strip_prefix('.') {
            (0, rest)
        } else {
            return Err(NodeLocError::MustBeRelative);
        };

        if rest.is_empty() {
            return Ok(Self {
                parent_hops,
                segments: Vec::new(),
            });
        }
        if rest.starts_with('.') || rest.ends_with('.') || rest.contains("..") {
            return Err(NodeLocError::MalformedDots);
        }

        let mut segments = Vec::new();
        for raw in rest.split('.') {
            let name = NodeName::parse(raw).map_err(NodeLocError::InvalidSegment)?;
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

/// Error returned when parsing a [`NodeLoc`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeLocError {
    Empty,
    SlashSyntax,
    PropertySuffix,
    MustBeRelative,
    MalformedDots,
    InvalidSegment(NodeNameError),
}

impl fmt::Display for NodeLocError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("node location is empty"),
            Self::SlashSyntax => f.write_str("node locations use dot syntax, not slash syntax"),
            Self::PropertySuffix => {
                f.write_str("node location parser does not accept property suffixes")
            }
            Self::MustBeRelative => f.write_str("node location must start with `.` or `..`"),
            Self::MalformedDots => f.write_str("node location has malformed dot separators"),
            Self::InvalidSegment(err) => write!(f, "invalid node location segment: {err}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_loc_string_wrapper_creation() {
        let loc = NodeLoc::new("..texture".to_string());
        assert_eq!(loc.as_str(), "..texture");
    }

    #[test]
    fn node_loc_from_string() {
        let loc = NodeLoc::from("..shader".to_string());
        assert_eq!(loc.as_str(), "..shader");
    }

    #[test]
    fn node_loc_from_str() {
        let loc = NodeLoc::from("..output");
        assert_eq!(loc.as_str(), "..output");
    }

    #[test]
    fn parse_current_node() {
        let parsed = NodeLoc::from(".").parse().unwrap();
        assert_eq!(parsed.parent_hops(), 0);
        assert!(parsed.segments().is_empty());
    }

    #[test]
    fn parse_child_descendant() {
        let parsed = NodeLoc::from(".child.grandchild").parse().unwrap();
        assert_eq!(parsed.parent_hops(), 0);
        assert_eq!(parsed.segments().len(), 2);
        assert_eq!(parsed.segments()[0].as_str(), "child");
        assert_eq!(parsed.segments()[1].as_str(), "grandchild");
    }

    #[test]
    fn parse_parent_sibling() {
        let parsed = NodeLoc::from("..sibling.child").parse().unwrap();
        assert_eq!(parsed.parent_hops(), 1);
        assert_eq!(parsed.segments().len(), 2);
        assert_eq!(parsed.segments()[0].as_str(), "sibling");
        assert_eq!(parsed.segments()[1].as_str(), "child");
    }

    #[test]
    fn rejects_slash_paths() {
        assert!(matches!(
            NodeLoc::from("/src/test.texture").parse(),
            Err(NodeLocError::SlashSyntax)
        ));
        assert!(matches!(
            NodeLoc::from("./texture").parse(),
            Err(NodeLocError::SlashSyntax)
        ));
    }

    #[test]
    fn rejects_empty_and_absolute_like_names() {
        assert!(matches!(
            NodeLoc::from("").parse(),
            Err(NodeLocError::Empty)
        ));
        assert!(matches!(
            NodeLoc::from("texture").parse(),
            Err(NodeLocError::MustBeRelative)
        ));
    }

    #[test]
    fn rejects_malformed_dot_sequences() {
        assert!(matches!(
            NodeLoc::from("...child").parse(),
            Err(NodeLocError::MalformedDots)
        ));
        assert!(matches!(
            NodeLoc::from(".child.").parse(),
            Err(NodeLocError::MalformedDots)
        ));
    }

    #[test]
    fn rejects_property_suffixes_for_now() {
        assert!(matches!(
            NodeLoc::from("..shader#state.output").parse(),
            Err(NodeLocError::PropertySuffix)
        ));
    }
}
