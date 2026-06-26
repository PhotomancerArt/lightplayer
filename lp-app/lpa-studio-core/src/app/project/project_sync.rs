use std::collections::BTreeMap;

use lpc_model::{Revision, SlotShapeId, VisualProduct};
use lpc_view::{ProjectView, apply_project_read_response};
use lpc_wire::{
    NodeReadQuery, NodeReadSelection, ProjectProbeRequest, ProjectProbeResult, ProjectReadQuery,
    ProjectReadRequest, ProjectReadResponse, ProjectReadResult, ReadLevel,
    RenderProductProbeRequest, RenderProductProbeResult, ResourcePayloadRead, ResourceReadQuery,
    RuntimeReadQuery, ShapeReadQuery, WireTextureFormat,
};

use crate::{
    ProjectRuntimeSummary, ProjectSyncPhase, ProjectSyncSummary, UiError, UiIssue,
    UiProductPreview, UiProductRef,
};

// Keep shape pages small. Some shape definitions include other shapes and can
// overflow the firmware's 16KB internal JSON buffer, which has caused project
// sync parse errors/crashes. Raise this only after the server buffer/streaming
// limitation is fixed.
const SHAPE_SYNC_PAGE_LIMIT: u32 = 4;
const SHAPE_SYNC_MAX_PAGES: u32 = 256;
const PRODUCT_PREVIEW_WIDTH: u32 = 64;
const PRODUCT_PREVIEW_HEIGHT: u32 = 36;

pub struct ProjectSync {
    view: ProjectView,
    phase: ProjectSyncPhase,
    shape_cursor: Option<SlotShapeId>,
    shape_page_count: u32,
    shapes_complete: bool,
    product_previews: BTreeMap<UiProductRef, UiProductPreview>,
    requested_product_previews: Vec<UiProductRef>,
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
            product_previews: BTreeMap::new(),
            requested_product_previews: Vec::new(),
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

