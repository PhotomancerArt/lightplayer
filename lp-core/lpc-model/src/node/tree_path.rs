use crate::{NodeName, NodeNameError};
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

/// A **hierarchical** node address: a leading `/`, then one or more
/// [`NodePathSegment`]s (each `name.type`), concatenated. Display never puts a
/// trailing slash. Example: `/main.show/fluid.vis` (see
/// `docs/plans-old/2026-04-22-lp-domain-m2-domain-skeleton/00-design.md` and unit tests
/// in this module).
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct TreePath(pub Vec<NodePathSegment>);

impl TreePath {
    /// Parses a string into a [`TreePath`], enforcing a leading `/` and the
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
            let name = NodeName::parse(name).map_err(PathError::InvalidName)?;
            let ty = NodeName::parse(ty).map_err(PathError::InvalidName)?;
            segments.push(NodePathSegment { name, ty });
        }
        Ok(TreePath(segments))
    }
}

impl fmt::Display for TreePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for seg in &self.0 {
            write!(f, "/{}.{}", seg.name, seg.ty)?;
        }
        Ok(())
    }
}

/// Error from [`TreePath::parse`]: empty path, bad slash layout, a segment
/// missing `name.type`, or a nested [`NodeNameError`].
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
    /// A [`NodeName`] in a segment failed validation.
    InvalidName(NodeNameError),
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
#[cfg(test)]
mod tests {
    use super::TreePath;
    use alloc::string::ToString;

    #[test]
    fn node_path_round_trips() {
        for s in [
            "/main.show",
            "/main.show/fluid.vis",
            "/dome.rig/main.layout/sector4.fixture",
        ] {
            let parsed = TreePath::parse(s).unwrap();
            assert_eq!(parsed.to_string(), s);
        }
    }

    #[test]
    fn node_path_rejects_malformed() {
        for s in ["", "main.show", "/", "//", "/main", "/main.show//x.y"] {
            assert!(TreePath::parse(s).is_err(), "should have rejected {s:?}");
        }
    }
}

/// A single path segment: **`name`**.**`ty`**, the atom of a [`TreePath`]
/// (examples in tests: `main.show`, `fluid.vis` — see `m2` design: slash-joined
/// `/<name>.<type>/…`).
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct NodePathSegment {
    /// Instance name in the parent container.
    pub name: NodeName,
    /// Node *kind* (type tag) in the design-doc sense (e.g. `show`, `vis`).
    pub ty: NodeName,
}
