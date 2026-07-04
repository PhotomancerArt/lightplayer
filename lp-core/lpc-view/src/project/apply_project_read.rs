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
                    view.slots.apply_roots_snapshot(slots)?;
                }
            }
            ProjectReadResult::Resources(resources) => {
                view.resource_cache.apply_summaries(&resources.summaries);
                view.resource_cache
                    .apply_runtime_buffer_payloads(&resources.runtime_buffer_payloads);
            }
            ProjectReadResult::Runtime(runtime) => {
                view.runtime = Some(runtime);
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
        NodeReadResult, NodeRuntimeStatus, ProjectReadResponse, ReadLevel, ResourceReadResult,
        WireEntryState, WireTreeDelta,
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
                    status: NodeRuntimeStatus::Created,
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
                membership: None,
            })],
            probes: vec![],
        };

        apply_project_read_response(&mut view, response).unwrap();

        assert_eq!(view.revision, Revision::new(1));
    }

    #[test]
    fn apply_project_read_retains_runtime_status() {
        let mut view = ProjectView::new();
        let response = ProjectReadResponse {
            revision: Revision::new(9),
            results: vec![ProjectReadResult::Runtime(lpc_wire::RuntimeReadResult {
                project: lpc_wire::ProjectRuntimeStatus {
                    revision: Revision::new(9),
                    frame_num: 42,
                    frame_delta_ms: 16,
                    frame_total_ms: 17,
                    demand_root_count: 2,
                    runtime_buffer_count: 3,
                },
                server: Some(lpc_wire::ServerRuntimeStatus {
                    theoretical_fps: Some(60.0),
                    last_frame_time_us: Some(16_000),
                    memory: Some(lpc_wire::server::MemoryStats {
                        free_bytes: 1024,
                        used_bytes: 2048,
                        total_bytes: 3072,
                    }),
                }),
            })],
            probes: vec![],
        };

        apply_project_read_response(&mut view, response).unwrap();

        let runtime = view.runtime.as_ref().expect("runtime retained");
        assert_eq!(runtime.project.frame_num, 42);
        assert_eq!(runtime.project.runtime_buffer_count, 3);
        assert_eq!(
            runtime
                .server
                .as_ref()
                .and_then(|server| server.memory.as_ref()),
            Some(&lpc_wire::server::MemoryStats {
                free_bytes: 1024,
                used_bytes: 2048,
                total_bytes: 3072,
            })
        );
    }
}
