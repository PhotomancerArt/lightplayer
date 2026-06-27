use std::collections::BTreeMap;

use lpc_model::{ControlDisplayLayout, Revision, SlotShapeId};
use lpc_view::{ProjectView, apply_project_read_response};
use lpc_wire::{
    ControlDisplayLayoutProbeResult, ControlDisplayLayoutRead, ControlProductProbeRequest,
    ControlProductProbeResult, NodeReadQuery, NodeReadSelection, ProjectProbeRequest,
    ProjectProbeResult, ProjectReadQuery, ProjectReadRequest, ProjectReadResponse,
    ProjectReadResult, ReadLevel, RenderProductProbeRequest, RenderProductProbeResult,
    ResourcePayloadRead, ResourceReadQuery, RuntimeReadQuery, ShapeReadQuery,
    WireChannelSampleFormat, WireTextureFormat,
};

use crate::{
    ProjectRuntimeSummary, ProjectSyncPhase, ProjectSyncSummary, UiControlProductPreview,
    UiControlSampleFormat, UiError, UiIssue, UiProductPreview, UiProductPreviewFrame, UiProductRef,
};

// Keep shape pages small. Some shape definitions include other shapes and can
// inflate quickly on firmware serial links. Raise this only after the project
// read transport has a true async JSON writer instead of a bounded chunk queue.
const SHAPE_SYNC_PAGE_LIMIT: u32 = 4;
const SHAPE_SYNC_MAX_PAGES: u32 = 256;
const VISUAL_PRODUCT_PREVIEW_FRAME: UiProductPreviewFrame = UiProductPreviewFrame::VISUAL_DEFAULT;

