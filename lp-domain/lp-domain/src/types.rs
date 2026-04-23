//! **Identity and addressing** types for the domain layer: runtime identity,
//! human-readable names, node paths, property paths, and bus channel names.
//!
//! These are separate from the Quantity model (`Kind`, `Shape`, …) but are how
//! authored graphs, runtime nodes, and the bus are *named* in `docs/roadmaps/2026-04-22-lp-domain/m2-domain-skeleton.md` and
//! `docs/plans-old/2026-04-22-lp-domain-m2-domain-skeleton/00-design.md`.
//! [`Uid`] is a cheap process-local handle; strings like [`Name`] and [`NodePath`]
//! are the stable authored-addressing story.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

// --- Uid ----------------------------------------------------------------

/// Opaque **runtime** node id: a compact `u32` handle, never a place-holding
/// string in authored TOML.
///
/// v0 uses `u32` for embedded *performance* (Copy, no heap, cheap compare/hash)
/// instead of a base-62 string, per
/// `docs/plans-old/2026-04-22-lp-domain-m2-domain-skeleton/summary.md` (Uid
/// decision). **Authored** identity uses [`NodePath`] and [`NodePropSpec`], not
/// a serialized Uid in artifact files (see same summary: “runtime-only”). Serde
/// derives on this type support schema/tests in this crate; persisted authored
/// graphs use string paths (e.g. [`NodePath`]), not embedding Uid in TOML, per
/// the same M2 “runtime-only / addressing split” story.
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

/// Parse failure for [`Name::parse`]: empty string, disallowed first character, or a character outside the allowed set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NameError {
    /// Empty input.
    Empty,
    /// First character is ASCII digit (names must be identifiers, `m2` design: `[A-Za-z0-9_]+` with no leading digit).
    LeadingDigit,
    /// A character is not in `[A-Za-z0-9_]`.
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

/// A **human-readable label** and path segment: non-empty, ASCII
/// alphanumerics and `_`, and must not start with a digit. Used inside
/// [`NodePathSegment`] and struct field keys (see
/// `docs/plans-old/2026-04-22-lp-domain-m2-domain-skeleton/00-design.md` — `Name` grammar).
#[derive(
    Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct Name(pub String);

impl Name {
    /// Parses and validates a [`Name`] from a string, enforcing the v0
    /// character rules above.
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

/// Error from [`NodePath::parse`]: empty path, bad slash layout, a segment
/// missing `name.type`, or a nested [`NameError`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PathError {
    /// The input was empty (after optional leading `/`).
    Empty,
    /// The path does not start with `/`.
    MissingLeadingSlash,
    /// A `//` or trailing slash produced an empty segment.
    EmptySegment,
    /// A segment is not `name.type` (no `.` with a type part).
    SegmentMissingType(String),
    /// A [`Name`] in a segment failed validation.
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

/// A single path segment: **`name`**.**`ty`**, the atom of a [`NodePath`]
/// (examples in tests: `main.show`, `fluid.vis` — see `m2` design: slash-joined
/// `/<name>.<type>/…`).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct NodePathSegment {
    /// Instance name in the parent container.
    pub name: Name,
    /// Node *kind* (type tag) in the design-doc sense (e.g. `show`, `vis`).
    pub ty: Name,
}

/// A **hierarchical** node address: a leading `/`, then one or more
/// [`NodePathSegment`]s (each `name.type`), concatenated. Display never puts a
/// trailing slash. Example: `/main.show/fluid.vis` (see
/// `docs/plans-old/2026-04-22-lp-domain-m2-domain-skeleton/00-design.md` and unit tests
/// in this module).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct NodePath(pub Vec<NodePathSegment>);

impl NodePath {
    /// Parses a string into a [`NodePath`], enforcing a leading `/` and the
    /// per-segment `name.ty` form.
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

/// Re-exports for parsing **property** paths: dot fields and array indices, as
/// used by the shared GLSL/struct path layer in `lps_shared`.
pub mod prop_path {
    /// Re-export: path segment (field name or array index).
    pub use lps_shared::path::{LpsPathSeg as Segment, PathParseError, parse_path};
}

/// A parsed property path: `field`, `a.b[0]`, `config.spacing`, etc. (wire form
/// is a string; see `prop_path::parse_path` and
/// `docs/plans-old/2026-04-22-lp-domain-m2-domain-skeleton/00-design.md` — `PropPath`
/// = `lps_shared::path`).
pub type PropPath = Vec<prop_path::Segment>;

// --- NodePropSpec -------------------------------------------------------

/// Error from [`NodePropSpec::parse`]: missing or duplicate `#` separator, a
/// path error, or a property path parse error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NodePropSpecError {
    /// No `#` between node path and property path.
    MissingHash,
    /// More than one `#`.
    ExtraHash,
    /// The left-hand [`NodePath`] could not be parsed.
    Path(PathError),
    /// The right-hand property string could not be parsed.
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

/// A **node + property** address: a [`NodePath`], a single `#`, then a
/// [`PropPath`]. `Display` is round-trippable (see module tests) and matches
/// the v0 `node#property` form in
/// `docs/plans-old/2026-04-22-lp-domain-m2-domain-skeleton/00-design.md` (`NodePropSpec`).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct NodePropSpec {
    /// Which node in the project graph.
    pub node: NodePath,
    /// Which property (possibly nested) on that node.
    pub prop: PropPath,
}

impl NodePropSpec {
    /// Parses `node#prop` where `node` is a [`NodePath`] string and `prop` is
    /// a property path for [`prop_path::parse_path`].
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

/// A **string payload** for referring to an on-disk *artifact* (pattern, effect, …) from
/// another file. v0 is intentionally opaque and file-resolution rules land in
/// M3+ (`docs/roadmaps/2026-04-22-lp-domain/overview.md` — “Artifact resolution model is intentionally minimal in v0”).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct ArtifactSpec(pub String);

impl fmt::Display for ArtifactSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

// --- ChannelName --------------------------------------------------------

/// A **bus channel** name: convention-only string with shape like
/// `<sort>/<in|out>/<id>/…` (e.g. `time`, `video/in/0`, `audio/in/0`), as in
/// `docs/design/lightplayer/quantity.md` §8 and §11 (channel naming). The type
/// does not enforce the grammar in v0; compose-time code validates against the
/// project’s bus graph.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(transparent)]
pub struct ChannelName(pub String);

impl fmt::Display for ChannelName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

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
