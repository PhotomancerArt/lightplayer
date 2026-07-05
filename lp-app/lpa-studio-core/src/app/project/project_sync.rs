use std::collections::BTreeMap;
use std::rc::Rc;

use lpc_model::{
    ArtifactLocation, ArtifactOverlay, ControlDisplayLayout, MutationCmd, ProjectOverlay, Revision,
    SlotEditOp, SlotPath,
};
use lpc_view::{ApplyStatus, ProjectReadApplier, ProjectView};
use lpc_wire::{
    ControlDisplayLayoutProbeResult, ControlDisplayLayoutRead, ControlProductProbeRequest,
    ControlProductProbeResult, NodeReadQuery, NodeReadSelection, ProjectProbeRequest,
    ProjectProbeResult, ProjectReadEvent, ProjectReadQuery, ProjectReadRequest, ReadLevel,
    RenderProductProbeRequest, RenderProductProbeResult, ResourcePayloadRead, ResourceReadQuery,
    RuntimeReadQuery, ShapeReadQuery, WireChannelSampleFormat, WireTextureFormat,
};

use crate::{
    ProjectRuntimeSummary, ProjectSyncPhase, ProjectSyncSummary, UiControlProductPreview,
    UiControlSampleFormat, UiError, UiIssue, UiProductPreview, UiProductPreviewFrame, UiProductRef,
};

const VISUAL_PRODUCT_PREVIEW_FRAME: UiProductPreviewFrame = UiProductPreviewFrame::VISUAL_DEFAULT;

pub struct ProjectSync {
    view: ProjectView,
    phase: ProjectSyncPhase,
    product_previews: BTreeMap<UiProductRef, UiProductPreview>,
    issue: Option<UiIssue>,
    /// Client mirror of the server's pending-edit overlay.
    overlay: ProjectOverlay,
    /// Revision at which `overlay` was last known to match the server
    /// (`Revision` zero means "never mirrored / no overlay mutation ever").
    overlay_revision: Revision,
}

impl ProjectSync {
    pub fn new() -> Self {
        Self {
            view: ProjectView::new(),
            phase: ProjectSyncPhase::Empty,
            product_previews: BTreeMap::new(),
            issue: None,
            overlay: ProjectOverlay::new(),
            overlay_revision: Revision::default(),
        }
    }

    pub fn begin_initial_sync(&mut self) {
        *self = Self {
            phase: ProjectSyncPhase::SyncingProject,
            ..Self::new()
        };
    }

    pub fn begin_refresh(&mut self) {
        self.phase = ProjectSyncPhase::SyncingProject;
        self.issue = None;
    }

