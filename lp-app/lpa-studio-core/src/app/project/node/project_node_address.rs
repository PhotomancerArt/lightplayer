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
}
