use crate::prop::prop_namespace::PropNamespace;
use crate::prop::prop_path::{PropPath, Segment};
use crate::tree::tree_path::{PathError, TreePath};
use core::fmt;
use lps_shared::path;

/// Error from [`NodePropSpec::parse`]: missing or duplicate `#` separator, a
/// path error, or a property path parse error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NodePropSpecError {
    /// No `#` between node path and property path.
    MissingHash,
    /// More than one `#`.
    ExtraHash,
    /// The left-hand [`TreePath`] could not be parsed.
    Path(PathError),
    /// The right-hand property string could not be parsed.
    Prop(path::PathParseError),
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

/// A **node + property** address: a [`TreePath`], a single `#`, then a
/// [`PropPath`]. `Display` is round-trippable (see module tests) and matches
/// the v0 `node#property` form in
/// `docs/plans-old/2026-04-22-lp-domain-m2-domain-skeleton/00-design.md` (`NodePropSpec`).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct NodePropSpec {
    /// Which node in the project graph.
    pub node: TreePath,
    /// Which property (possibly nested) on that node.
    pub prop: PropPath,
}

impl NodePropSpec {
    /// Parses `node#prop` where `node` is a [`TreePath`] string and `prop` is
    /// a property path for [`path::parse_path`].
    pub fn parse(s: &str) -> Result<Self, NodePropSpecError> {
        let (node_part, prop_part) = s.split_once('#').ok_or(NodePropSpecError::MissingHash)?;
        if prop_part.contains('#') {
            return Err(NodePropSpecError::ExtraHash);
        }
        let node = TreePath::parse(node_part).map_err(NodePropSpecError::Path)?;
        let prop = path::parse_path(prop_part).map_err(NodePropSpecError::Prop)?;
        Ok(NodePropSpec { node, prop })
    }
}

impl fmt::Display for NodePropSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}#", self.node)?;
        for (i, seg) in self.prop.iter().enumerate() {
            if i > 0 && matches!(seg, Segment::Field(_)) {
                f.write_str(".")?;
            }
            match seg {
                path::LpsPathSeg::Field(name) => f.write_str(name)?,
                path::LpsPathSeg::Index(idx) => write!(f, "[{idx}]")?,
            }
        }
        Ok(())
    }
}

impl NodePropSpec {
    /// The top-level namespace of `prop`. Returns `None` if `prop` is
    /// empty or its head segment is not a recognised namespace name.
    ///
    /// M4.3 config-load uses this to enforce "NodeProp targets must
    /// address `outputs`" (per design 06 §"NodeProp resolution").
    pub fn target_namespace(&self) -> Option<PropNamespace> {
        PropNamespace::from_prop_path(&self.prop)
    }
}

#[cfg(test)]
mod tests {
    use super::NodePropSpec;
    use crate::prop::prop_namespace::PropNamespace;
    use alloc::string::ToString;

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
    fn target_namespace_outputs() {
        let spec = NodePropSpec::parse("/main.show#outputs[0]").unwrap();
        assert_eq!(spec.target_namespace(), Some(PropNamespace::Outputs));
    }

    #[test]
    fn target_namespace_params() {
        let spec = NodePropSpec::parse("/x.y#params.speed").unwrap();
        assert_eq!(spec.target_namespace(), Some(PropNamespace::Params));
    }

    #[test]
    fn target_namespace_unknown_returns_none() {
        let spec = NodePropSpec::parse("/x.y#weird.field").unwrap();
        assert_eq!(spec.target_namespace(), None);
    }

    #[test]
    fn target_namespace_inputs() {
        let spec = NodePropSpec::parse("/audio.node#inputs.level").unwrap();
        assert_eq!(spec.target_namespace(), Some(PropNamespace::Inputs));
    }

    #[test]
    fn target_namespace_state() {
        let spec = NodePropSpec::parse("/counter.node#state.value").unwrap();
        assert_eq!(spec.target_namespace(), Some(PropNamespace::State));
    }
}
