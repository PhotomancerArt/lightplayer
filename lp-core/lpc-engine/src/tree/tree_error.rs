//! Errors from tree mutation operations.

use alloc::string::String;
use lpc_model::{NodeId, NodeName, TreePath};

/// Error from tree operations.
#[derive(Clone, Debug, PartialEq)]
pub enum TreeError {
    /// Tried to add a child with a name that already exists under this parent.
    SiblingNameCollision { parent: NodeId, name: NodeName },
    /// Referenced a node id that doesn't exist (or was destroyed).
    UnknownNode(NodeId),
    /// Referenced a path that doesn't exist in the tree.
    UnknownPath(TreePath),
    /// Tried to mutate the root (remove it, or add a sibling to it).
    RootMutation,
    /// Tried to add a child to a node that is not in the tree (tombstone slot).
    NotInTree(NodeId),
    /// Internal: node id out of bounds for the slot vector.
    InvalidSlotIndex(u32),
    /// Custom error message for rare edge cases.
    Custom(String),
}

impl core::fmt::Display for TreeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            TreeError::SiblingNameCollision { parent, name } => {
                write!(
                    f,
                    "sibling name collision: {name} already exists under {parent}"
                )
            }
            TreeError::UnknownNode(id) => write!(f, "unknown node: {id}"),
            TreeError::UnknownPath(path) => write!(f, "unknown path: {path}"),
            TreeError::RootMutation => f.write_str("cannot mutate root"),
            TreeError::NotInTree(id) => write!(f, "node not in tree: {id}"),
            TreeError::InvalidSlotIndex(idx) => write!(f, "invalid slot index: {idx}"),
            TreeError::Custom(msg) => f.write_str(msg),
        }
    }
}

impl core::error::Error for TreeError {}

#[cfg(test)]
mod tests {
    use super::TreeError;
    use lpc_model::{NodeId, NodeName};

    #[test]
    fn tree_error_display() {
        let err = TreeError::SiblingNameCollision {
            parent: NodeId::new(1),
            name: NodeName::parse("foo").unwrap(),
        };
        let s = alloc::format!("{}", err);
        assert!(s.contains("foo"));
        assert!(s.contains("1"));
    }
}
