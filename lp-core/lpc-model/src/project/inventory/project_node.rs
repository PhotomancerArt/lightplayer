use crate::{NodeDefLocation, NodeInvocation, ProjectNodeLocation, ProjectNodePlacement, SlotPath};

/// One effective project node instance.
///
/// A project node is an occurrence in the expanded [`crate::ProjectTree`]. It
/// points at the [`crate::NodeDefLocation`] that supplies its definition, but it
/// is not itself definition identity and is not a runtime node.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectNode {
    /// Stable project-tree location for this node occurrence.
    pub key: ProjectNodeLocation,
    /// Parent node occurrence, or `None` for the project root.
    pub parent: Option<ProjectNodeLocation>,
    /// Effective definition used by this node occurrence.
    pub def_location: NodeDefLocation,
    /// Authored origin of this occurrence.
    pub origin: ProjectNodeOrigin,
}

impl ProjectNode {
    pub fn root(key: ProjectNodeLocation, def_location: NodeDefLocation) -> Self {
        Self {
            key,
            parent: None,
            def_location,
            origin: ProjectNodeOrigin::Root,
        }
    }

    pub fn invocation(
        key: ProjectNodeLocation,
        parent: ProjectNodeLocation,
        def_location: NodeDefLocation,
        slot: SlotPath,
        role: ProjectNodePlacement,
        invocation: NodeInvocation,
    ) -> Self {
        Self {
            key,
            parent: Some(parent),
            def_location,
            origin: ProjectNodeOrigin::Invocation {
                slot,
                role,
                invocation,
            },
        }
    }
}

/// How a project node occurrence appears in authored project topology.
#[derive(Clone, Debug, PartialEq)]
pub enum ProjectNodeOrigin {
    /// Root project node.
    Root,
    /// Child produced by a parent-owned [`crate::NodeInvocation`] slot.
    Invocation {
        /// Slot path of the invocation within the parent definition.
        slot: SlotPath,
        /// Placement metadata for the parent container.
        role: ProjectNodePlacement,
        /// Authored invocation value at `slot`.
        invocation: NodeInvocation,
    },
}
