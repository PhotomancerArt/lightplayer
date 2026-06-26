use lpc_model::{Revision, SlotShapeId};
use lpc_view::{ProjectView, apply_project_read_response};
use lpc_wire::{
    NodeReadQuery, NodeReadSelection, ProjectReadQuery, ProjectReadRequest, ProjectReadResponse,
    ProjectReadResult, ReadLevel, ResourcePayloadRead, ResourceReadQuery, RuntimeReadQuery,
    ShapeReadQuery,
};

use crate::{ProjectRuntimeSummary, ProjectSyncPhase, ProjectSyncSummary, UiError, UiIssue};

// Keep shape pages small. Some shape definitions include other shapes and can
// overflow the firmware's 16KB internal JSON buffer, which has caused project
// sync parse errors/crashes. Raise this only after the server buffer/streaming
// limitation is fixed.
const SHAPE_SYNC_PAGE_LIMIT: u32 = 4;
const SHAPE_SYNC_MAX_PAGES: u32 = 256;

pub struct ProjectSync {
    view: ProjectView,
    phase: ProjectSyncPhase,
    shape_cursor: Option<SlotShapeId>,
    shape_page_count: u32,
    shapes_complete: bool,
    issue: Option<UiIssue>,
}

impl ProjectSync {
    pub fn new() -> Self {
        Self {
            view: ProjectView::new(),
            phase: ProjectSyncPhase::Empty,
            shape_cursor: None,
            shape_page_count: 0,
            shapes_complete: false,
            issue: None,
        }
    }

    pub fn begin_initial_sync(&mut self) {
        *self = Self {
            phase: ProjectSyncPhase::SyncingShapes,
            ..Self::new()
        };
    }

    pub fn begin_refresh(&mut self) {
        self.phase = ProjectSyncPhase::SyncingProject;
        self.issue = None;
    }

    pub fn summary(&self) -> ProjectSyncSummary {
        ProjectSyncSummary {
            phase: self.phase,
            revision: self.view.revision.0,
            node_count: self.view.tree.nodes.len(),
            root_node_count: self
                .view
                .tree
                .nodes
                .values()
                .filter(|entry| entry.parent.is_none())
                .count(),
            slot_root_count: self.view.slots.roots.len(),
            resource_count: self.view.resource_cache.summary_count(),
            shape_count: self.view.slots.registry.iter().count(),
            shapes_complete: self.shapes_complete,
            runtime: self.view.runtime.as_ref().map(ProjectRuntimeSummary::from),
            issue: self.issue.clone(),
        }
    }

    /// Latest protocol/client project mirror.
    pub fn project_view(&self) -> &ProjectView {
        &self.view
    }

    pub fn is_ready(&self) -> bool {
        self.phase == ProjectSyncPhase::Ready
    }

    pub fn is_failed(&self) -> bool {
        self.phase == ProjectSyncPhase::Failed
    }

    pub fn is_syncing(&self) -> bool {
        matches!(
            self.phase,
            ProjectSyncPhase::SyncingShapes | ProjectSyncPhase::SyncingProject
        )
    }

    pub fn needs_shape_sync(&self) -> bool {
        !self.shapes_complete
    }

    pub fn shape_sync_request(&self) -> Result<ProjectReadRequest, UiError> {
        if self.shape_page_count >= SHAPE_SYNC_MAX_PAGES {
            return Err(UiError::Protocol(format!(
                "shape sync exceeded {SHAPE_SYNC_MAX_PAGES} pages"
            )));
        }
        Ok(shape_sync_request(self.shape_cursor))
    }

    pub fn initial_project_read_request(&mut self) -> ProjectReadRequest {
        self.phase = ProjectSyncPhase::SyncingProject;
        project_read_request(None, true)
    }

    pub fn refresh_project_read_request(&mut self) -> ProjectReadRequest {
        self.begin_refresh();
        let since = (self.view.revision != Revision::default()).then_some(self.view.revision);
        let include_slots = self.view.slots.roots.is_empty();
        project_read_request(since, include_slots)
    }