    /// Latest cached preview state for a produced product.
    pub fn product_preview(&self, product: &UiProductRef) -> Option<&UiProductPreview> {
        self.product_previews.get(product)
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

    pub fn initial_project_read_request(
        &mut self,
        visual_products: Vec<VisualProduct>,
    ) -> ProjectReadRequest {
        self.phase = ProjectSyncPhase::SyncingProject;
        project_read_request(None, true, self.product_probe_requests(visual_products))
    }

    pub fn refresh_project_read_request(
        &mut self,
        visual_products: Vec<VisualProduct>,
    ) -> ProjectReadRequest {
        self.begin_refresh();
        let since = (self.view.revision != Revision::default()).then_some(self.view.revision);
        // Runtime state slots carry live values/products, so refresh snapshots
        // include slots even when the tree itself only changes by revision.
        let include_slots = true;
        project_read_request(
            since,
            include_slots,
            self.product_probe_requests(visual_products),
        )
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
        self.apply_product_probe_results(&response.probes);
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

    fn product_probe_requests(
        &mut self,
        visual_products: Vec<VisualProduct>,
    ) -> Vec<ProjectProbeRequest> {
        self.requested_product_previews = visual_products
            .iter()
            .copied()
            .map(UiProductRef::from_visual_product)
            .collect();
        for product in &self.requested_product_previews {
            self.product_previews
                .entry(*product)
                .or_insert(UiProductPreview::Pending);
        }
        visual_products
            .into_iter()
            .map(|product| {
                ProjectProbeRequest::RenderProduct(RenderProductProbeRequest {
                    product,
                    width: PRODUCT_PREVIEW_WIDTH,
                    height: PRODUCT_PREVIEW_HEIGHT,
                    format: WireTextureFormat::Srgb8,
                })
            })
            .collect()
    }

    fn apply_product_probe_results(&mut self, probes: &[ProjectProbeResult]) {
        let requested = core::mem::take(&mut self.requested_product_previews);
        for (index, probe) in probes.iter().enumerate() {
            let fallback_key = requested.get(index).copied();
            if let Some((product, preview)) = product_preview_from_probe(probe, fallback_key) {
                self.product_previews.insert(product, preview);
            }
        }
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

pub fn project_read_request(
    since: Option<Revision>,
    include_slots: bool,
    probes: Vec<ProjectProbeRequest>,
) -> ProjectReadRequest {
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
        probes,
    }
}

fn product_preview_from_probe(
    probe: &ProjectProbeResult,
    fallback_key: Option<UiProductRef>,
) -> Option<(UiProductRef, UiProductPreview)> {
    match probe {
        ProjectProbeResult::RenderProduct(RenderProductProbeResult::Texture {
            product,
            revision,
            width,
            height,
            format: WireTextureFormat::Srgb8,
            bytes,
        }) => Some((
            UiProductRef::from_visual_product(*product),
            UiProductPreview::VisualSrgb8 {
                width: *width,
                height: *height,
                revision: revision.0,
                bytes: bytes.clone(),
            },
        )),
        ProjectProbeResult::RenderProduct(RenderProductProbeResult::Texture {
            product,
            format,
            ..
        }) => Some((
            UiProductRef::from_visual_product(*product),
            UiProductPreview::Unsupported {
                reason: format!("visual preview format {format:?} is not supported by Studio"),
            },
        )),
        ProjectProbeResult::RenderProduct(RenderProductProbeResult::Unsupported { reason }) => {
            fallback_key.map(|product| {
                (
                    product,
                    UiProductPreview::Unsupported {
                        reason: reason.clone(),
                    },
                )
            })
        }
        ProjectProbeResult::RenderProduct(RenderProductProbeResult::Error { message }) => {
            fallback_key.map(|product| {
                (
                    product,
                    UiProductPreview::Error {
                        message: message.clone(),
                    },
                )
            })
        }
        ProjectProbeResult::ExplainSlot(_) => None,
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
        let request = project_read_request(Some(Revision::new(12)), true, Vec::new());

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

        let request = sync.refresh_project_read_request(Vec::new());

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

    #[test]
    fn refresh_request_includes_slots_after_roots_exist() {
        let mut sync = ProjectSync::new();
        sync.view.revision = Revision::new(9);
        sync.view.slots.roots.insert(
            "node.1.state".to_string(),
            lpc_model::SlotData::Unit {
                revision: Revision::new(9),
            },
        );

        let request = sync.refresh_project_read_request(Vec::new());

        assert_eq!(
            request.queries[0],
            ProjectReadQuery::Nodes(NodeReadQuery {
                level: ReadLevel::Detail,
                nodes: NodeReadSelection::All,
                include_slots: true,
            })
        );
    }

    #[test]
    fn refresh_request_includes_visual_product_probes() {
        let mut sync = ProjectSync::new();
        let product = VisualProduct::new(lpc_model::NodeId::new(7), 2);

        let request = sync.refresh_project_read_request(vec![product]);

        assert_eq!(request.probes.len(), 1);
        assert_eq!(
            request.probes[0],
            ProjectProbeRequest::RenderProduct(RenderProductProbeRequest {
                product,
                width: PRODUCT_PREVIEW_WIDTH,
                height: PRODUCT_PREVIEW_HEIGHT,
                format: WireTextureFormat::Srgb8,
            })
        );
        assert_eq!(
            sync.product_preview(&UiProductRef::from_visual_product(product)),
            Some(&UiProductPreview::Pending)
        );
    }

    #[test]
    fn project_read_response_caches_visual_product_preview() {
        let mut sync = ProjectSync::new();
        let product = VisualProduct::new(lpc_model::NodeId::new(7), 2);
        let _ = sync.refresh_project_read_request(vec![product]);
        let bytes = vec![1, 2, 3, 4, 5, 6];

        sync.apply_project_read_response(ProjectReadResponse {
            revision: Revision::new(9),
            results: Vec::new(),
            probes: vec![ProjectProbeResult::RenderProduct(
                RenderProductProbeResult::Texture {
                    product,
                    revision: Revision::new(8),
                    width: 1,
                    height: 2,
                    format: WireTextureFormat::Srgb8,
                    bytes: bytes.clone(),
                },
            )],
        })
        .unwrap();

        assert_eq!(
            sync.product_preview(&UiProductRef::from_visual_product(product)),
            Some(&UiProductPreview::VisualSrgb8 {
                width: 1,
                height: 2,
                revision: 8,
                bytes,
            })
        );
    }
}
