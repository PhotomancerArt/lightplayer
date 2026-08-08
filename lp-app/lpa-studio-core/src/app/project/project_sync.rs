use std::collections::BTreeMap;
use std::rc::Rc;

use lpc_model::{
    ArtifactLocation, ArtifactOverlay, AssetBodyOverlay, ControlDisplayLayout, MutationCmd,
    MutationEffect, MutationOp, ProjectOverlay, Revision, SlotEditOp, SlotPath, StoredSlotEdit,
};
use lpc_view::{ApplyStatus, ProjectReadApplier, ProjectView};
use lpc_wire::{
    BindingGraphProbeRequest, BindingGraphProbeResult, ControlDisplayLayoutProbeResult,
    ControlDisplayLayoutRead, ControlProductProbeRequest, ControlProductProbeResult, NodeReadQuery,
    NodeReadSelection, ProjectProbeRequest, ProjectProbeResult, ProjectReadEvent, ProjectReadQuery,
    ProjectReadRequest, ReadLevel, RenderProductProbeRequest, RenderProductProbeResult,
    ResourcePayloadRead, ResourceReadQuery, RuntimeReadQuery, ShapeReadQuery, TimebaseProbeRequest,
    TimebaseProbeResult, WireBindingGraph, WireCellProjection, WireChannelSampleFormat,
    WireConsumerPolicy, WireProjectionOrigin, WireTextureFormat, WireVisualSpace,
};

use crate::{
    ProjectRuntimeSummary, ProjectSyncPhase, ProjectSyncSummary, UiCellProjection,
    UiConsumerPolicy, UiControlProductPreview, UiControlSampleFormat, UiError, UiIssue,
    UiPreviewSpaces, UiProductPreview, UiProductPreviewFrame, UiProductRef, UiProductSpaceView,
    UiProjectionOrigin, UiVisualProductSpace, UiVisualSpace,
};

pub struct ProjectSync {
    view: ProjectView,
    phase: ProjectSyncPhase,
    /// The resolution visual-product probes request. Runtime-tiered policy
    /// pushed down by the project controller before each request build
    /// (sim 32×32, device 16×16 — probe-performance plan); the engine
    /// renders natively at whatever is asked.
    visual_preview_frame: UiProductPreviewFrame,
    product_previews: BTreeMap<UiProductRef, UiProductPreview>,
    /// Latest timebase listing per subscribed time product (D10). Keyed like
    /// `product_previews` and filled by the same probe cycle: a clock's card
    /// asks for its listing exactly while it is subscribed for products, so
    /// the debug surface costs nothing on an unfocused clock.
    timebases: BTreeMap<UiProductRef, UiTimebaseRead>,
    /// Latest space metadata a render-product probe answered, per
    /// (visual product, space) (plan-B P2, re-keyed in P3 for the D15
    /// checkboxes). Rides beside the previews rather than inside them, so
    /// the metadata is available without widening the `VisualSrgb8` DTO
    /// every hand-built preview fixture constructs.
    product_spaces: BTreeMap<(UiProductRef, UiVisualSpace), UiVisualProductSpace>,
    /// Per-space preview bytes for visual products (P3). A card previewing
    /// one space has one entry here and it also mirrors into
    /// `product_previews`; a card previewing BOTH has two, and only the
    /// hero mirrors. Keeping `product_previews` as the hero's home is what
    /// lets every space-unaware surface — module heroes, playlist thumbs,
    /// the GPU gallery — stay exactly as it was.
    product_space_previews: BTreeMap<(UiProductRef, UiVisualSpace), UiProductPreview>,
    /// What each visual product's card asks its previews to be (D15's
    /// checkbox state, resolved). Pushed down by the project controller
    /// before each request build, exactly like `visual_preview_frame`;
    /// products with no entry keep today's single 2D probe.
    preview_spaces: BTreeMap<UiProductRef, UiProductSpaceRequest>,
    /// Latest binding-graph snapshot, kept while a consumer subscribes.
    binding_graph: Option<WireBindingGraph>,
    /// Whether reads should carry the binding-graph probe. Armed for every
    /// ready project: module faces derive from it.
    binding_graph_subscribed: bool,
    issue: Option<UiIssue>,
    /// Client mirror of the server's pending-edit overlay.
    overlay: ProjectOverlay,
    /// Base (saved) value display strings for the overlay's pending slot-edit
    /// paths — a parallel map beside the overlay, keyed exactly like its
    /// entries. Populated wholesale by [`Self::apply_overlay_read`] and per
    /// effect annotation by [`Self::apply_acked_edits`]; entries drop when
    /// their overlay entries drop, so the map never outlives the edits it
    /// describes. Paths without an entry render as "not set".
    base_values: BTreeMap<(ArtifactLocation, SlotPath), String>,
    /// Revision at which `overlay` was last known to match the server
    /// (`Revision` zero means "never mirrored / no overlay mutation ever").
    overlay_revision: Revision,
}

impl ProjectSync {
    pub fn new() -> Self {
        Self {
            view: ProjectView::new(),
            phase: ProjectSyncPhase::Empty,
            visual_preview_frame: UiProductPreviewFrame::VISUAL_DEFAULT,
            product_previews: BTreeMap::new(),
            timebases: BTreeMap::new(),
            product_spaces: BTreeMap::new(),
            product_space_previews: BTreeMap::new(),
            preview_spaces: BTreeMap::new(),
            binding_graph: None,
            binding_graph_subscribed: false,
            issue: None,
            overlay: ProjectOverlay::new(),
            base_values: BTreeMap::new(),
            overlay_revision: Revision::default(),
        }
    }

