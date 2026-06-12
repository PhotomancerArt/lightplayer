//! Effective project graph node entry.

use crate::{NodeDefLocation, NodeInvocation, ProjectNodeKey, ProjectNodeRole, SlotPath};

/// One effective project node instance.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectNodeEntry {
    pub key: ProjectNodeKey,
    pub parent: Option<ProjectNodeKey>,
    pub def_location: NodeDefLocation,
    pub origin: ProjectNodeOrigin,
}

impl ProjectNodeEntry {
    pub fn root(key: ProjectNodeKey, def_location: NodeDefLocation) -> Self {
        Self {
            key,
            parent: None,
            def_location,
            origin: ProjectNodeOrigin::Root,
        }
    }

    pub fn invocation(
        key: ProjectNodeKey,
        parent: ProjectNodeKey,
        def_location: NodeDefLocation,
        slot: SlotPath,
        role: ProjectNodeRole,
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

/// How a project graph node instance appears in authored project topology.
#[derive(Clone, Debug, PartialEq)]
pub enum ProjectNodeOrigin {
    Root,
    Invocation {
        slot: SlotPath,
        role: ProjectNodeRole,
        invocation: NodeInvocation,
    },
}