pub struct ProjectSync {
    view: ProjectView,
    phase: ProjectSyncPhase,
    shape_cursor: Option<SlotShapeId>,
    shape_page_count: u32,
    shapes_complete: bool,
    control_product_probes_enabled: bool,
    control_product_probe_unsupported_reason: Option<String>,
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
            control_product_probes_enabled: true,
            control_product_probe_unsupported_reason: None,
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
        products: Vec<UiProductRef>,
    ) -> ProjectReadRequest {
        self.phase = ProjectSyncPhase::SyncingProject;
        project_read_request(None, true, self.product_probe_requests(products))
    }

    pub fn refresh_project_read_request(
        &mut self,
        products: Vec<UiProductRef>,
    ) -> ProjectReadRequest {
        self.begin_refresh();
        let since = (self.view.revision != Revision::default()).then_some(self.view.revision);
        // Runtime state slots carry live values/products, so refresh snapshots
        // include slots even when the tree itself only changes by revision.
        let include_slots = true;
        project_read_request(since, include_slots, self.product_probe_requests(products))
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

    pub fn disable_control_product_probes(&mut self, reason: impl Into<String>) -> bool {
        let reason = reason.into();
        let changed = self.control_product_probes_enabled
            || self.control_product_probe_unsupported_reason.as_ref() != Some(&reason);
        self.control_product_probes_enabled = false;
        self.control_product_probe_unsupported_reason = Some(reason.clone());
        for (product, preview) in &mut self.product_previews {
            if matches!(product, UiProductRef::Control { .. }) {
                *preview = UiProductPreview::Unsupported {
                    reason: reason.clone(),
                };
            }
        }
        changed
    }

    fn product_probe_requests(&mut self, products: Vec<UiProductRef>) -> Vec<ProjectProbeRequest> {
        let mut probes = Vec::new();
        self.requested_product_previews.clear();
        for product in products {
            match product {
                UiProductRef::Visual { .. } => {
                    self.product_previews
                        .entry(product)
                        .or_insert(UiProductPreview::Pending);
                    if let Some(visual) = product.visual_product() {
                        self.requested_product_previews.push(product);
                        probes.push(ProjectProbeRequest::RenderProduct(
                            RenderProductProbeRequest {
                                product: visual,
                                width: VISUAL_PRODUCT_PREVIEW_FRAME.width,
                                height: VISUAL_PRODUCT_PREVIEW_FRAME.height,
                                format: WireTextureFormat::Srgb8,
                            },
                        ));
                    }
                }
                UiProductRef::Control { .. } => {
                    if !self.control_product_probes_enabled {
                        let reason = self
                            .control_product_probe_unsupported_reason
                            .clone()
                            .unwrap_or_else(|| {
                                "control product probes are disabled for this session".to_string()
                            });
                        self.product_previews
                            .insert(product, UiProductPreview::Unsupported { reason });
                        continue;
                    }
                    self.product_previews
                        .entry(product)
                        .or_insert(UiProductPreview::Pending);
                    if let Some(control) = product.control_product() {
                        self.requested_product_previews.push(product);
                        probes.push(ProjectProbeRequest::ControlProduct(
                            ControlProductProbeRequest {
                                product: control,
                                sample_format: WireChannelSampleFormat::U16,
                                display_layout: self.display_layout_read_for(product),
                            },
                        ));
                    }
                }
            }
        }
        probes
    }

    fn apply_product_probe_results(&mut self, probes: &[ProjectProbeResult]) {
        let requested = core::mem::take(&mut self.requested_product_previews);
        for (index, probe) in probes.iter().enumerate() {
            let fallback_key = requested.get(index).copied();
            if let Some((product, preview)) = self.product_preview_from_probe(probe, fallback_key) {
                self.product_previews.insert(product, preview);
            }
        }
    }

    fn display_layout_read_for(&self, product: UiProductRef) -> ControlDisplayLayoutRead {
        match self
            .product_previews
            .get(&product)
            .and_then(control_preview_display_layout)
            .map(ControlDisplayLayout::revision)
        {
            Some(revision) => ControlDisplayLayoutRead::IfChanged {
                known_revision: Some(revision),
            },
            None => ControlDisplayLayoutRead::Always,
        }
    }

    fn product_preview_from_probe(
        &self,
        probe: &ProjectProbeResult,
        fallback_key: Option<UiProductRef>,
    ) -> Option<(UiProductRef, UiProductPreview)> {
        match probe {
            ProjectProbeResult::ControlProduct(ControlProductProbeResult::Preview {
                product,
                revision,
                extent,
                sample_format: WireChannelSampleFormat::U16,
                sample_layout,
                display_layout,
                bytes,
            }) => {
                let product_ref = UiProductRef::from_control_product(*product);
                let cached = self.product_previews.get(&product_ref);
                let display_layout =
                    display_layout_from_probe_result(display_layout, cached).cloned();
                Some((
                    product_ref,
                    UiProductPreview::ControlNative(UiControlProductPreview {
                        revision: revision.0,
                        extent: *extent,
                        sample_format: UiControlSampleFormat::U16,
                        sample_layout: sample_layout.clone(),
                        display_layout,
                        bytes: bytes.clone(),
                    }),
                ))
            }
            ProjectProbeResult::ControlProduct(ControlProductProbeResult::Preview {
                product,
                sample_format,
                ..
            }) => Some((
                UiProductRef::from_control_product(*product),
                UiProductPreview::Unsupported {
                    reason: format!(
                        "control preview sample format {sample_format:?} is not supported by Studio"
                    ),
                },
            )),
            ProjectProbeResult::ControlProduct(ControlProductProbeResult::Unsupported {
                product,
                reason,
            }) => Some((
                UiProductRef::from_control_product(*product),
                UiProductPreview::Unsupported {
                    reason: reason.clone(),
                },
            )),
            ProjectProbeResult::ControlProduct(ControlProductProbeResult::Error {
                product,
                message,
            }) => Some((
                UiProductRef::from_control_product(*product),
                UiProductPreview::Error {
                    message: message.clone(),
                },
            )),
            _ => product_preview_from_probe(probe, fallback_key),
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
        ProjectProbeResult::ControlProduct(_) => None,
        ProjectProbeResult::ExplainSlot(_) => None,
    }
}

fn control_preview_display_layout(preview: &UiProductPreview) -> Option<&ControlDisplayLayout> {
    match preview {
        UiProductPreview::ControlNative(preview) => preview.display_layout.as_ref(),
        _ => None,
    }
}

fn display_layout_from_probe_result<'a>(
    result: &'a ControlDisplayLayoutProbeResult,
    cached: Option<&'a UiProductPreview>,
) -> Option<&'a ControlDisplayLayout> {
    match result {
        ControlDisplayLayoutProbeResult::Layout(layout) => Some(layout),
        ControlDisplayLayoutProbeResult::Unchanged { revision } => cached
            .and_then(control_preview_display_layout)
            .filter(|layout| layout.revision() == *revision),
        ControlDisplayLayoutProbeResult::Omitted => cached.and_then(control_preview_display_layout),
        ControlDisplayLayoutProbeResult::Unsupported { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_model::{
        ControlDisplayLayout, ControlExtent, ControlLamp2d, ControlLayout2d, ControlProduct,
        ControlSampleEncoding, ControlSampleLayout, ControlSampleSpan, NodeId, VisualProduct,
    };

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
        let product = VisualProduct::new(NodeId::new(7), 2);

        let request =
            sync.refresh_project_read_request(vec![UiProductRef::from_visual_product(product)]);

        assert_eq!(request.probes.len(), 1);
        assert_eq!(
            request.probes[0],
            ProjectProbeRequest::RenderProduct(RenderProductProbeRequest {
                product,
                width: VISUAL_PRODUCT_PREVIEW_FRAME.width,
                height: VISUAL_PRODUCT_PREVIEW_FRAME.height,
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
        let product = VisualProduct::new(NodeId::new(7), 2);
        let _ = sync.refresh_project_read_request(vec![UiProductRef::from_visual_product(product)]);
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

    #[test]
    fn refresh_request_includes_control_product_probes() {
        let mut sync = ProjectSync::new();
        let product = ControlProduct::new(NodeId::new(7), 2, ControlExtent::new(1, 3));
        let product_ref = UiProductRef::from_control_product(product);

        let request = sync.refresh_project_read_request(vec![product_ref]);

        assert_eq!(
            request.probes,
            vec![ProjectProbeRequest::ControlProduct(
                ControlProductProbeRequest {
                    product,
                    sample_format: WireChannelSampleFormat::U16,
                    display_layout: ControlDisplayLayoutRead::Always,
                },
            )]
        );
        assert_eq!(
            sync.product_preview(&product_ref),
            Some(&UiProductPreview::Pending)
        );
    }

    #[test]
    fn disabled_control_product_probes_preserve_visual_probe_alignment() {
        let mut sync = ProjectSync::new();
        let visual = VisualProduct::new(NodeId::new(7), 1);
        let control = ControlProduct::new(NodeId::new(7), 2, ControlExtent::new(1, 3));
        let visual_ref = UiProductRef::from_visual_product(visual);
        let control_ref = UiProductRef::from_control_product(control);

        assert!(sync.disable_control_product_probes("old firmware"));
        let request = sync.refresh_project_read_request(vec![control_ref, visual_ref]);

        assert_eq!(
            request.probes,
            vec![ProjectProbeRequest::RenderProduct(
                RenderProductProbeRequest {
                    product: visual,
                    width: VISUAL_PRODUCT_PREVIEW_FRAME.width,
                    height: VISUAL_PRODUCT_PREVIEW_FRAME.height,
                    format: WireTextureFormat::Srgb8,
                },
            )]
        );
        assert_eq!(
            sync.product_preview(&control_ref),
            Some(&UiProductPreview::Unsupported {
                reason: "old firmware".to_string(),
            })
        );
        assert_eq!(
            sync.product_preview(&visual_ref),
            Some(&UiProductPreview::Pending)
        );

        let bytes = vec![1, 2, 3];
        sync.apply_project_read_response(ProjectReadResponse {
            revision: Revision::new(9),
            results: Vec::new(),
            probes: vec![ProjectProbeResult::RenderProduct(
                RenderProductProbeResult::Texture {
                    product: visual,
                    revision: Revision::new(9),
                    width: 1,
                    height: 1,
                    format: WireTextureFormat::Srgb8,
                    bytes: bytes.clone(),
                },
            )],
        })
        .unwrap();

        assert_eq!(
            sync.product_preview(&visual_ref),
            Some(&UiProductPreview::VisualSrgb8 {
                width: 1,
                height: 1,
                revision: 9,
                bytes,
            })
        );
    }

    #[test]
    fn control_product_preview_reuses_cached_display_layout_revision() {
        let mut sync = ProjectSync::new();
        let product = ControlProduct::new(NodeId::new(7), 2, ControlExtent::new(1, 3));
        let product_ref = UiProductRef::from_control_product(product);
        let sample_layout = ControlSampleLayout {
            spans: vec![ControlSampleSpan {
                row: 0,
                start: 0,
                len: 3,
                encoding: ControlSampleEncoding::RgbPixels {
                    count: 1,
                    color_order: lpc_model::ColorOrder::Rgb,
                },
            }],
        };
        let display_layout = ControlDisplayLayout::Layout2d(ControlLayout2d::new(
            Revision::new(12),
            16,
            16,
            vec![ControlLamp2d {
                lamp_index: 0,
                sample_start: 0,
                center: [0.5, 0.5],
                radius: 0.1,
            }],
        ));
        let first_bytes = vec![0, 0, 255, 255, 0, 0];
        let _ = sync.refresh_project_read_request(vec![product_ref]);

        sync.apply_project_read_response(ProjectReadResponse {
            revision: Revision::new(9),
            results: Vec::new(),
            probes: vec![ProjectProbeResult::ControlProduct(
                ControlProductProbeResult::Preview {
                    product,
                    revision: Revision::new(9),
                    extent: product.preferred_extent(),
                    sample_format: WireChannelSampleFormat::U16,
                    sample_layout: sample_layout.clone(),
                    display_layout: ControlDisplayLayoutProbeResult::Layout(display_layout.clone()),
                    bytes: first_bytes,
                },
            )],
        })
        .unwrap();

        let request = sync.refresh_project_read_request(vec![product_ref]);

        assert_eq!(
            request.probes,
            vec![ProjectProbeRequest::ControlProduct(
                ControlProductProbeRequest {
                    product,
                    sample_format: WireChannelSampleFormat::U16,
                    display_layout: ControlDisplayLayoutRead::IfChanged {
                        known_revision: Some(Revision::new(12)),
                    },
                },
            )]
        );

        let second_bytes = vec![255, 255, 0, 0, 0, 0];
        sync.apply_project_read_response(ProjectReadResponse {
            revision: Revision::new(10),
            results: Vec::new(),
            probes: vec![ProjectProbeResult::ControlProduct(
                ControlProductProbeResult::Preview {
                    product,
                    revision: Revision::new(10),
                    extent: product.preferred_extent(),
                    sample_format: WireChannelSampleFormat::U16,
                    sample_layout: sample_layout.clone(),
                    display_layout: ControlDisplayLayoutProbeResult::Unchanged {
                        revision: Revision::new(12),
                    },
                    bytes: second_bytes.clone(),
                },
            )],
        })
        .unwrap();

        assert_eq!(
            sync.product_preview(&product_ref),
            Some(&UiProductPreview::ControlNative(UiControlProductPreview {
                revision: 10,
                extent: product.preferred_extent(),
                sample_format: UiControlSampleFormat::U16,
                sample_layout,
                display_layout: Some(display_layout),
                bytes: second_bytes,
            }))
        );
    }
}