    pub fn apply_shape_sync_response(
        &mut self,
        response: ProjectReadResponse,
    ) -> Result<(), UiError> {
        let mut saw_shapes = false;
        for result in response.results {
            if let ProjectReadResult::Shapes(shapes) = result {
                saw_shapes = true;
                if let Some(registry) = shapes.registry {
                    self.view.slots.apply_registry_page(registry);
                }
                self.shapes_complete = shapes.complete;
                self.shape_cursor = shapes.next;
            }
        }
        if !saw_shapes {
            return Err(UiError::Protocol(
                "shape sync response did not include shapes".to_string(),
            ));
        }
        self.shape_page_count = self.shape_page_count.saturating_add(1);
        Ok(())
    }

    pub fn apply_project_read_response(
        &mut self,
        response: ProjectReadResponse,
    ) -> Result<(), UiError> {
        apply_project_read_response(&mut self.view, response)
            .map_err(|error| UiError::Protocol(error.to_string()))?;
        self.phase = ProjectSyncPhase::Ready;
        self.issue = None;
        Ok(())
    }

    pub fn fail(&mut self, issue: impl Into<String>) {
        self.phase = ProjectSyncPhase::Failed;
        self.issue = Some(UiIssue::new(issue));
    }
}

impl Default for ProjectSync {
    fn default() -> Self {
        Self::new()
    }
}

pub fn shape_sync_request(after: Option<SlotShapeId>) -> ProjectReadRequest {
    ProjectReadRequest {
        since: None,
        queries: Vec::from([ProjectReadQuery::Shapes(ShapeReadQuery {
            level: ReadLevel::Detail,
            after,
            limit: Some(SHAPE_SYNC_PAGE_LIMIT),
        })]),
        probes: Vec::new(),
    }
}

pub fn project_read_request(since: Option<Revision>, include_slots: bool) -> ProjectReadRequest {
    ProjectReadRequest {
        since,
        queries: Vec::from([
            ProjectReadQuery::Nodes(NodeReadQuery {
                level: ReadLevel::Detail,
                nodes: NodeReadSelection::All,
                include_slots,
            }),
            ProjectReadQuery::Resources(ResourceReadQuery {
                level: ReadLevel::Summary,
                payloads: ResourcePayloadRead::None,
            }),
            ProjectReadQuery::Runtime(RuntimeReadQuery),
        ]),
        probes: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shape_sync_request_uses_safe_page_limit_and_cursor() {
        let after = SlotShapeId::new(7);
        let request = shape_sync_request(Some(after));

        assert_eq!(request.since, None);
        assert!(request.probes.is_empty());
        assert_eq!(request.queries.len(), 1);
        assert_eq!(
            request.queries[0],
            ProjectReadQuery::Shapes(ShapeReadQuery {
                level: ReadLevel::Detail,
                after: Some(after),
                limit: Some(4),
            })
        );
    }

    #[test]
    fn project_read_request_includes_nodes_resources_and_runtime() {
        let request = project_read_request(Some(Revision::new(12)), true);

        assert_eq!(request.since, Some(Revision::new(12)));
        assert_eq!(request.queries.len(), 3);
        assert_eq!(
            request.queries[0],
            ProjectReadQuery::Nodes(NodeReadQuery {
                level: ReadLevel::Detail,
                nodes: NodeReadSelection::All,
                include_slots: true,
            })
        );
        assert_eq!(
            request.queries[1],
            ProjectReadQuery::Resources(ResourceReadQuery {
                level: ReadLevel::Summary,
                payloads: ResourcePayloadRead::None,
            })
        );
        assert_eq!(
            request.queries[2],
            ProjectReadQuery::Runtime(RuntimeReadQuery)
        );
        assert!(request.probes.is_empty());
    }

    #[test]
    fn refresh_request_includes_slots_when_roots_are_missing() {
        let mut sync = ProjectSync::new();
        sync.view.revision = Revision::new(9);

        let request = sync.refresh_project_read_request();

        assert_eq!(request.since, Some(Revision::new(9)));
        assert_eq!(
            request.queries[0],
            ProjectReadQuery::Nodes(NodeReadQuery {
                level: ReadLevel::Detail,
                nodes: NodeReadSelection::All,
                include_slots: true,
            })
        );
    }
}
