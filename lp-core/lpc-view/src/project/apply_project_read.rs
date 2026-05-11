//! Apply stateless project read responses to [`ProjectView`].

use lpc_wire::{ProjectReadResponse, ProjectReadResult};

use super::ProjectView;
use crate::slot::SlotMirrorError;
use crate::tree::{ApplyError, apply_tree_deltas};

/// Error applying a project read response.
#[derive(Clone, Debug, PartialEq)]
pub enum ProjectReadApplyError {
    Tree(ApplyError),
    Slot(SlotMirrorError),
}

impl core::fmt::Display for ProjectReadApplyError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Tree(error) => write!(f, "tree apply error: {error}"),
            Self::Slot(error) => write!(f, "slot apply error: {error}"),
        }
    }
}

impl core::error::Error for ProjectReadApplyError {}

impl From<ApplyError> for ProjectReadApplyError {
    fn from(value: ApplyError) -> Self {
        Self::Tree(value)
    }
}

impl From<SlotMirrorError> for ProjectReadApplyError {
    fn from(value: SlotMirrorError) -> Self {
        Self::Slot(value)
    }
}

/// Apply one stateless project read response to a client-side mirror.
pub fn apply_project_read_response(
    view: &mut ProjectView,
    response: ProjectReadResponse,
) -> Result<(), ProjectReadApplyError> {
    let revision = response.revision;
    for result in response.results {
        match result {
            ProjectReadResult::Shapes(shapes) => {
                if let Some(registry) = shapes.registry {
                    view.slots.apply_registry_snapshot(registry);
                }
            }
            ProjectReadResult::Nodes(nodes) => {
                apply_tree_deltas(&mut view.tree, &nodes.tree_deltas, revision)?;
                if let Some(slots) = nodes.slots {
                    view.slots.apply_full_sync(slots);
                }
            }
            ProjectReadResult::Resources(resources) => {
                view.resource_cache.apply_summaries(&resources.summaries);
                view.resource_cache
                    .apply_runtime_buffer_payloads(&resources.runtime_buffer_payloads);
            }
        }
    }
    view.revision = revision;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use lpc_model::{NodeId, Revision, TreePath};
    use lpc_wire::{
        NodeReadResult, ProjectReadResponse, ReadLevel, ResourceReadResult, WireEntryState,
        WireNodeStatus, WireTreeDelta,
    };

    #[test]
    fn apply_project_read_updates_revision_and_tree() {
        let mut view = ProjectView::new();
        let response = ProjectReadResponse {
            revision: Revision::new(3),
            results: vec![ProjectReadResult::Nodes(NodeReadResult {
                level: ReadLevel::Detail,
                tree_deltas: vec![WireTreeDelta::Created {
                    id: NodeId::new(0),
                    path: TreePath::parse("/basic.project").unwrap(),
                    parent: None,
                    child_kind: None,
                    children: vec![],
                    status: WireNodeStatus::Created,
                    state: WireEntryState::Pending,
                    created_frame: Revision::new(0),
                    change_frame: Revision::new(0),
                    children_ver: Revision::new(0),
                }],
                slots: None,
            })],
            probes: vec![],
        };

        apply_project_read_response(&mut view, response).unwrap();

        assert_eq!(view.revision, Revision::new(3));
        assert!(view.tree.get(NodeId::new(0)).is_some());
    }

    #[test]
    fn apply_project_read_updates_resource_cache() {
        let mut view = ProjectView::new();
        let response = ProjectReadResponse {
            revision: Revision::new(1),
            results: vec![ProjectReadResult::Resources(ResourceReadResult {
                level: ReadLevel::Summary,
                summaries: vec![],
                runtime_buffer_payloads: vec![],
            })],
            probes: vec![],
        };

        apply_project_read_response(&mut view, response).unwrap();

        assert_eq!(view.revision, Revision::new(1));
    }
}
