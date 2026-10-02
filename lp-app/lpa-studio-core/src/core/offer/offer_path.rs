//! [`OfferPath`]: the stable id of one offer, `project/save` or
//! `project/demo.module/orbit.shader/remove`.

use core::fmt;
use core::str::FromStr;

use crate::ProjectNodeAddress;

/// The stable id of one offer: a sequence of segments, written `a/b/c`.
///
/// Two kinds of segment share the path, and a dot tells them apart, which
/// is why a node's tree path can sit inline without escaping:
///
/// - a **verb or namespace** segment (`project`, `devices`, `save`,
///   `remove`) never contains a `.`;
/// - a **node** segment always does, because it is the node's `name.kind`
///   (`demo.module`, `orbit.shader`).
///
/// So `project/demo.module/orbit.shader/remove` reads as: the project
/// namespace, the node `/demo.module/orbit.shader`, the verb `remove`.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OfferPath {
    segments: Vec<String>,
}

impl OfferPath {
    /// The namespace every project offer lives under.
    pub const PROJECT: &'static str = "project";
    /// The namespace every device offer lives under.
    pub const DEVICES: &'static str = "devices";

    /// A one-segment path: a namespace or verb (no `.`).
    pub fn root(segment: impl Into<String>) -> Self {
        let segment = segment.into();
        debug_assert_verb(&segment);
        Self {
            segments: vec![segment],
        }
    }

    /// `project`, the project namespace.
    pub fn project() -> Self {
        Self::root(Self::PROJECT)
    }

    /// `devices`, the device namespace.
    pub fn devices() -> Self {
        Self::root(Self::DEVICES)
    }

    /// `project/<node tree path>`: the prefix a node card asks
    /// [`crate::UiOfferTree::verbs_of`] for.
    pub fn project_node(address: &ProjectNodeAddress) -> Self {
        Self::project().node(address)
    }

    /// This path plus one verb or namespace segment (no `.`).
    pub fn child(mut self, segment: impl Into<String>) -> Self {
        let segment = segment.into();
        debug_assert_verb(&segment);
        self.segments.push(segment);
        self
    }

    /// This path plus a node's tree path, one `name.kind` segment per tree
    /// level: `/demo.module/orbit.shader` appends `demo.module` and
    /// `orbit.shader`.
    pub fn node(mut self, address: &ProjectNodeAddress) -> Self {
        self.segments.extend(
            address
                .path()
                .0
                .iter()
                .map(|segment| format!("{}.{}", segment.name, segment.ty)),
        );
        self
    }

    /// Parse the text form `a/b/c`. Every segment must be non-empty.
    pub fn parse(text: &str) -> Result<Self, OfferPathError> {
        if text.is_empty() {
            return Err(OfferPathError::Empty);
        }
        let segments = text
            .split('/')
            .map(|segment| {
                if segment.is_empty() {
                    Err(OfferPathError::EmptySegment(text.to_string()))
                } else {
                    Ok(segment.to_string())
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { segments })
    }

    /// The segments, in order.
    pub fn segments(&self) -> &[String] {
        &self.segments
    }

    /// How many segments the path has.
    pub fn len(&self) -> usize {
        self.segments.len()
    }

    /// Never true for a constructed path; present for the `len` pair.
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    /// The last segment: an offer's verb.
    pub fn last(&self) -> Option<&str> {
        self.segments.last().map(String::as_str)
    }

    /// Whether `prefix` is this path or an ancestor of it, compared segment
    /// by segment: `project/save` starts with `project`, never with `proj`.
    pub fn starts_with(&self, prefix: &OfferPath) -> bool {
        self.segments.starts_with(&prefix.segments)
    }
}

impl fmt::Display for OfferPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.segments.join("/"))
    }
}

impl FromStr for OfferPath {
    type Err = OfferPathError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::parse(text)
    }
}

/// Why a text could not be read as an [`OfferPath`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OfferPathError {
    /// The text was empty.
    Empty,
    /// The text had an empty segment (`a//b`, a leading or trailing `/`).
    EmptySegment(String),
}

impl fmt::Display for OfferPathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("an offer path cannot be empty"),
            Self::EmptySegment(text) => write!(f, "offer path `{text}` has an empty segment"),
        }
    }
}

impl std::error::Error for OfferPathError {}

/// A verb or namespace segment is non-empty and has no `.` (a node
/// segment's mark) and no `/` (the separator).
fn debug_assert_verb(segment: &str) {
    debug_assert!(
        !segment.is_empty() && !segment.contains('.') && !segment.contains('/'),
        "offer verb segment `{segment}` must be non-empty with no `.` or `/`"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_and_parse_round_trip() {
        let address = ProjectNodeAddress::parse("/demo.module/orbit.shader").unwrap();
        let path = OfferPath::project_node(&address).child("remove");

        assert_eq!(path.to_string(), "project/demo.module/orbit.shader/remove");
        assert_eq!(OfferPath::parse(&path.to_string()).unwrap(), path);
        assert_eq!(
            "project/save".parse::<OfferPath>().unwrap(),
            OfferPath::project().child("save")
        );
        assert_eq!(path.last(), Some("remove"));
        assert_eq!(path.len(), 4);
    }

    #[test]
    fn parse_refuses_empty_paths_and_segments() {
        assert_eq!(OfferPath::parse(""), Err(OfferPathError::Empty));
        for text in ["project//save", "/project", "project/"] {
            assert!(
                matches!(OfferPath::parse(text), Err(OfferPathError::EmptySegment(_))),
                "{text}"
            );
        }
    }

    #[test]
    fn starts_with_is_segment_wise() {
        let save = OfferPath::project().child("save");

        assert!(save.starts_with(&OfferPath::project()));
        assert!(save.starts_with(&save), "a path starts with itself");
        assert!(
            !save.starts_with(&OfferPath::root("proj")),
            "never a string prefix"
        );
        assert!(!OfferPath::project().starts_with(&save));

        let node = OfferPath::project_node(&ProjectNodeAddress::parse("/demo.module").unwrap());
        assert!(!node.starts_with(&OfferPath::parse("project/demo.mod").unwrap()));
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "must be non-empty with no `.`")]
    fn a_verb_with_a_dot_trips_the_debug_assertion() {
        let _ = OfferPath::project().child("orbit.shader");
    }
}