    pub fn begin_initial_sync(&mut self) {
        // Reset per-read data (mirror, previews, overlay) but preserve the
        // binding-graph subscription: it is a property of "a project is
        // connected" (set at `mark_ready`), not per-read state. Without
        // this, the reset here wipes the subscription that `mark_ready`
        // just set and the graph never arrives — which since P3 means
        // every module card loses its face, not just a pane going quiet.
        let binding_graph_subscribed = self.binding_graph_subscribed;
        *self = Self {
            phase: ProjectSyncPhase::SyncingProject,
            binding_graph_subscribed,
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
            overlay_revision: self.overlay_revision.0,
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
        overlay_edit_at(&self.overlay, artifact, path)
    }

    /// Base (saved) value display string for the pending edit at `path` in
    /// `artifact`, if the server derived one (`None`: no pending edit there,
    /// or the target is absent in the base — render "not set").
    pub fn base_value_at(&self, artifact: &ArtifactLocation, path: &SlotPath) -> Option<&str> {
        self.base_values
            .get(&(artifact.clone(), path.clone()))
            .map(String::as_str)
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

    /// Iterate every mirrored pending asset body edit as `(artifact, body)`
    /// — the `ArtifactOverlay::Asset` complement of
    /// [`Self::overlay_slot_edits`].
    pub fn overlay_asset_edits(
        &self,
    ) -> impl Iterator<Item = (&ArtifactLocation, &AssetBodyOverlay)> + '_ {
        self.overlay
            .iter()
            .filter_map(|(artifact, overlay)| overlay.as_body().map(|body| (artifact, body)))
    }

    /// Mirrored pending asset body edit for `artifact`, if any.
    pub fn overlay_asset_edit_at(&self, artifact: &ArtifactLocation) -> Option<&AssetBodyOverlay> {
        self.overlay.artifact(artifact)?.as_body()
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

    /// Replace the overlay mirror — and its parallel base-value map — with a
    /// freshly fetched server overlay and stamp the revision the server
    /// reported for it. The replacement is wholesale on both structures: a
    /// path the response does not annotate has no base value (absent in
    /// base), so stale map entries must not survive the read.
    pub fn apply_overlay_read(
        &mut self,
        overlay: ProjectOverlay,
        base_values: Vec<(ArtifactLocation, SlotPath, String)>,
        revision: Revision,
    ) {
        self.overlay = overlay;
        self.base_values = base_values
            .into_iter()
            .map(|(artifact, path, display)| ((artifact, path), display))
            .collect();
        self.overlay_revision = revision;
    }

    /// Apply the client's own **accepted** mutation commands to the mirror
    /// when their batch acks, stamping the post-mutation revision from the
    /// mutation response. Per the editing model there is no follow-up fetch
    /// for the client's own edits: the ack is authoritative for them.
    ///
    /// What is applied is each command's server-reported **effect**, not the
    /// command as sent: the server normalizes a `PutSlotEdit` assigning the
    /// base value into a removal (`MutationEffect::NormalizedToRemoval`), and
    /// it materializes a `MoveSlotEntry` into several per-path stored edits
    /// (`MutationEffect::Materialized`), which the mirror replays verbatim.
    /// Since a no-op normalization does not bump the overlay revision, a
    /// mirror that applied the sent Put would read dirty forever with no
    /// corrective fetch ever due.
    ///
    /// If a foreign client mutated concurrently, the acked revision may skip
    /// states the mirror never saw; that is fine — the next pull's
    /// `overlay_changed_at` comparison self-corrects with a full fetch once
    /// the revision moves past our stamp.
    /// Base-value annotations ride the same effects: a stored `Put` carrying
    /// `base_display` installs (or, when `None`, clears) the parallel map
    /// entry at its path, and after the replay the map is pruned to paths the
    /// overlay still holds — removals and normalizations (including
    /// canonicalization side effects like descendants cleared under a stored
    /// `Remove`) drop their base values with their entries.
    pub fn apply_acked_edits(
        &mut self,
        batch: &[(MutationCmd, MutationEffect)],
        overlay_revision: Revision,
    ) {
        for (command, effect) in batch {
            for mutation in effective_mutations(command, effect) {
                self.overlay.apply_mutation(mutation);
            }
            for (artifact, path, base_display) in effect_base_annotations(command, effect) {
                match base_display {
                    Some(display) => {
                        self.base_values.insert((artifact, path), display);
                    }
                    None => {
                        self.base_values.remove(&(artifact, path));
                    }
                }
            }
        }
        self.prune_base_values();
        self.overlay_revision = overlay_revision;
    }

    /// Drop base-value entries whose overlay entries no longer exist, so the
    /// parallel map tracks the overlay exactly even through canonicalization
    /// side effects the ack does not spell out per path.
    fn prune_base_values(&mut self) {
        let overlay = &self.overlay;
        self.base_values
            .retain(|(artifact, path), _| overlay_edit_at(overlay, artifact, path).is_some());
    }

    /// Latest cached preview state for a produced product.
    pub fn product_preview(&self, product: &UiProductRef) -> Option<&UiProductPreview> {
        self.product_previews.get(product)
    }

    /// Latest cached timebase listing for a time product (D10). `None` means
    /// no read has landed yet — distinct from a read that came back
    /// [`UiTimebaseRead::Unknown`].
    pub fn timebase(&self, product: &UiProductRef) -> Option<&UiTimebaseRead> {
        self.timebases.get(product)
    }

    /// Latest cached space metadata for a visual product's HERO probe —
    /// the resolved space/projection/origin and the producer's own primary
    /// space. `None` means no probe result with space metadata has landed
    /// yet (e.g. still `Pending`, or the last result was
    /// `Unsupported`/`Error`).
    ///
    /// The hero is the space the card leads with (its primary, until the
    /// checkboxes say otherwise); ask [`Self::product_space_in`] for a
    /// specific one.
    pub fn product_space(&self, product: &UiProductRef) -> Option<&UiVisualProductSpace> {
        self.product_space_in(product, self.hero_space(product))
            .or_else(|| {
                self.product_spaces
                    .range((*product, UiVisualSpace::OneD)..=(*product, UiVisualSpace::TwoD))
                    .next()
                    .map(|(_, space)| space)
            })
    }

    /// Latest cached space metadata for one specific probed space.
    pub fn product_space_in(
        &self,
        product: &UiProductRef,
        space: UiVisualSpace,
    ) -> Option<&UiVisualProductSpace> {
        self.product_spaces.get(&(*product, space))
    }

    /// Latest cached preview for one specific probed space. `None` when
    /// that space has never been asked for.
    pub fn product_preview_in(
        &self,
        product: &UiProductRef,
        space: UiVisualSpace,
    ) -> Option<&UiProductPreview> {
        self.product_space_previews.get(&(*product, space))
    }

    /// The per-space preview views a card's face renders (D15's stacked
    /// previews and their captions), in checked order with the hero
    /// flagged.
    ///
    /// A product no card has registered yet — every product on the FIRST
    /// read, since the card tree does not exist when that request is built
    /// — reports the default single 2D view, which is exactly what was
    /// probed for it. The next read resolves against the producer's real
    /// primary.
    pub fn product_space_views(&self, product: &UiProductRef) -> Vec<UiProductSpaceView> {
        let request = self
            .preview_spaces
            .get(product)
            .copied()
            .unwrap_or_default();
        request
            .spaces
            .checked()
            .map(|space| UiProductSpaceView {
                space,
                preview: self
                    .product_preview_in(product, space)
                    .cloned()
                    .unwrap_or(UiProductPreview::Pending),
                frame: probe_frame(self.visual_preview_frame, space),
                meta: self.product_space_in(product, space).copied(),
                hero: space == request.hero,
            })
            .collect()
    }

    /// The space whose probe feeds this product's product-keyed (hero)
    /// preview. `TwoD` for every product no card registered — today's
    /// behavior, unchanged.
    fn hero_space(&self, product: &UiProductRef) -> UiVisualSpace {
        self.preview_spaces
            .get(product)
            .map_or(UiVisualSpace::TwoD, |request| request.hero)
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
        project_read_request(None, true, self.probe_requests(products))
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
        project_read_request(since, include_slots, self.probe_requests(products))
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
        self.apply_binding_graph_probe_results(&probe_refs);
        self.phase = ProjectSyncPhase::Ready;
        self.issue = None;
        Ok(())
    }

    fn apply_binding_graph_probe_results(&mut self, probes: &[&ProjectProbeResult]) {
        for probe in probes {
            if let ProjectProbeResult::BindingGraph(BindingGraphProbeResult::Graph(graph)) = probe {
                self.binding_graph = Some(graph.clone());
            }
        }
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
        self.timebases.clear();
        self.product_spaces.clear();
        self.product_space_previews.clear();
        self.binding_graph = None;
    }

    /// Latest binding-graph snapshot, when a consumer subscribes.
    pub fn binding_graph(&self) -> Option<&WireBindingGraph> {
        self.binding_graph.as_ref()
    }

    /// Inject a binding-graph snapshot directly (test fixture path).
    #[cfg(test)]
    pub(crate) fn set_binding_graph_for_test(&mut self, graph: WireBindingGraph) {
        self.binding_graph = Some(graph);
    }

    /// Inject a timebase read directly (test fixture path), standing in for
    /// the probe cycle the same way `set_binding_graph_for_test` does.
    #[cfg(test)]
    pub(crate) fn set_timebase_for_test(&mut self, product: UiProductRef, read: UiTimebaseRead) {
        self.timebases.insert(product, read);
    }

    /// Set the resolution visual-product probes request (runtime-tiered:
    /// the project controller pushes the lens tier down before each
    /// request build).
    pub fn set_visual_preview_frame(&mut self, frame: UiProductPreviewFrame) {
        self.visual_preview_frame = frame;
    }

    /// Set which spaces each visual product's previews are probed in (D15
    /// checkbox state, resolved per card). Pushed down by the project
    /// controller before each request build, exactly like
    /// [`Self::set_visual_preview_frame`] — the sync knows products, not
    /// cards.
    ///
    /// Products dropped from the map lose their extra per-space previews
    /// so a card that goes back to one space stops carrying the other's
    /// stale bytes.
    pub fn set_preview_spaces(&mut self, requests: BTreeMap<UiProductRef, UiProductSpaceRequest>) {
        self.product_space_previews.retain(|(product, space), _| {
            requests
                .get(product)
                .is_some_and(|request| request.spaces.is_checked(*space))
        });
        self.product_spaces.retain(|(product, space), _| {
            requests
                .get(product)
                .is_some_and(|request| request.spaces.is_checked(*space))
                // A product no card registered keeps its single 2D entry:
                // that is the metadata every unregistered product has ever
                // had, and dropping it would blind the very first
                // resolution pass (which reads `primary` to decide what to
                // ask for next).
                || (!requests.contains_key(product) && *space == UiVisualSpace::TwoD)
        });
        self.preview_spaces = requests;
    }

    /// Toggle the binding-graph probe on project reads. Unsubscribing drops
    /// the cached snapshot so stale topology never renders — and with it
    /// every module face, which cannot derive without a graph.
    pub fn set_binding_graph_subscribed(&mut self, subscribed: bool) {
        self.binding_graph_subscribed = subscribed;
        if !subscribed {
            self.binding_graph = None;
        }
    }

    fn probe_requests(&mut self, products: Vec<UiProductRef>) -> Vec<ProjectProbeRequest> {
        let mut probes = self.product_probe_requests(products);
        if self.binding_graph_subscribed {
            probes.push(ProjectProbeRequest::BindingGraph(
                BindingGraphProbeRequest {
                    include_values: true,
                },
            ));
        }
        probes
    }

    fn product_probe_requests(&mut self, products: Vec<UiProductRef>) -> Vec<ProjectProbeRequest> {
        let mut probes = Vec::new();
        for product in products {
            match product {
                // ONE PROBE PER CHECKED SPACE (D15). A card previewing
                // both spaces asks the engine twice for the same product —
                // the two renders are genuinely different frames, and the
                // resolved `space` on each result is what keys them apart
                // on the way back. A product no card registered asks once,
                // in 2D, exactly as before.
                UiProductRef::Visual { .. } => {
                    self.product_previews
                        .entry(product)
                        .or_insert(UiProductPreview::Pending);
                    if let Some(visual) = product.visual_product() {
                        let request = self
                            .preview_spaces
                            .get(&product)
                            .copied()
                            .unwrap_or_default();
                        for space in request.spaces.checked() {
                            self.product_space_previews
                                .entry((product, space))
                                .or_insert(UiProductPreview::Pending);
                            probes.push(ProjectProbeRequest::RenderProduct(visual_probe_request(
                                visual,
                                self.visual_preview_frame,
                                space,
                                UiConsumerPolicy::AUTO,
                            )));
                        }
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
                // A time product has no preview frame — there is no picture
                // behind the handle. What it has is the timebase listing, so
                // the same subscription that would fetch a preview fetches
                // the phasor rows instead (D10).
                UiProductRef::Time { .. } => {
                    if let Some(time) = product.time_product() {
                        probes.push(ProjectProbeRequest::Timebase(TimebaseProbeRequest {
                            product: time,
                        }));
                    }
                }
            }
        }
        probes
    }

    fn apply_product_probe_results(&mut self, probes: &[&ProjectProbeResult]) {
        for probe in probes {
            if let Some((product, preview)) = self.product_preview_from_probe(probe) {
                match probe_result_space(probe) {
                    // A space-tagged visual result files per (product,
                    // space); it ALSO becomes the product-keyed preview
                    // when it is the hero, which is what keeps every
                    // space-unaware surface reading the right frame. Two
                    // results for one product can no longer overwrite each
                    // other.
                    Some(space) => {
                        if self.hero_space(&product) == space {
                            self.product_previews.insert(product, preview.clone());
                        }
                        self.product_space_previews
                            .insert((product, space), preview);
                    }
                    // Control/time results, and the visual refusals that
                    // name no space (`Unsupported`/`Error`): product-keyed
                    // as they have always been. A refusal leaves the
                    // per-space entries holding their last good frame
                    // rather than blanking a stack.
                    None => {
                        self.product_previews.insert(product, preview);
                    }
                }
            }
            if let Some((product, read)) = timebase_from_probe(probe) {
                self.timebases.insert(product, read);
            }
            if let Some((product, space, meta)) = product_space_from_probe(probe) {
                self.product_spaces.insert((product, space), meta);
            }
        }
    }

    /// Control products whose latest preview carries no display layout,
    /// paired with that preview's product revision — the candidates for the
    /// client-side fallback
    /// ([`crate::app::project::control_display_layout_fallback`]).
    ///
    /// A layout is missing whenever the engine refused it (over the wire
    /// budget, or a producer that publishes none) and nothing cached stands
    /// in, which is exactly the state
    /// [`display_layout_from_probe_result`]'s `Unsupported` arm leaves
    /// behind.
    pub(in crate::app::project) fn control_products_missing_display_layout(
        &self,
    ) -> Vec<(UiProductRef, Revision)> {
        self.product_previews
            .iter()
            .filter_map(|(product, preview)| match preview {
                UiProductPreview::ControlNative(preview) if preview.display_layout.is_none() => {
                    Some((*product, Revision::new(preview.revision)))
                }
                _ => None,
            })
            .collect()
    }

    /// Install a client-synthesized display layout on a cached control
    /// preview. A no-op when the preview is gone or is not a native control
    /// preview — the fallback never creates preview state of its own.
    pub(in crate::app::project) fn set_control_display_layout(
        &mut self,
        product: &UiProductRef,
        layout: Rc<ControlDisplayLayout>,
    ) {
        if let Some(UiProductPreview::ControlNative(preview)) =
            self.product_previews.get_mut(product)
        {
            preview.display_layout = Some(layout);
        }
    }

    /// What the next probe should ask for: nothing new while the cached
    /// layout's revision still stands, otherwise the whole layout.
    ///
    /// A client-synthesized layout ([`Self::set_control_display_layout`])
    /// rides this the same way an engine-sent one does, carrying the
    /// preview's PRODUCT revision rather than the engine's layout revision —
    /// the refusal never tells us the latter. Both answers the engine can
    /// give back are correct:
    ///
    /// - the revisions differ (the normal case, since the product revision
    ///   advances every tick), so the engine re-evaluates, refuses again,
    ///   and the fallback re-synthesizes from the current document;
    /// - they coincide, so the engine answers `Unchanged` and the cached
    ///   synthesis is kept — which is what a matching layout revision means
    ///   anyway: the geometry has not moved.
    ///
    /// Neither path issues an extra request or loses the layout, so the
    /// fallback cannot churn the read loop.
    fn display_layout_read_for(&self, product: UiProductRef) -> ControlDisplayLayoutRead {
        match self
            .product_previews
            .get(&product)
            .and_then(control_preview_display_layout)
            .map(|layout| layout.revision())
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
                let display_layout = display_layout_from_probe_result(display_layout, cached);
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

/// The mutations the server actually stored for an acked command.
///
/// A `NormalizedToRemoval` effect on a `PutSlotEdit` means the server dropped
/// (or never created) the overlay entry at the edit's path, so the mirror
/// applies the equivalent removal. A `Materialized` effect (a
/// `MoveSlotEntry`'s synthesized per-path edits, a structural `Remove`
/// that normalized away and also cleared the overlay entries stranded under
/// its path, or an enum-variant `EnsurePresent` that also cleared pending
/// entries at sibling variant paths) carries the full ordered list of stored
/// per-path edits, which the mirror replays verbatim against the command's
/// artifact. Every other
/// effect applies the command as sent. Removing an absent entry is a no-op,
/// so the `changed: false` cases need no special handling.
fn effective_mutations(command: &MutationCmd, effect: &MutationEffect) -> Vec<MutationOp> {
    match (effect, &command.mutation) {
        (
            MutationEffect::NormalizedToRemoval { .. },
            MutationOp::PutSlotEdit { artifact, edit },
        ) => vec![MutationOp::RemoveSlotEdit {
            artifact: artifact.clone(),
            path: edit.path.clone(),
        }],
        (
            MutationEffect::Materialized { edits, .. },
            MutationOp::MoveSlotEntry { artifact, .. } | MutationOp::PutSlotEdit { artifact, .. },
        ) => edits
            .iter()
            .map(|stored| match stored {
                StoredSlotEdit::Put { edit, .. } => MutationOp::PutSlotEdit {
                    artifact: artifact.clone(),
                    edit: edit.clone(),
                },
                StoredSlotEdit::Removed { path } => MutationOp::RemoveSlotEdit {
                    artifact: artifact.clone(),
                    path: path.clone(),
                },
            })
            .collect(),
        _ => vec![command.mutation.clone()],
    }
}

/// The base-value map updates an acked command's effect prescribes, as
/// `(artifact, path, annotation)` — one entry per stored `Put`, mirroring
/// [`effective_mutations`]'s replay. `Some` installs the base display at the
/// path, `None` clears any stale entry (the overlay entry exists but its base
/// target does not, so the old value reads "not set"). Effects that only drop
/// overlay entries prescribe nothing here: the post-replay prune removes
/// their map entries alongside.
fn effect_base_annotations(
    command: &MutationCmd,
    effect: &MutationEffect,
) -> Vec<(ArtifactLocation, SlotPath, Option<String>)> {
    match (effect, &command.mutation) {
        (
            MutationEffect::OverlayChanged { base_display, .. },
            MutationOp::PutSlotEdit { artifact, edit },
        ) => vec![(artifact.clone(), edit.path.clone(), base_display.clone())],
        (
            MutationEffect::Materialized { edits, .. },
            MutationOp::MoveSlotEntry { artifact, .. } | MutationOp::PutSlotEdit { artifact, .. },
        ) => edits
            .iter()
            .filter_map(|stored| match stored {
                StoredSlotEdit::Put { edit, base_display } => {
                    Some((artifact.clone(), edit.path.clone(), base_display.clone()))
                }
                StoredSlotEdit::Removed { .. } => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// Pending slot edit at `path` in `artifact` inside `overlay`, if any.
fn overlay_edit_at<'a>(
    overlay: &'a ProjectOverlay,
    artifact: &ArtifactLocation,
    path: &SlotPath,
) -> Option<&'a SlotEditOp> {
    match overlay.artifact(artifact)? {
        ArtifactOverlay::Slot { overlay } => overlay.edits.get(path),
        ArtifactOverlay::Asset { .. } => None,
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
            ..
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
        ProjectProbeResult::RenderProduct(RenderProductProbeResult::GpuResident {
            product,
            ..
        }) => Some((
            UiProductRef::from_visual_product(*product),
            UiProductPreview::Unsupported {
                reason: "product is GPU-resident on this runtime (no byte readback on the GPU \
                         tier); probe a CPU-tier runtime for byte previews"
                    .to_string(),
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
        // Published output frames are the device card's play-tab feed, not a
        // lens product preview: they key on an output NODE, not a
        // `UiProductRef`, and are consumed by their own path.
        ProjectProbeResult::OutputFrame(_) => None,
        ProjectProbeResult::BindingGraph(_) | ProjectProbeResult::Timebase(_) => None,
    }
}

/// One clock's timebase, as the Studio caches it between reads (D10).
///
/// `Unknown` is a real answer, not a missing one: the product names a clock
/// the runtime resolves no timebase for (the node just left the tree, or has
/// never produced). "No read has landed yet" is the *absence* of a
/// [`UiTimebaseRead`], which the clock face renders differently.
#[derive(Clone, Debug, PartialEq)]
pub enum UiTimebaseRead {
    /// The clock published a timebase; `phasors` is what rides it (empty is
    /// a normal state — nothing declared a phasor, or they all went idle).
    Live {
        /// Effective project seconds (rate and scrub applied).
        seconds: f32,
        /// Seconds the timebase advanced on the most recent tick. Negative
        /// while a device scrubs backwards.
        delta_seconds: f32,
        /// Live integrators, in store order.
        phasors: Vec<lpc_wire::WirePhasorRow>,
    },
    /// The named product resolves to no clock in this runtime.
    Unknown,
}

fn timebase_from_probe(probe: &ProjectProbeResult) -> Option<(UiProductRef, UiTimebaseRead)> {
    match probe {
        ProjectProbeResult::Timebase(TimebaseProbeResult::Timebase {
            product,
            seconds,
            delta_seconds,
            phasors,
            ..
        }) => Some((
            UiProductRef::from_time_product(*product),
            UiTimebaseRead::Live {
                seconds: *seconds,
                delta_seconds: *delta_seconds,
                phasors: phasors.clone(),
            },
        )),
        ProjectProbeResult::Timebase(TimebaseProbeResult::Unknown { product }) => Some((
            UiProductRef::from_time_product(*product),
            UiTimebaseRead::Unknown,
        )),
        _ => None,
    }
}

/// What a visual product's card asks its previews to be: which spaces to
/// probe (D15's checkboxes) and which of them feeds the product-keyed hero
/// preview.
///
/// The hero is normally the producer's own primary space — a 1D shader
/// leads with its strip — so the space-unaware surfaces render the
/// producer as it actually is. [`Default`] is today's answer for every
/// product no card registered: 2D, alone.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UiProductSpaceRequest {
    /// The checked spaces (at least one, by [`UiPreviewSpaces`]'s own
    /// invariant).
    pub spaces: UiPreviewSpaces,
    /// The space whose probe becomes the product-keyed preview.
    pub hero: UiVisualSpace,
}

impl UiProductSpaceRequest {
    /// A card previewing one space, leading with it.
    pub fn only(space: UiVisualSpace) -> Self {
        Self {
            spaces: UiPreviewSpaces::only(space),
            hero: space,
        }
    }
}

impl Default for UiProductSpaceRequest {
    fn default() -> Self {
        Self::only(UiVisualSpace::TwoD)
    }
}

/// The probe frame a space asks for: a 1D render is one row of `width`
/// samples, which is exactly what the shader compiler's 1D entry contract
/// requires (`outputSize` reports `(N, 1)`).
fn probe_frame(frame: UiProductPreviewFrame, space: UiVisualSpace) -> UiProductPreviewFrame {
    match space {
        UiVisualSpace::OneD => UiProductPreviewFrame::new(frame.width, 1),
        UiVisualSpace::TwoD => frame,
    }
}

/// Build one space-tagged render-product probe request.
///
/// The single place a visual preview request is composed — the per-card
/// checkbox probes below and P4's forced-policy TILE probes (pass
/// [`UiConsumerPolicy::forcing`]) go through here, so a tile and a preview
/// can never disagree about frame geometry or format.
pub fn visual_probe_request(
    product: lpc_model::VisualProduct,
    frame: UiProductPreviewFrame,
    space: UiVisualSpace,
    policy: UiConsumerPolicy,
) -> RenderProductProbeRequest {
    let frame = probe_frame(frame, space);
    RenderProductProbeRequest {
        product,
        width: frame.width,
        height: frame.height,
        format: WireTextureFormat::Srgb8,
        space: Some(wire_visual_space(space)),
        policy: Some(WireConsumerPolicy {
            default_1d_to_2d: wire_cell_projection(policy.default_1d_to_2d),
            force: policy.force,
        }),
    }
}

/// The space a render-product result rendered in, when it names one.
/// `None` for `Unsupported`/`Error` (and every non-render probe): those
/// answer for the product as a whole, not for one space.
fn probe_result_space(probe: &ProjectProbeResult) -> Option<UiVisualSpace> {
    match probe {
        ProjectProbeResult::RenderProduct(
            RenderProductProbeResult::Texture { space, .. }
            | RenderProductProbeResult::GpuResident { space, .. },
        ) => Some(ui_visual_space(*space)),
        _ => None,
    }
}

/// Space metadata riding a render-product probe result (plan-B P2), keyed
/// by the space it rendered in (P3). `None` for `Unsupported`/`Error`
/// results — those carry no space to cache, and the previous cached answer
/// (if any) is left as the last known one.
fn product_space_from_probe(
    probe: &ProjectProbeResult,
) -> Option<(UiProductRef, UiVisualSpace, UiVisualProductSpace)> {
    match probe {
        ProjectProbeResult::RenderProduct(RenderProductProbeResult::Texture {
            product,
            space,
            projection,
            origin,
            primary,
            ..
        })
        | ProjectProbeResult::RenderProduct(RenderProductProbeResult::GpuResident {
            product,
            space,
            projection,
            origin,
            primary,
            ..
        }) => Some((
            UiProductRef::from_visual_product(*product),
            ui_visual_space(*space),
            UiVisualProductSpace {
                space: ui_visual_space(*space),
                projection: projection.map(ui_cell_projection),
                origin: origin.map(ui_projection_origin),
                primary: ui_visual_space(*primary),
            },
        )),
        _ => None,
    }
}

fn wire_visual_space(space: UiVisualSpace) -> WireVisualSpace {
    match space {
        UiVisualSpace::OneD => WireVisualSpace::OneD,
        UiVisualSpace::TwoD => WireVisualSpace::TwoD,
    }
}

fn wire_cell_projection(cell: UiCellProjection) -> WireCellProjection {
    match cell {
        UiCellProjection::Extrude => WireCellProjection::Extrude,
        UiCellProjection::Radial => WireCellProjection::Radial,
        UiCellProjection::Angular => WireCellProjection::Angular,
        UiCellProjection::Mirror => WireCellProjection::Mirror,
    }
}

fn ui_visual_space(space: WireVisualSpace) -> UiVisualSpace {
    match space {
        WireVisualSpace::OneD => UiVisualSpace::OneD,
        WireVisualSpace::TwoD => UiVisualSpace::TwoD,
    }
}

fn ui_cell_projection(cell: WireCellProjection) -> UiCellProjection {
    match cell {
        WireCellProjection::Extrude => UiCellProjection::Extrude,
        WireCellProjection::Radial => UiCellProjection::Radial,
        WireCellProjection::Angular => UiCellProjection::Angular,
        WireCellProjection::Mirror => UiCellProjection::Mirror,
    }
}

fn ui_projection_origin(origin: WireProjectionOrigin) -> UiProjectionOrigin {
    match origin {
        WireProjectionOrigin::Declared => UiProjectionOrigin::Declared,
        WireProjectionOrigin::ConsumerDefault => UiProjectionOrigin::ConsumerDefault,
        WireProjectionOrigin::Forced => UiProjectionOrigin::Forced,
    }
}

fn control_preview_display_layout(preview: &UiProductPreview) -> Option<&Rc<ControlDisplayLayout>> {
    match preview {
        UiProductPreview::ControlNative(preview) => preview.display_layout.as_ref(),
        _ => None,
    }
}

/// The layout the fresh preview should carry. Reuses the cached `Rc` on the
/// per-tick `Unchanged`/`Omitted` paths — a dome-scale layout is 1500 lamps,
/// and deep-copying it every tick was a measurable slice of the 2026-08-05
/// editor-perf trace. Only a genuinely new engine-sent layout allocates.
fn display_layout_from_probe_result(
    result: &ControlDisplayLayoutProbeResult,
    cached: Option<&UiProductPreview>,
) -> Option<Rc<ControlDisplayLayout>> {
    match result {
        ControlDisplayLayoutProbeResult::Layout(layout) => Some(Rc::new(layout.clone())),
        ControlDisplayLayoutProbeResult::Unchanged { revision } => cached
            .and_then(control_preview_display_layout)
            .filter(|layout| layout.revision() == *revision)
            .cloned(),
        ControlDisplayLayoutProbeResult::Omitted => {
            cached.and_then(control_preview_display_layout).cloned()
        }
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
    fn binding_graph_subscription_requests_and_caches_the_snapshot() {
        let mut sync = ProjectSync::new();

        // Unsubscribed reads carry no binding-graph probe.
        let request = sync.refresh_project_read_request(Vec::new());
        assert!(request.probes.is_empty());

        sync.set_binding_graph_subscribed(true);
        let request = sync.refresh_project_read_request(Vec::new());
        assert_eq!(
            request.probes,
            vec![ProjectProbeRequest::BindingGraph(
                BindingGraphProbeRequest {
                    include_values: true,
                }
            )]
        );

        let graph = WireBindingGraph {
            revision: Revision::new(4),
            bindings: Vec::new(),
            channels: Vec::new(),
        };
        sync.apply_project_read_events(probe_events(
            4,
            vec![ProjectProbeResult::BindingGraph(
                BindingGraphProbeResult::Graph(graph.clone()),
            )],
        ))
        .expect("apply events");
        assert_eq!(sync.binding_graph(), Some(&graph));

        // Unsubscribing drops the cached snapshot.
        sync.set_binding_graph_subscribed(false);
        assert_eq!(sync.binding_graph(), None);
        let request = sync.refresh_project_read_request(Vec::new());
        assert!(request.probes.is_empty());
    }

    #[test]
    fn binding_graph_subscription_survives_begin_initial_sync() {
        // Regression: the demo flow sets the subscription at `mark_ready`,
        // then `sync_loaded_project` calls `begin_initial_sync`, which resets
        // per-read state. The subscription is a property of "a project is
        // connected", not read data, so it must survive — otherwise the
        // initial read omits the probe and every module card loses its face.
        let mut sync = ProjectSync::new();
        sync.set_binding_graph_subscribed(true);

        sync.begin_initial_sync();

        let request = sync.initial_project_read_request(Vec::new());
        assert_eq!(
            request.probes,
            vec![ProjectProbeRequest::BindingGraph(
                BindingGraphProbeRequest {
                    include_values: true,
                }
            )],
            "binding-graph probe must ride the initial read after begin_initial_sync"
        );
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
                width: UiProductPreviewFrame::VISUAL_DEFAULT.width,
                height: UiProductPreviewFrame::VISUAL_DEFAULT.height,
                format: WireTextureFormat::Srgb8,
                space: Some(WireVisualSpace::TwoD),
                policy: Some(WireConsumerPolicy::AUTO),
            })
        );
        assert_eq!(
            sync.product_preview(&UiProductRef::from_visual_product(product)),
            Some(&UiProductPreview::Pending)
        );
    }

    #[test]
    fn device_tier_frame_flows_into_visual_probe_requests() {
        let mut sync = ProjectSync::new();
        let product = VisualProduct::new(NodeId::new(7), 2);

        sync.set_visual_preview_frame(UiProductPreviewFrame::VISUAL_DEVICE);
        let request =
            sync.refresh_project_read_request(vec![UiProductRef::from_visual_product(product)]);

        assert_eq!(
            request.probes[0],
            ProjectProbeRequest::RenderProduct(RenderProductProbeRequest {
                product,
                width: UiProductPreviewFrame::VISUAL_DEVICE.width,
                height: UiProductPreviewFrame::VISUAL_DEVICE.height,
                format: WireTextureFormat::Srgb8,
                space: Some(WireVisualSpace::TwoD),
                policy: Some(WireConsumerPolicy::AUTO),
            })
        );
    }

    /// D15: a card with both checkboxes on asks the engine TWICE for one
    /// product — a 1D render (one row of samples) and a 2D one — and the
    /// two results file separately instead of overwriting each other.
    #[test]
    fn a_two_space_card_fans_out_one_probe_per_checked_space() {
        let mut sync = ProjectSync::new();
        let product = VisualProduct::new(NodeId::new(7), 2);
        let key = UiProductRef::from_visual_product(product);
        sync.set_preview_spaces(BTreeMap::from([(
            key,
            UiProductSpaceRequest {
                spaces: UiPreviewSpaces {
                    one_d: true,
                    two_d: true,
                },
                hero: UiVisualSpace::OneD,
            },
        )]));

        let request = sync.refresh_project_read_request(vec![key]);

        assert_eq!(request.probes.len(), 2, "one probe per checked space");
        assert_eq!(
            request.probes[0],
            ProjectProbeRequest::RenderProduct(RenderProductProbeRequest {
                product,
                width: UiProductPreviewFrame::VISUAL_DEFAULT.width,
                // A 1D render is one row: the compiler's 1D entry contract
                // reports `outputSize` as `(N, 1)`.
                height: 1,
                format: WireTextureFormat::Srgb8,
                space: Some(WireVisualSpace::OneD),
                policy: Some(WireConsumerPolicy::AUTO),
            }),
        );
        assert_eq!(
            request.probes[1],
            ProjectProbeRequest::RenderProduct(RenderProductProbeRequest {
                product,
                width: UiProductPreviewFrame::VISUAL_DEFAULT.width,
                height: UiProductPreviewFrame::VISUAL_DEFAULT.height,
                format: WireTextureFormat::Srgb8,
                space: Some(WireVisualSpace::TwoD),
                policy: Some(WireConsumerPolicy::AUTO),
            }),
        );

        let texture = |space, projection, bytes: Vec<u8>| {
            ProjectProbeResult::RenderProduct(RenderProductProbeResult::Texture {
                product,
                revision: Revision::new(8),
                width: 2,
                height: 1,
                format: WireTextureFormat::Srgb8,
                bytes,
                space,
                projection,
                origin: projection.map(|_| WireProjectionOrigin::Declared),
                primary: WireVisualSpace::OneD,
            })
        };
        sync.apply_project_read_events(probe_events(
            9,
            vec![
                texture(WireVisualSpace::OneD, None, vec![1, 1, 1, 2, 2, 2]),
                texture(
                    WireVisualSpace::TwoD,
                    Some(WireCellProjection::Radial),
                    vec![3, 3, 3, 4, 4, 4],
                ),
            ],
        ))
        .unwrap();

        let views = sync.product_space_views(&key);
        assert_eq!(
            views
                .iter()
                .map(|view| (view.space, view.hero))
                .collect::<Vec<_>>(),
            vec![(UiVisualSpace::OneD, true), (UiVisualSpace::TwoD, false)],
        );
        let bytes_of = |view: &UiProductSpaceView| match &view.preview {
            UiProductPreview::VisualSrgb8 { bytes, .. } => bytes.to_vec(),
            other => panic!("expected preview bytes, got {other:?}"),
        };
        assert_eq!(bytes_of(&views[0]), vec![1, 1, 1, 2, 2, 2]);
        assert_eq!(
            bytes_of(&views[1]),
            vec![3, 3, 3, 4, 4, 4],
            "the second render did not overwrite the first"
        );
        assert_eq!(
            views[1].meta.expect("2D metadata").projection,
            Some(UiCellProjection::Radial),
            "each space keeps its OWN caption metadata"
        );
        assert_eq!(views[0].meta.expect("1D metadata").projection, None);
        // The product-keyed preview — what every space-unaware surface
        // reads — is the HERO's frame, unambiguously.
        assert_eq!(
            match sync.product_preview(&key) {
                Some(UiProductPreview::VisualSrgb8 { bytes, .. }) => bytes.to_vec(),
                other => panic!("expected hero bytes, got {other:?}"),
            },
            vec![1, 1, 1, 2, 2, 2],
        );
    }

    /// Dropping a space (unchecking a box) drops its cached frame, so a
    /// card back on one space cannot render the other's stale bytes.
    #[test]
    fn unchecking_a_space_drops_its_cached_frame() {
        let mut sync = ProjectSync::new();
        let product = VisualProduct::new(NodeId::new(7), 2);
        let key = UiProductRef::from_visual_product(product);
        sync.set_preview_spaces(BTreeMap::from([(
            key,
            UiProductSpaceRequest {
                spaces: UiPreviewSpaces {
                    one_d: true,
                    two_d: true,
                },
                hero: UiVisualSpace::TwoD,
            },
        )]));
        let _ = sync.refresh_project_read_request(vec![key]);
        assert!(sync.product_preview_in(&key, UiVisualSpace::OneD).is_some());

        sync.set_preview_spaces(BTreeMap::from([(
            key,
            UiProductSpaceRequest::only(UiVisualSpace::TwoD),
        )]));

        assert_eq!(sync.product_preview_in(&key, UiVisualSpace::OneD), None);
        assert!(sync.product_preview_in(&key, UiVisualSpace::TwoD).is_some());
    }

    /// The helper P4's forced-policy tile probes reuse: same frame rules,
    /// same format, only the policy differs.
    #[test]
    fn a_tile_probe_forces_its_cell_through_the_shared_request_builder() {
        let product = VisualProduct::new(NodeId::new(3), 0);

        let request = visual_probe_request(
            product,
            UiProductPreviewFrame::VISUAL_DEFAULT,
            UiVisualSpace::TwoD,
            UiConsumerPolicy::forcing(UiCellProjection::Angular),
        );

        assert_eq!(
            request,
            RenderProductProbeRequest {
                product,
                width: UiProductPreviewFrame::VISUAL_DEFAULT.width,
                height: UiProductPreviewFrame::VISUAL_DEFAULT.height,
                format: WireTextureFormat::Srgb8,
                space: Some(WireVisualSpace::TwoD),
                policy: Some(WireConsumerPolicy {
                    default_1d_to_2d: WireCellProjection::Angular,
                    force: true,
                }),
            },
        );
    }

    /// P7 item 4: a subscribed TIME product asks for its timebase listing
    /// instead of a preview frame — there is no picture behind the handle,
    /// so the probe that would fetch one fetches the phasor rows.
    #[test]
    fn refresh_request_asks_a_time_product_for_its_timebase() {
        let mut sync = ProjectSync::new();
        let product = lpc_model::TimeProduct::new(NodeId::new(2), 0);

        let request =
            sync.refresh_project_read_request(vec![UiProductRef::from_time_product(product)]);

        assert_eq!(
            request.probes,
            vec![ProjectProbeRequest::Timebase(TimebaseProbeRequest {
                product
            })]
        );
        assert_eq!(
            sync.product_preview(&UiProductRef::from_time_product(product)),
            None,
            "and no Pending preview is parked for a frame that can never arrive"
        );
    }

    /// Both probe answers cache, and they are DIFFERENT answers: `Unknown`
    /// is a real verdict, while "no read yet" is the absence of an entry.
    #[test]
    fn a_timebase_probe_result_caches_live_rows_and_unknown_alike() {
        let mut sync = ProjectSync::new();
        let product = lpc_model::TimeProduct::new(NodeId::new(2), 0);
        let key = UiProductRef::from_time_product(product);
        assert_eq!(sync.timebase(&key), None, "nothing read yet");

        let row = lpc_wire::WirePhasorRow {
            origin: lpc_wire::WirePhasorOrigin::Node {
                node: 8,
                slot: "phase".to_string(),
            },
            phase: 0.25,
            cycle: 3,
            period_seconds: 4.0,
            readings: vec![],
        };
        sync.apply_project_read_events(probe_events(
            9,
            vec![ProjectProbeResult::Timebase(
                TimebaseProbeResult::Timebase {
                    product,
                    revision: Revision::new(8),
                    seconds: 3.5,
                    delta_seconds: 0.033,
                    phasors: vec![row.clone()],
                },
            )],
        ))
        .unwrap();

        assert_eq!(
            sync.timebase(&key),
            Some(&UiTimebaseRead::Live {
                seconds: 3.5,
                delta_seconds: 0.033,
                phasors: vec![row],
            })
        );

        sync.apply_project_read_events(probe_events(
            10,
            vec![ProjectProbeResult::Timebase(TimebaseProbeResult::Unknown {
                product,
            })],
        ))
        .unwrap();

        assert_eq!(sync.timebase(&key), Some(&UiTimebaseRead::Unknown));
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
                    space: WireVisualSpace::TwoD,
                    projection: Some(WireCellProjection::Radial),
                    origin: Some(WireProjectionOrigin::Declared),
                    primary: WireVisualSpace::OneD,
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
        // P2 carries the result's space metadata into its own cache
        // (P3 wires it into the preview checkboxes' captions).
        assert_eq!(
            sync.product_space(&UiProductRef::from_visual_product(product)),
            Some(&UiVisualProductSpace {
                space: UiVisualSpace::TwoD,
                projection: Some(UiCellProjection::Radial),
                origin: Some(UiProjectionOrigin::Declared),
                primary: UiVisualSpace::OneD,
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
                display_layout: Some(Rc::new(display_layout)),
                bytes: Rc::from(second_bytes.as_slice()),
            }))
        );
    }

    fn overlay_test_artifact() -> lpc_model::ArtifactLocation {
        lpc_model::ArtifactLocation::file("/orbit.shader.toml")
    }

    fn overlay_test_path() -> SlotPath {
        SlotPath::parse("transport.rate").unwrap()
    }

    fn put_cmd(id: u64, value: f32) -> (MutationCmd, MutationEffect) {
        (
            MutationCmd {
                id: lpc_model::MutationCmdId::new(id),
                mutation: lpc_model::MutationOp::PutSlotEdit {
                    artifact: overlay_test_artifact(),
                    edit: lpc_model::SlotEdit::assign_value(
                        overlay_test_path(),
                        lpc_model::LpValue::F32(value),
                    ),
                },
            },
            MutationEffect::overlay_changed(true),
        )
    }

    fn remove_cmd(id: u64) -> (MutationCmd, MutationEffect) {
        (
            MutationCmd {
                id: lpc_model::MutationCmdId::new(id),
                mutation: lpc_model::MutationOp::RemoveSlotEdit {
                    artifact: overlay_test_artifact(),
                    path: overlay_test_path(),
                },
            },
            MutationEffect::overlay_changed(true),
        )
    }

    /// [`put_cmd`] whose ack effect carries a base-display annotation.
    fn put_cmd_with_base(
        id: u64,
        value: f32,
        base_display: Option<&str>,
    ) -> (MutationCmd, MutationEffect) {
        let (command, effect) = put_cmd(id, value);
        (
            command,
            effect.with_base_display(base_display.map(str::to_string)),
        )
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
        sync.apply_overlay_read(overlay, Vec::new(), Revision::new(4));
        assert!(!sync.overlay_fetch_needed());
        assert_eq!(sync.overlay_slot_edits().count(), 1, "dirty but quiet");
    }

    #[test]
    fn summary_reports_the_overlay_mirror_revision() {
        let mut sync = ProjectSync::new();
        assert_eq!(sync.summary().overlay_revision, 0);

        sync.apply_acked_edits(&[put_cmd(1, 1.0)], Revision::new(6));

        assert_eq!(sync.summary().overlay_revision, 6);
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
        sync.apply_overlay_read(replacement, Vec::new(), Revision::new(7));

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
    fn normalized_ack_removes_the_mirrored_entry_instead_of_applying_the_put() {
        // Set-back-to-base: the sent command is a Put, but the server stored a
        // removal. The mirror must apply the effect — applying the Put would
        // leave the slot dirty forever (a no-op ack does not advance the
        // overlay revision, so no corrective fetch is ever due).
        let mut sync = ProjectSync::new();
        sync.apply_acked_edits(&[put_cmd(1, 2.0)], Revision::new(3));
        assert_eq!(sync.overlay_slot_edits().count(), 1);

        let (put_back, _) = put_cmd(2, 1.0);
        sync.apply_acked_edits(
            &[(put_back, MutationEffect::normalized_to_removal(true))],
            Revision::new(4),
        );

        assert!(
            sync.overlay().is_empty(),
            "mirror mirrors the stored removal"
        );
        assert_eq!(sync.overlay_revision(), Revision::new(4));

        // The no-op flavor (no prior entry) leaves the mirror clean too.
        let (noop_put, _) = put_cmd(3, 1.0);
        sync.apply_acked_edits(
            &[(noop_put, MutationEffect::normalized_to_removal(false))],
            Revision::new(4),
        );
        assert!(sync.overlay().is_empty());
    }

    #[test]
    fn materialized_ack_replays_each_stored_edit_into_the_mirror() {
        // A move's ack carries several per-path stored edits; the mirror must
        // replay all of them — including removals of pre-existing entries —
        // without a follow-up fetch.
        let mut sync = ProjectSync::new();
        let map = SlotPath::parse("mapping.PathPoints.paths").unwrap();
        let from = map.child_key(lpc_model::SlotMapKey::U32(0));
        let to = map.child_key(lpc_model::SlotMapKey::U32(1));

        // Pre-existing mirrored edit under the source entry: the ack's
        // Removed entries must clear it (stranded-descendant cleanup).
        let stale = SlotPath::parse("mapping.PathPoints.paths[0].PointList.first_channel").unwrap();
        sync.apply_acked_edits(
            &[(
                MutationCmd {
                    id: lpc_model::MutationCmdId::new(1),
                    mutation: lpc_model::MutationOp::PutSlotEdit {
                        artifact: overlay_test_artifact(),
                        edit: lpc_model::SlotEdit::assign_value(
                            stale.clone(),
                            lpc_model::LpValue::U32(5),
                        ),
                    },
                },
                MutationEffect::overlay_changed(true),
            )],
            Revision::new(3),
        );

        let command = MutationCmd {
            id: lpc_model::MutationCmdId::new(2),
            mutation: lpc_model::MutationOp::MoveSlotEntry {
                artifact: overlay_test_artifact(),
                from: from.clone(),
                to: to.clone(),
            },
        };
        let effect = MutationEffect::Materialized {
            edits: vec![
                lpc_model::StoredSlotEdit::put(lpc_model::SlotEdit::ensure_present(to.clone())),
                lpc_model::StoredSlotEdit::put(lpc_model::SlotEdit::assign_value(
                    to.child(lpc_model::SlotName::parse("PointList").unwrap())
                        .child(lpc_model::SlotName::parse("first_channel").unwrap()),
                    lpc_model::LpValue::U32(5),
                )),
                lpc_model::StoredSlotEdit::removed(from.clone()),
                lpc_model::StoredSlotEdit::removed(stale.clone()),
            ],
            changed: true,
        };
        sync.apply_acked_edits(&[(command, effect)], Revision::new(4));

        assert_eq!(
            sync.overlay_edit_at(&overlay_test_artifact(), &to),
            Some(&SlotEditOp::EnsurePresent)
        );
        assert_eq!(
            sync.overlay_edit_at(&overlay_test_artifact(), &from),
            None,
            "the move's source entry is gone from the mirror"
        );
        assert_eq!(
            sync.overlay_edit_at(&overlay_test_artifact(), &stale),
            None,
            "stranded descendant edits are cleared by the ack's Removed entries"
        );
        assert_eq!(sync.overlay_slot_edits().count(), 2, "ensure + leaf assign");
        assert_eq!(sync.overlay_revision(), Revision::new(4));
    }

    #[test]
    fn acked_asset_body_and_clear_round_trip_through_mirror() {
        // A SetArtifactBody ack applies as sent (no normalization exists for
        // whole-artifact ops); ClearArtifact removes the entry again.
        let mut sync = ProjectSync::new();
        let artifact = lpc_model::ArtifactLocation::file("/shader.glsl");
        let body = AssetBodyOverlay::ReplaceBody(b"void main() {}".to_vec());

        sync.apply_acked_edits(
            &[(
                MutationCmd {
                    id: lpc_model::MutationCmdId::new(1),
                    mutation: lpc_model::MutationOp::SetArtifactBody {
                        artifact: artifact.clone(),
                        edit: body.clone(),
                    },
                },
                MutationEffect::OverlayChanged {
                    changed: true,
                    base_display: None,
                },
            )],
            Revision::new(3),
        );

        assert_eq!(sync.overlay_asset_edit_at(&artifact), Some(&body));
        assert_eq!(
            sync.overlay_asset_edits().collect::<Vec<_>>(),
            vec![(&artifact, &body)]
        );
        assert_eq!(
            sync.overlay_slot_edits().count(),
            0,
            "asset entries never leak into the slot iterator"
        );
        assert_eq!(sync.overlay_revision(), Revision::new(3));

        sync.apply_acked_edits(
            &[(
                MutationCmd {
                    id: lpc_model::MutationCmdId::new(2),
                    mutation: lpc_model::MutationOp::ClearArtifact {
                        artifact: artifact.clone(),
                    },
                },
                MutationEffect::OverlayChanged {
                    changed: true,
                    base_display: None,
                },
            )],
            Revision::new(4),
        );

        assert_eq!(sync.overlay_asset_edit_at(&artifact), None);
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

    #[test]
    fn annotated_ack_installs_the_base_value_and_revert_drops_it() {
        let mut sync = ProjectSync::new();

        sync.apply_acked_edits(&[put_cmd_with_base(1, 2.0, Some("1.0"))], Revision::new(3));

        assert_eq!(
            sync.base_value_at(&overlay_test_artifact(), &overlay_test_path()),
            Some("1.0"),
            "the client's own ack carries the old value with no fetch"
        );

        sync.apply_acked_edits(&[remove_cmd(2)], Revision::new(4));

        assert_eq!(
            sync.base_value_at(&overlay_test_artifact(), &overlay_test_path()),
            None,
            "the base value drops with its overlay entry"
        );
        assert!(sync.overlay().is_empty());
    }

    #[test]
    fn normalized_ack_drops_the_base_value_with_the_entry() {
        let mut sync = ProjectSync::new();
        sync.apply_acked_edits(&[put_cmd_with_base(1, 2.0, Some("1.0"))], Revision::new(3));

        let (put_back, _) = put_cmd(2, 1.0);
        sync.apply_acked_edits(
            &[(
                put_back,
                MutationEffect::normalized_to_removal(true).with_base_display(Some("1.0".into())),
            )],
            Revision::new(4),
        );

        assert!(sync.overlay().is_empty());
        assert_eq!(
            sync.base_value_at(&overlay_test_artifact(), &overlay_test_path()),
            None,
            "a normalized removal leaves no base value behind"
        );
    }

    #[test]
    fn unannotated_ack_degrades_the_base_value_to_none() {
        // An overlay entry can exist without a derivable base (base-absent
        // target, or a server that annotated nothing): re-acking the path
        // without an annotation must clear any stale base value rather than
        // let an outdated string linger beside the fresh edit.
        let mut sync = ProjectSync::new();
        sync.apply_acked_edits(&[put_cmd_with_base(1, 2.0, Some("1.0"))], Revision::new(3));
        assert!(
            sync.base_value_at(&overlay_test_artifact(), &overlay_test_path())
                .is_some()
        );

        sync.apply_acked_edits(&[put_cmd(2, 3.0)], Revision::new(4));

        assert_eq!(
            sync.overlay_edit_at(&overlay_test_artifact(), &overlay_test_path()),
            Some(&SlotEditOp::AssignValue(lpc_model::LpValue::F32(3.0))),
            "the overlay entry itself survives"
        );
        assert_eq!(
            sync.base_value_at(&overlay_test_artifact(), &overlay_test_path()),
            None,
            "entries without annotations degrade to None"
        );
    }

    #[test]
    fn overlay_read_replaces_the_base_value_map_wholesale() {
        let mut sync = ProjectSync::new();
        sync.apply_acked_edits(&[put_cmd_with_base(1, 2.0, Some("1.0"))], Revision::new(3));

        let mut replacement = ProjectOverlay::new();
        let other_path = SlotPath::parse("controls.hue").unwrap();
        replacement.put_slot_edit(
            overlay_test_artifact(),
            lpc_model::SlotEdit::assign_value(other_path.clone(), lpc_model::LpValue::F32(0.5)),
        );
        sync.apply_overlay_read(
            replacement,
            vec![(overlay_test_artifact(), other_path.clone(), "0.25".into())],
            Revision::new(7),
        );

        assert_eq!(
            sync.base_value_at(&overlay_test_artifact(), &overlay_test_path()),
            None,
            "stale base values do not survive the wholesale read"
        );
        assert_eq!(
            sync.base_value_at(&overlay_test_artifact(), &other_path),
            Some("0.25"),
            "reconnect/foreign-edit reads restore the bases they carry"
        );
    }

    #[test]
    fn materialized_ack_updates_base_values_per_stored_edit() {
        // A move's ack annotates its stored puts individually; Removed
        // entries drop their base values, including stranded descendants the
        // canonicalization cleared.
        let mut sync = ProjectSync::new();
        let map = SlotPath::parse("mapping.PathPoints.paths").unwrap();
        let from = map.child_key(lpc_model::SlotMapKey::U32(0));
        let to = map.child_key(lpc_model::SlotMapKey::U32(1));
        let stale = SlotPath::parse("mapping.PathPoints.paths[0].PointList.first_channel").unwrap();
        sync.apply_acked_edits(
            &[(
                MutationCmd {
                    id: lpc_model::MutationCmdId::new(1),
                    mutation: lpc_model::MutationOp::PutSlotEdit {
                        artifact: overlay_test_artifact(),
                        edit: lpc_model::SlotEdit::assign_value(
                            stale.clone(),
                            lpc_model::LpValue::U32(5),
                        ),
                    },
                },
                MutationEffect::overlay_changed(true).with_base_display(Some("7".into())),
            )],
            Revision::new(3),
        );
        assert_eq!(
            sync.base_value_at(&overlay_test_artifact(), &stale),
            Some("7")
        );

        let command = MutationCmd {
            id: lpc_model::MutationCmdId::new(2),
            mutation: lpc_model::MutationOp::MoveSlotEntry {
                artifact: overlay_test_artifact(),
                from: from.clone(),
                to: to.clone(),
            },
        };
        let effect = MutationEffect::Materialized {
            edits: vec![
                lpc_model::StoredSlotEdit::put(lpc_model::SlotEdit::ensure_present(to.clone())),
                lpc_model::StoredSlotEdit::put_with_base_display(
                    lpc_model::SlotEdit::remove(from.clone()),
                    Some("{\"kind\":\"RingArray\"}".into()),
                ),
                lpc_model::StoredSlotEdit::removed(stale.clone()),
            ],
            changed: true,
        };
        sync.apply_acked_edits(&[(command, effect)], Revision::new(4));

        assert_eq!(
            sync.base_value_at(&overlay_test_artifact(), &to),
            None,
            "base-absent move target has no old value"
        );
        assert_eq!(
            sync.base_value_at(&overlay_test_artifact(), &from),
            Some("{\"kind\":\"RingArray\"}"),
            "the stored remove of the base-present source keeps its base display"
        );
        assert_eq!(
            sync.base_value_at(&overlay_test_artifact(), &stale),
            None,
            "stranded descendants drop their base values with their entries"
        );
    }
}