    /// Roll a [`Self::begin_refresh`] back to `Ready` when a gated refresh ends
    /// without applying (cancelled or timed out). The mirror is untouched, so
    /// the prior revision is still valid and the summary should reflect it
    /// rather than lingering in `SyncingProject`.
    pub fn abort_refresh(&mut self) {
        if self.phase == ProjectSyncPhase::SyncingProject {
            self.phase = ProjectSyncPhase::Ready;
        }
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
            shapes_complete: self.phase == ProjectSyncPhase::Ready,
            runtime: self.view.runtime.as_ref().map(ProjectRuntimeSummary::from),
            issue: self.issue.clone(),
        }
    }

    /// Latest protocol/client project mirror.
    pub fn project_view(&self) -> &ProjectView {
        &self.view
    }

    /// Latest mirror of the server's pending-edit overlay.
    pub fn overlay(&self) -> &ProjectOverlay {
        &self.overlay
    }

    /// Revision at which the overlay mirror was last stamped from a server
    /// response (zero: never mirrored).
    pub fn overlay_revision(&self) -> Revision {
        self.overlay_revision
    }

    /// Mirrored pending slot edit for `path` in `artifact`, if any.
    pub fn overlay_edit_at(
        &self,
        artifact: &ArtifactLocation,
        path: &SlotPath,
    ) -> Option<&SlotEditOp> {
        match self.overlay.artifact(artifact)? {
            ArtifactOverlay::Slot { overlay } => overlay.edits.get(path),
            ArtifactOverlay::Asset { .. } => None,
        }
    }

    /// Iterate every mirrored pending slot edit as `(artifact, path, op)`.
    pub fn overlay_slot_edits(
        &self,
    ) -> impl Iterator<Item = (&ArtifactLocation, &SlotPath, &SlotEditOp)> + '_ {
        self.overlay
            .iter()
            .filter_map(|(artifact, overlay)| overlay.as_slot().map(|slot| (artifact, slot)))
            .flat_map(|(artifact, slot)| {
                slot.edits
                    .iter()
                    .map(move |(path, op)| (artifact, path, op))
            })
    }

    /// True when the last applied runtime status reports an overlay revision
    /// the mirror has not caught up with, i.e. a ride-along overlay fetch is
    /// due. A quiet-but-dirty project (revision unchanged since the mirror was
    /// stamped) reports `false`, so no overlay read is issued for it.
    pub fn overlay_fetch_needed(&self) -> bool {
        self.view
            .runtime
            .as_ref()
            .is_some_and(|runtime| runtime.project.overlay_changed_at != self.overlay_revision)
    }

    /// Replace the overlay mirror with a freshly fetched server overlay and
    /// stamp the revision the server reported for it.
    pub fn apply_overlay_read(&mut self, overlay: ProjectOverlay, revision: Revision) {
        self.overlay = overlay;
        self.overlay_revision = revision;
    }

    /// Apply the client's own **accepted** mutation commands to the mirror
    /// when their batch acks, stamping the post-mutation revision from the
    /// mutation response. Per the editing model there is no follow-up fetch
    /// for the client's own edits: the ack is authoritative for them.
    ///
    /// If a foreign client mutated concurrently, the acked revision may skip
    /// states the mirror never saw; that is fine — the next pull's
    /// `overlay_changed_at` comparison self-corrects with a full fetch once
    /// the revision moves past our stamp.
    pub fn apply_acked_edits(&mut self, batch: &[MutationCmd], overlay_revision: Revision) {
        for command in batch {
            self.overlay.apply_mutation(command.mutation.clone());
        }
        self.overlay_revision = overlay_revision;
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
        self.phase == ProjectSyncPhase::SyncingProject
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

    pub fn apply_project_read_events(
        &mut self,
        events: Vec<ProjectReadEvent>,
    ) -> Result<(), UiError> {
        // Probes are read-time diagnostics and are not retained on the view;
        // the applier collects them — reassembling chunked results — and
        // exposes them once the stream completes.
        let probes = {
            let mut applier = ProjectReadApplier::new(&mut self.view);
            let mut probes = Vec::new();
            for event in events {
                if let ApplyStatus::Complete { .. } = applier
                    .apply(event)
                    .map_err(|error| UiError::Protocol(error.to_string()))?
                {
                    probes = applier.take_completed_probe_results();
                    break;
                }
            }
            probes
        };
        let probe_refs: Vec<&ProjectProbeResult> = probes.iter().collect();
        self.apply_product_probe_results(&probe_refs);
        self.phase = ProjectSyncPhase::Ready;
        self.issue = None;
        Ok(())
    }

    pub fn fail(&mut self, issue: impl Into<String>) {
        self.phase = ProjectSyncPhase::Failed;
        self.issue = Some(UiIssue::new(issue));
    }

    /// Drop the accumulated mirror so the next request re-reads from
    /// `since = 0`.
    ///
    /// Gated refreshes assume the local mirror is a faithful prefix of the
    /// server's revision history. If the applier rejects a stream as
    /// malformed, that assumption is broken and further deltas cannot be
    /// trusted, so we discard the mirror (resetting `view.revision` to `0`)
    /// and let the caller resync with a full read. Product-preview caches are
    /// cleared with it, since they are keyed off the same read cycle.
    pub fn reset_view(&mut self) {
        self.view = ProjectView::new();
        self.product_previews.clear();
    }

    fn product_probe_requests(&mut self, products: Vec<UiProductRef>) -> Vec<ProjectProbeRequest> {
        let mut probes = Vec::new();
        for product in products {
            match product {
                UiProductRef::Visual { .. } => {
                    self.product_previews
                        .entry(product)
                        .or_insert(UiProductPreview::Pending);
                    if let Some(visual) = product.visual_product() {
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
                    self.product_previews
                        .entry(product)
                        .or_insert(UiProductPreview::Pending);
                    if let Some(control) = product.control_product() {
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

    fn apply_product_probe_results(&mut self, probes: &[&ProjectProbeResult]) {
        for probe in probes {
            if let Some((product, preview)) = self.product_preview_from_probe(probe) {
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
                        bytes: Rc::from(bytes.as_slice()),
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
            _ => product_preview_from_probe(probe),
        }
    }
}

impl Default for ProjectSync {
    fn default() -> Self {
        Self::new()
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
            ProjectReadQuery::Shapes(ShapeReadQuery {
                level: ReadLevel::Detail,
            }),
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
                bytes: Rc::from(bytes.as_slice()),
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
        ProjectProbeResult::RenderProduct(RenderProductProbeResult::Unsupported {
            product,
            reason,
        }) => Some((
            UiProductRef::from_visual_product(*product),
            UiProductPreview::Unsupported {
                reason: reason.clone(),
            },
        )),
        ProjectProbeResult::RenderProduct(RenderProductProbeResult::Error { product, message }) => {
            Some((
                UiProductRef::from_visual_product(*product),
                UiProductPreview::Error {
                    message: message.clone(),
                },
            ))
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
    use lpc_wire::ProjectReadProbeEvent;

    /// Build a probe-only project-read event stream at `revision`.
    fn probe_events(revision: i64, probes: Vec<ProjectProbeResult>) -> Vec<ProjectReadEvent> {
        let mut events = Vec::with_capacity(probes.len() + 2);
        events.push(ProjectReadEvent::Begin {
            revision: Revision::new(revision),
        });
        for (index, probe) in probes.into_iter().enumerate() {
            events.push(ProjectReadEvent::Probe {
                index: index as u32,
                event: ProjectReadProbeEvent::Result(probe),
            });
        }
        events.push(ProjectReadEvent::End {
            revision: Revision::new(revision),
        });
        events
    }

    #[test]
    fn project_read_request_includes_shapes_nodes_resources_and_runtime() {
        let request = project_read_request(Some(Revision::new(12)), true, Vec::new());

        assert_eq!(request.since, Some(Revision::new(12)));
        assert_eq!(request.queries.len(), 4);
        assert_eq!(
            request.queries[0],
            ProjectReadQuery::Shapes(ShapeReadQuery {
                level: ReadLevel::Detail,
            })
        );
        assert_eq!(
            request.queries[1],
            ProjectReadQuery::Nodes(NodeReadQuery {
                level: ReadLevel::Detail,
                nodes: NodeReadSelection::All,
                include_slots: true,
            })
        );
        assert_eq!(
            request.queries[2],
            ProjectReadQuery::Resources(ResourceReadQuery {
                level: ReadLevel::Summary,
                payloads: ResourcePayloadRead::None,
            })
        );
        assert_eq!(
            request.queries[3],
            ProjectReadQuery::Runtime(RuntimeReadQuery)
        );
        assert!(request.probes.is_empty());
    }

    #[test]
    fn initial_request_reads_from_scratch() {
        // A fresh/reconnected session has no mirror to trust, so the initial
        // read is a full snapshot (`since == None`), the un-gated bulk sync.
        let mut sync = ProjectSync::new();
        sync.view.revision = Revision::new(9);

        let request = sync.initial_project_read_request(Vec::new());

        assert_eq!(request.since, None);
    }

    #[test]
    fn reset_view_forces_full_resync() {
        // The applier-error recovery path resets the mirror; the next request
        // must therefore fall back to a full (`since == None`) read even though
        // a stale revision was previously applied.
        let mut sync = ProjectSync::new();
        sync.view.revision = Revision::new(9);
        sync.view.slots.roots.insert(
            "node.1.state".to_string(),
            lpc_model::SlotData::Unit {
                revision: Revision::new(9),
            },
        );

        sync.reset_view();

        assert_eq!(sync.view.revision, Revision::default());
        assert!(sync.view.slots.roots.is_empty());
        let request = sync.refresh_project_read_request(Vec::new());
        assert_eq!(request.since, None, "reset mirror re-reads from since=0");
    }

    #[test]
    fn malformed_stream_surfaces_protocol_error() {
        // A stream whose End revision disagrees with its Begin is a protocol
        // violation. Surfacing it as `UiError::Protocol` is what triggers the
        // controller's reset-and-resync-from-since=0 recovery.
        let mut sync = ProjectSync::new();
        let result = sync.apply_project_read_events(vec![
            ProjectReadEvent::Begin {
                revision: Revision::new(4),
            },
            ProjectReadEvent::End {
                revision: Revision::new(5),
            },
        ]);

        assert!(matches!(result, Err(UiError::Protocol(_))));
    }

    #[test]
    fn refresh_request_includes_slots_when_roots_are_missing() {
        let mut sync = ProjectSync::new();
        sync.view.revision = Revision::new(9);

        let request = sync.refresh_project_read_request(Vec::new());

        assert_eq!(request.since, Some(Revision::new(9)));
        assert_eq!(
            request.queries[1],
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
            request.queries[1],
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

        sync.apply_project_read_events(probe_events(
            9,
            vec![ProjectProbeResult::RenderProduct(
                RenderProductProbeResult::Texture {
                    product,
                    revision: Revision::new(8),
                    width: 1,
                    height: 2,
                    format: WireTextureFormat::Srgb8,
                    bytes: bytes.clone(),
                },
            )],
        ))
        .unwrap();

        assert_eq!(
            sync.product_preview(&UiProductRef::from_visual_product(product)),
            Some(&UiProductPreview::VisualSrgb8 {
                width: 1,
                height: 2,
                revision: 8,
                bytes: Rc::from(bytes.as_slice()),
            })
        );
    }

    #[test]
    fn unsupported_and_error_probe_results_attribute_by_identity_out_of_order() {
        let mut sync = ProjectSync::new();
        let first = VisualProduct::new(NodeId::new(1), 0);
        let second = VisualProduct::new(NodeId::new(2), 0);
        let first_ref = UiProductRef::from_visual_product(first);
        let second_ref = UiProductRef::from_visual_product(second);

        let _ = sync.refresh_project_read_request(vec![first_ref, second_ref]);

        // Results arrive in the reverse order of the requests: the probe for the
        // second product comes first. Identity on each result must drive
        // attribution — positional matching would have mislabeled these.
        sync.apply_project_read_events(probe_events(
            9,
            vec![
                ProjectProbeResult::RenderProduct(RenderProductProbeResult::Error {
                    product: second,
                    message: "second failed".to_string(),
                }),
                ProjectProbeResult::RenderProduct(RenderProductProbeResult::Unsupported {
                    product: first,
                    reason: "first unsupported".to_string(),
                }),
            ],
        ))
        .unwrap();

        assert_eq!(
            sync.product_preview(&first_ref),
            Some(&UiProductPreview::Unsupported {
                reason: "first unsupported".to_string(),
            })
        );
        assert_eq!(
            sync.product_preview(&second_ref),
            Some(&UiProductPreview::Error {
                message: "second failed".to_string(),
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

        sync.apply_project_read_events(probe_events(
            9,
            vec![ProjectProbeResult::ControlProduct(
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
        ))
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
        sync.apply_project_read_events(probe_events(
            10,
            vec![ProjectProbeResult::ControlProduct(
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
        ))
        .unwrap();

        assert_eq!(
            sync.product_preview(&product_ref),
            Some(&UiProductPreview::ControlNative(UiControlProductPreview {
                revision: 10,
                extent: product.preferred_extent(),
                sample_format: UiControlSampleFormat::U16,
                sample_layout,
                display_layout: Some(display_layout),
                bytes: Rc::from(second_bytes.as_slice()),
            }))
        );
    }

    fn overlay_test_artifact() -> lpc_model::ArtifactLocation {
        lpc_model::ArtifactLocation::file("/orbit.shader.toml")
    }

    fn overlay_test_path() -> SlotPath {
        SlotPath::parse("controls.rate").unwrap()
    }

    fn put_cmd(id: u64, value: f32) -> MutationCmd {
        MutationCmd {
            id: lpc_model::MutationCmdId::new(id),
            mutation: lpc_model::MutationOp::PutSlotEdit {
                artifact: overlay_test_artifact(),
                edit: lpc_model::SlotEdit::assign_value(
                    overlay_test_path(),
                    lpc_model::LpValue::F32(value),
                ),
            },
        }
    }

    fn remove_cmd(id: u64) -> MutationCmd {
        MutationCmd {
            id: lpc_model::MutationCmdId::new(id),
            mutation: lpc_model::MutationOp::RemoveSlotEdit {
                artifact: overlay_test_artifact(),
                path: overlay_test_path(),
            },
        }
    }

    /// Install a runtime status reporting `overlay_changed_at` on the view, as
    /// an applied project read would.
    fn set_runtime_overlay_changed_at(sync: &mut ProjectSync, changed_at: i64) {
        sync.view.runtime = Some(lpc_wire::RuntimeReadResult {
            project: lpc_wire::ProjectRuntimeStatus {
                revision: Revision::new(1),
                overlay_changed_at: Revision::new(changed_at),
                frame_num: 1,
                frame_delta_ms: 16,
                frame_total_ms: 16,
                demand_root_count: 0,
                runtime_buffer_count: 0,
            },
            server: None,
        });
    }

    #[test]
    fn overlay_mirror_starts_empty_at_revision_zero() {
        let sync = ProjectSync::new();

        assert!(sync.overlay().is_empty());
        assert_eq!(sync.overlay_revision(), Revision::default());
        assert_eq!(sync.overlay_slot_edits().count(), 0);
        assert!(
            !sync.overlay_fetch_needed(),
            "no runtime status yet, so no fetch is due"
        );
    }

    #[test]
    fn overlay_fetch_needed_only_when_runtime_revision_advances() {
        let mut sync = ProjectSync::new();

        // Zero changed_at (no overlay mutation ever) matches the fresh mirror.
        set_runtime_overlay_changed_at(&mut sync, 0);
        assert!(!sync.overlay_fetch_needed());

        // The server's overlay changed past the mirror: a fetch is due.
        set_runtime_overlay_changed_at(&mut sync, 4);
        assert!(sync.overlay_fetch_needed());

        // Fetch applied at the reported revision: quiet-but-dirty from here on.
        let mut overlay = ProjectOverlay::new();
        overlay.put_slot_edit(
            overlay_test_artifact(),
            lpc_model::SlotEdit::assign_value(overlay_test_path(), lpc_model::LpValue::F32(1.0)),
        );
        sync.apply_overlay_read(overlay, Revision::new(4));
        assert!(!sync.overlay_fetch_needed());
        assert_eq!(sync.overlay_slot_edits().count(), 1, "dirty but quiet");
    }

    #[test]
    fn apply_overlay_read_replaces_mirror_and_revision() {
        let mut sync = ProjectSync::new();
        sync.apply_acked_edits(&[put_cmd(1, 1.0)], Revision::new(2));

        let mut replacement = ProjectOverlay::new();
        let other_path = SlotPath::parse("controls.hue").unwrap();
        replacement.put_slot_edit(
            overlay_test_artifact(),
            lpc_model::SlotEdit::ensure_present(other_path.clone()),
        );
        sync.apply_overlay_read(replacement, Revision::new(7));

        assert_eq!(sync.overlay_revision(), Revision::new(7));
        assert_eq!(
            sync.overlay_edit_at(&overlay_test_artifact(), &overlay_test_path()),
            None,
            "replacement is wholesale, not merged"
        );
        assert_eq!(
            sync.overlay_edit_at(&overlay_test_artifact(), &other_path),
            Some(&SlotEditOp::EnsurePresent)
        );
    }

    #[test]
    fn acked_put_and_remove_round_trip_through_mirror() {
        let mut sync = ProjectSync::new();

        sync.apply_acked_edits(&[put_cmd(1, 2.5)], Revision::new(3));

        assert_eq!(
            sync.overlay_edit_at(&overlay_test_artifact(), &overlay_test_path()),
            Some(&SlotEditOp::AssignValue(lpc_model::LpValue::F32(2.5)))
        );
        assert_eq!(sync.overlay_revision(), Revision::new(3));

        sync.apply_acked_edits(&[remove_cmd(2)], Revision::new(4));

        assert_eq!(
            sync.overlay_edit_at(&overlay_test_artifact(), &overlay_test_path()),
            None
        );
        assert!(sync.overlay().is_empty());
        assert_eq!(sync.overlay_revision(), Revision::new(4));
    }

    #[test]
    fn acked_edits_at_reported_revision_need_no_fetch() {
        // Plan Q2: the client's own acked edits are applied locally and never
        // trigger a follow-up overlay read; the ack revision keeps the mirror
        // aligned with what the next pull's runtime status reports.
        let mut sync = ProjectSync::new();

        sync.apply_acked_edits(&[put_cmd(1, 1.0)], Revision::new(5));
        set_runtime_overlay_changed_at(&mut sync, 5);

        assert!(!sync.overlay_fetch_needed());
        assert_eq!(sync.overlay_slot_edits().count(), 1);
    }
}
