//! Effective project graph node entry.

use crate::{NodeDefLocation, NodeInvocation, ProjectNodeLocation, ProjectNodePlacement, SlotPath};

/// One effective project node instance.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectNode {
    pub key: ProjectNodeLocation,
    pub parent: Option<ProjectNodeLocation>,
    pub def_location: NodeDefLocation,
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

/// How a project graph node instance appears in authored project topology.
#[derive(Clone, Debug, PartialEq)]
pub enum ProjectNodeOrigin {
    Root,
    Invocation {
        slot: SlotPath,
        role: ProjectNodePlacement,
        invocation: NodeInvocation,
    },
}
