use core::fmt;

use lpc_model::{PathError, TreePath};

/// Stable authored address for a node inside a LightPlayer project.
///
/// Studio uses this as the node controller key. Runtime `NodeId`s can change
/// when the project reconnects or reloads, but a stable `TreePath` lets local
/// UI/controller state survive ordinary mirror updates.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ProjectNodeAddress {
    path: TreePath,
}

impl ProjectNodeAddress {
    /// Create an address from an already parsed tree path.
    pub fn new(path: TreePath) -> Self {
        Self { path }
    }

    /// Parse an authored tree path.
    pub fn parse(path: &str) -> Result<Self, PathError> {
        TreePath::parse(path).map(Self::new)
    }

    /// Return the underlying model path.
    pub fn path(&self) -> &TreePath {
        &self.path
    }

    /// True when this address is `ancestor` itself or a node inside its
    /// subtree (segment-wise tree-path prefix, never a string prefix).
    pub fn is_self_or_under(&self, ancestor: &ProjectNodeAddress) -> bool {
        self.path.0.starts_with(&ancestor.path.0)
    }
}

impl fmt::Display for ProjectNodeAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.path)
    }
}

impl From<TreePath> for ProjectNodeAddress {
    fn from(path: TreePath) -> Self {
        Self::new(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_displays_canonical_tree_path() {
        let address = ProjectNodeAddress::parse("/demo.project/orbit.shader").unwrap();

        assert_eq!(address.to_string(), "/demo.project/orbit.shader");
    }

    #[test]
    fn is_self_or_under_is_a_segment_wise_subtree_test() {
        let root = ProjectNodeAddress::parse("/demo.project").unwrap();
        let node = ProjectNodeAddress::parse("/demo.project/orbit.shader").unwrap();
        let nested = ProjectNodeAddress::parse("/demo.project/orbit.shader/tail.vis").unwrap();
        let sibling = ProjectNodeAddress::parse("/demo.project/clock.clock").unwrap();

        assert!(node.is_self_or_under(&node), "a node is in its own subtree");
        assert!(node.is_self_or_under(&root));
        assert!(nested.is_self_or_under(&node));
        assert!(
            !root.is_self_or_under(&node),
            "never in the child direction"
        );
        assert!(!sibling.is_self_or_under(&node));
    }
}
