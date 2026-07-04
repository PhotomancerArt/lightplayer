//! Progressive apply of a project-read event stream to a [`ProjectView`].
//!
//! The wire protocol delivers a project read as an ordered stream of
//! [`ProjectReadEvent`] values (batched into frames by the transport). This
//! module applies those events directly to the client-side [`ProjectView`] as
//! they arrive. There is no aggregate response shape — the applier is the only
//! consumer of the event stream.
//!
//! [`ProjectReadApplier`] owns the full set of stream invariants (see the M6
//! discovery notes §5):
//!
//! - `Begin` arrives exactly once and captures the stream revision; every other
//!   event must arrive after `Begin`.
//! - Per query index: the query kind stays consistent across events; each
//!   family's `Begin`/`End` are paired; runtime is emitted at most once.
//! - Runtime-buffer payload chunk reassembly validates offsets and total
//!   length; each `Probe`/query result index appears at most once.
//! - Chunked *probe* results (`ResultBegin`/`ResultBytes`/`ResultEnd`) reassemble
//!   with the same offset/length strictness as runtime-buffer chunks and deliver
//!   a completed [`ProjectProbeResult`] identical to the whole-result path.
//! - `End`'s revision must equal `Begin`'s revision.
//! - Completion happens exactly once; nothing may follow the terminal event.
//! - An `Error { message }` event is surfaced as [`ProjectReadApplyStreamError::Remote`].
//! - Query and probe result indexes must be contiguous from 0 (the same
//!   strictness the collector applies at finish).
//!
//! # Probe results
//!
//! Probe results are read-time diagnostics, not part of the persistent mirror,
//! so they are *not* written onto the [`ProjectView`]. Instead the applier
//! collects completed results (whole or reassembled from chunks), keyed by probe
//! index, and exposes them after the stream completes via
//! [`ProjectReadApplier::completed_probe_results`] /
//! [`ProjectReadApplier::take_completed_probe_results`]. This is the seam a
//! caller's single probe-extraction helper points at: drive the stream to
//! [`ApplyStatus::Complete`], then read the probes back in index order.
//!
//! Frame `sequence` / envelope framing is deliberately *not* this type's
//! concern: the applier consumes [`ProjectReadEvent`] values and is
//! envelope-agnostic (that lives in the transport/session layer, M6 P1).
//!
//! # Mid-stream failure semantics
//!
//! [`ProjectView::revision`] advances only on `Complete`, mirroring the
//! aggregate path (revision is written last). A stream that fails partway
//! through — a protocol violation, a remote `Error`, or a sub-apply error —
//! can therefore leave *partially applied* family state in the view: families
//! whose `End` was reached before the failure are already merged, while the
//! view's `revision` still reflects the last fully applied stream. This is the
//! same guarantee the client has had since M5 P4's additive applies: the view
//! stays internally consistent per family, and the next full (or gated) read
//! self-corrects any partial state.

use alloc::collections::BTreeSet;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use lp_collection::VecMap;
use lpc_model::{
    NodeId, ResourceRef, Revision, SlotShapeEntry, SlotShapeId, SlotShapeRegistrySnapshot,
};
use lpc_wire::{
    ProjectProbeResult, ProjectProbeResultHeader, ProjectReadEvent, ProjectReadNodeEvent,
    ProjectReadProbeEvent, ProjectReadQueryEvent, ProjectReadResourceEvent, ProjectReadShapeEvent,
    ReadLevel, WireResourceSummary, WireRuntimeBufferMetadataPayload, WireRuntimeBufferPayload,
    WireSlotRootSnapshot, WireSlotRootsSnapshot, WireTreeDelta,
};

use super::ProjectView;
use crate::slot::SlotMirrorError;
use crate::tree::{ApplyError, apply_tree_deltas_collecting_removed};

/// Error applying project-read family payloads to a [`ProjectView`].
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

/// Outcome of applying one [`ProjectReadEvent`] to a [`ProjectReadApplier`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyStatus {
    /// The stream is still open; feed the next event.
    Continue,
    /// The terminal `End` event was applied; `revision` is the stream revision
    /// now stamped on the view.
    Complete { revision: Revision },
}

/// Error while applying a project-read event stream.
///
/// `Remote` is a server-side read failure carried by
/// [`ProjectReadEvent::Error`]. `Protocol` means the event stream itself was
/// malformed (events before `Begin`, kind mismatches, offset gaps, revision
/// mismatch, non-contiguous indexes, …). `Apply` wraps a failure applying a
/// well-formed event to the view.
#[derive(Debug, Clone, PartialEq)]
pub enum ProjectReadApplyStreamError {
    Remote(String),
    Protocol(String),
    Apply(ProjectReadApplyError),
}

impl core::fmt::Display for ProjectReadApplyStreamError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Remote(message) => write!(f, "project read failed: {message}"),
            Self::Protocol(message) => write!(f, "project read stream protocol error: {message}"),
            Self::Apply(error) => write!(f, "project read apply error: {error}"),
        }
    }
}

impl core::error::Error for ProjectReadApplyStreamError {}

impl From<ProjectReadApplyError> for ProjectReadApplyStreamError {
    fn from(value: ProjectReadApplyError) -> Self {
        Self::Apply(value)
    }
}

fn protocol(message: impl Into<String>) -> ProjectReadApplyStreamError {
    ProjectReadApplyStreamError::Protocol(message.into())
}

/// Applies a project-read event stream progressively to a [`ProjectView`].
///
/// Construct one applier per stream, feed each [`ProjectReadEvent`] to
/// [`apply`](Self::apply), and stop when it returns [`ApplyStatus::Complete`]
/// (or an [`ProjectReadApplyStreamError`]). The view is mutated in place as families close; see
/// the module docs for the mid-stream failure guarantee.
pub struct ProjectReadApplier<'view> {
    view: &'view mut ProjectView,
    /// Stream revision captured from `Begin`; `None` until `Begin` arrives.
    revision: Option<Revision>,
    /// Set of query indexes whose family `End` has been applied, so contiguity
    /// (from 0) can be checked at stream end.
    completed_queries: BTreeSet<u32>,
    /// Completed probe results keyed by probe index (whole or reassembled from
    /// chunks). Exposed to callers after the stream completes; contiguity from 0
    /// is checked at finish.
    completed_probes: VecMap<u32, ProjectProbeResult>,
    /// Per-probe-index in-flight chunk accumulation for a chunked result.
    pending_probes: VecMap<u32, PendingProbeResult>,
    /// Per-query in-flight accumulation, keyed by query index.
    queries: VecMap<u32, QueryState>,
    /// Set once the terminal event (`End`/`Error`) has been applied.
    complete: bool,
}

impl<'view> ProjectReadApplier<'view> {
    /// Start a new applier borrowing `view` for the duration of the stream.
    #[must_use]
    pub fn new(view: &'view mut ProjectView) -> Self {
        Self {
            view,
            revision: None,
            completed_queries: BTreeSet::new(),
            completed_probes: VecMap::new(),
            pending_probes: VecMap::new(),
            queries: VecMap::new(),
            complete: false,
        }
    }

    /// Completed probe results in probe-index order.
    ///
    /// Meaningful only after the stream reaches [`ApplyStatus::Complete`]; while
    /// the stream is open the set may be incomplete or (for chunked results)
    /// mid-reassembly. Each entry corresponds to one
    /// [`ProjectReadEvent::Probe`] index and is byte-identical to what the whole
    /// `Result` path would have produced.
    #[must_use]
    pub fn completed_probe_results(&self) -> impl Iterator<Item = (u32, &ProjectProbeResult)> {
        self.completed_probes
            .iter()
            .map(|(index, result)| (*index, result))
    }

    /// Take the completed probe results, in probe-index order, leaving the
    /// applier's probe set empty.
    ///
    /// The single probe-extraction seam callers use after driving the stream to
    /// completion: the results come out ordered by probe index (contiguity from
    /// 0 was validated at stream end).
    #[must_use]
    pub fn take_completed_probe_results(&mut self) -> Vec<ProjectProbeResult> {
        core::mem::take(&mut self.completed_probes)
            .into_iter()
            .map(|(_, result)| result)
            .collect()
    }

    /// Apply one event, mutating the view for family-closing events.
    pub fn apply(
        &mut self,
        event: ProjectReadEvent,
    ) -> Result<ApplyStatus, ProjectReadApplyStreamError> {
        if self.complete {
            return Err(protocol("project read stream is already complete"));
        }
        match event {
            ProjectReadEvent::Begin { revision } => {
                if self.revision.replace(revision).is_some() {
                    return Err(protocol("project read stream began twice"));
                }
                Ok(ApplyStatus::Continue)
            }
            ProjectReadEvent::Query { index, event } => {
                self.ensure_started()?;
                self.apply_query_event(index, event)?;
                Ok(ApplyStatus::Continue)
            }
            ProjectReadEvent::Probe { index, event } => {
                self.ensure_started()?;
                self.apply_probe_event(index, event)?;
                Ok(ApplyStatus::Continue)
            }
            ProjectReadEvent::End { revision } => {
                let begin_revision = self.ensure_started()?;
                if begin_revision != revision {
                    return Err(protocol(format!(
                        "project read end revision {} did not match begin revision {}",
                        revision.0, begin_revision.0
                    )));
                }
                self.finish()?;
                self.complete = true;
                // Advance the view revision last: a mid-stream failure never
                // claims the new revision (mirrors the aggregate path).
                self.view.revision = revision;
                Ok(ApplyStatus::Complete { revision })
            }
            ProjectReadEvent::Error { message } => {
                self.complete = true;
                Err(ProjectReadApplyStreamError::Remote(message))
            }
        }
    }

    fn ensure_started(&self) -> Result<Revision, ProjectReadApplyStreamError> {
        self.revision
            .ok_or_else(|| protocol("project read stream event arrived before begin"))
    }

    fn apply_query_event(
        &mut self,
        index: u32,
        event: ProjectReadQueryEvent,
    ) -> Result<(), ProjectReadApplyStreamError> {
        if self.completed_queries.contains(&index) {
            return Err(protocol(format!("query {index} emitted after its end")));
        }
        let revision = self.revision.expect("started");
        match event {
            ProjectReadQueryEvent::Shapes(event) => {
                let state = self.query_state(index, QueryKind::Shapes)?;
                let QueryState::Shapes(shapes) = state else {
                    return Err(protocol("internal query kind mismatch"));
                };
                if shapes.accept(event)? {
                    let QueryState::Shapes(shapes) = self.take_query(index) else {
                        unreachable!("kind checked above");
                    };
                    shapes.apply_to(self.view);
                    self.mark_query_complete(index);
                }
            }
            ProjectReadQueryEvent::Nodes(event) => {
                let state = self.query_state(index, QueryKind::Nodes)?;
                let QueryState::Nodes(nodes) = state else {
                    return Err(protocol("internal query kind mismatch"));
                };
                if nodes.accept(event)? {
                    let QueryState::Nodes(nodes) = self.take_query(index) else {
                        unreachable!("kind checked above");
                    };
                    nodes.apply_to(self.view, revision)?;
                    self.mark_query_complete(index);
                }
            }
            ProjectReadQueryEvent::Resources(event) => {
                let state = self.query_state(index, QueryKind::Resources)?;
                let QueryState::Resources(resources) = state else {
                    return Err(protocol("internal query kind mismatch"));
                };
                if resources.accept(event)? {
                    let QueryState::Resources(resources) = self.take_query(index) else {
                        unreachable!("kind checked above");
                    };
                    resources.apply_to(self.view);
                    self.mark_query_complete(index);
                }
            }
            ProjectReadQueryEvent::Runtime(runtime) => {
                // Runtime carries a single whole result and has no begin/end, so
                // it completes immediately. An index already in-flight with
                // another family is a kind mismatch; re-emission at a completed
                // index is caught by the completed-queries guard above.
                if let Some(existing) = self.queries.get(&index) {
                    return Err(protocol(format!(
                        "query {index} mixed {:?} and {:?} events",
                        existing.kind(),
                        QueryKind::Runtime
                    )));
                }
                self.view.runtime = Some(runtime);
                self.mark_query_complete(index);
            }
        }
        Ok(())
    }

    fn apply_probe_event(
        &mut self,
        index: u32,
        event: ProjectReadProbeEvent,
    ) -> Result<(), ProjectReadApplyStreamError> {
        match event {
            ProjectReadProbeEvent::Result(result) => self.complete_probe(index, result),
            ProjectReadProbeEvent::ResultBegin {
                byte_length,
                header,
            } => {
                if self.completed_probes.contains_key(&index) {
                    return Err(protocol(format!("probe {index} emitted after its result")));
                }
                if self
                    .pending_probes
                    .insert(
                        index,
                        PendingProbeResult {
                            header,
                            byte_length,
                            bytes: Vec::new(),
                        },
                    )
                    .is_some()
                {
                    return Err(protocol(format!("probe {index} result began twice")));
                }
                Ok(())
            }
            ProjectReadProbeEvent::ResultBytes { offset, bytes } => {
                let pending = self.pending_probes.get_mut(&index).ok_or_else(|| {
                    protocol(format!("probe {index} result bytes arrived before begin"))
                })?;
                if usize::try_from(offset).ok() != Some(pending.bytes.len()) {
                    return Err(protocol(format!(
                        "probe {index} result expected offset {}, got {offset}",
                        pending.bytes.len()
                    )));
                }
                pending.bytes.extend(bytes);
                Ok(())
            }
            ProjectReadProbeEvent::ResultEnd => {
                let pending = self.pending_probes.remove(&index).ok_or_else(|| {
                    protocol(format!("probe {index} result end arrived before begin"))
                })?;
                if usize::try_from(pending.byte_length).ok() != Some(pending.bytes.len()) {
                    return Err(protocol(format!(
                        "probe {index} result expected {} bytes, got {}",
                        pending.byte_length,
                        pending.bytes.len()
                    )));
                }
                let result = pending.header.into_result(pending.bytes);
                self.complete_probe(index, result)
            }
        }
    }

    /// Record a finished probe result (whole or reassembled), rejecting a second
    /// result at the same index.
    fn complete_probe(
        &mut self,
        index: u32,
        result: ProjectProbeResult,
    ) -> Result<(), ProjectReadApplyStreamError> {
        if self.completed_probes.insert(index, result).is_some() {
            return Err(protocol(format!("probe {index} emitted twice")));
        }
        Ok(())
    }

    fn query_state(
        &mut self,
        index: u32,
        kind: QueryKind,
    ) -> Result<&mut QueryState, ProjectReadApplyStreamError> {
        if let Some(existing) = self.queries.get(&index)
            && existing.kind() != kind
        {
            return Err(protocol(format!(
                "query {index} mixed {:?} and {:?} events",
                existing.kind(),
                kind
            )));
        }
        if self.queries.get(&index).is_none() {
            self.queries.insert(index, kind.empty_state());
        }
        Ok(self.queries.get_mut(&index).expect("just inserted"))
    }

    fn take_query(&mut self, index: u32) -> QueryState {
        self.queries.remove(&index).expect("query present")
    }

    fn mark_query_complete(&mut self, index: u32) {
        self.completed_queries.insert(index);
    }

    /// Validate index contiguity once the stream ends. Any query left in-flight
    /// (opened but never `End`ed) or a gap in the completed index set is a
    /// protocol error, matching the collector's finish-time checks.
    fn finish(&mut self) -> Result<(), ProjectReadApplyStreamError> {
        if let Some((index, _)) = self.queries.iter().next() {
            return Err(protocol(format!("query {index} did not end")));
        }
        if let Some((index, _)) = self.pending_probes.iter().next() {
            return Err(protocol(format!("probe {index} result did not end")));
        }
        ensure_contiguous(self.completed_queries.iter().copied(), "query")?;
        ensure_contiguous(self.completed_probes.keys().copied(), "probe")?;
        Ok(())
    }
}

/// Contiguity check: the ascending indexes must be exactly `{0, 1, .., n-1}`.
fn ensure_contiguous(
    indexes: impl IntoIterator<Item = u32>,
    label: &str,
) -> Result<(), ProjectReadApplyStreamError> {
    for (expected, index) in (0_u32..).zip(indexes) {
        if index != expected {
            return Err(protocol(format!(
                "missing {label} result index {expected}; next index was {index}"
            )));
        }
    }
    Ok(())
}

/// In-flight reassembly state for a chunked probe result.
#[derive(Debug)]
struct PendingProbeResult {
    header: ProjectProbeResultHeader,
    byte_length: u32,
    bytes: Vec<u8>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum QueryKind {
    Shapes,
    Nodes,
    Resources,
    Runtime,
}

impl QueryKind {
    fn empty_state(self) -> QueryState {
        match self {
            Self::Shapes => QueryState::Shapes(ShapeState::default()),
            Self::Nodes => QueryState::Nodes(NodeState::default()),
            Self::Resources => QueryState::Resources(ResourceState::default()),
            Self::Runtime => QueryState::Runtime,
        }
    }
}

#[derive(Debug)]
enum QueryState {
    Shapes(ShapeState),
    Nodes(NodeState),
    Resources(ResourceState),
    Runtime,
}

impl QueryState {
    fn kind(&self) -> QueryKind {
        match self {
            Self::Shapes(_) => QueryKind::Shapes,
            Self::Nodes(_) => QueryKind::Nodes,
            Self::Resources(_) => QueryKind::Resources,
            Self::Runtime => QueryKind::Runtime,
        }
    }
}

// ---- Shapes ----

#[derive(Debug, Default)]
struct ShapeState {
    level: Option<ReadLevel>,
    ids_revision: Option<Revision>,
    shapes: VecMap<SlotShapeId, SlotShapeEntry>,
    membership: Option<Vec<SlotShapeId>>,
    ended: bool,
}

impl ShapeState {
    /// Accept one shape event; returns `true` when the family has ended and is
    /// ready to apply.
    fn accept(
        &mut self,
        event: ProjectReadShapeEvent,
    ) -> Result<bool, ProjectReadApplyStreamError> {
        match event {
            ProjectReadShapeEvent::Begin {
                level,
                ids_revision,
            } => {
                if self.level.replace(level).is_some() {
                    return Err(protocol("shape query began twice"));
                }
                self.ids_revision = Some(ids_revision);
            }
            ProjectReadShapeEvent::Entry { id, entry } => {
                self.ensure_open("shape entry")?;
                self.shapes.insert(id, entry);
            }
            ProjectReadShapeEvent::Membership { ids } => {
                self.ensure_open("shape membership")?;
                if self.membership.replace(ids).is_some() {
                    return Err(protocol("shape membership sent twice"));
                }
            }
            ProjectReadShapeEvent::End => {
                self.ensure_open("shape end")?;
                self.ended = true;
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn apply_to(self, view: &mut ProjectView) {
        // Merge (upsert) the shape entries via the additive registry page path,
        // then prune to membership when present (a gated read includes it only
        // when the id set changed).
        let ids_revision = self.ids_revision.unwrap_or_default();
        view.slots.apply_registry_page(SlotShapeRegistrySnapshot {
            ids_revision,
            shapes: self.shapes,
        });
        if let Some(membership) = &self.membership {
            view.slots.prune_shapes(membership);
        }
    }

    fn ensure_open(&self, label: &str) -> Result<(), ProjectReadApplyStreamError> {
        if self.level.is_none() {
            return Err(protocol(format!("{label} arrived before shape begin")));
        }
        if self.ended {
            return Err(protocol(format!("{label} arrived after shape end")));
        }
        Ok(())
    }
}

// ---- Nodes ----

#[derive(Debug, Default)]
struct NodeState {
    level: Option<ReadLevel>,
    deltas: Vec<WireTreeDelta>,
    roots: Vec<WireSlotRootSnapshot>,
    ended: bool,
}

impl NodeState {
    fn accept(&mut self, event: ProjectReadNodeEvent) -> Result<bool, ProjectReadApplyStreamError> {
        match event {
            ProjectReadNodeEvent::Begin { level } => {
                if self.level.replace(level).is_some() {
                    return Err(protocol("node query began twice"));
                }
            }
            ProjectReadNodeEvent::TreeDeltas { deltas } => {
                self.ensure_open("tree deltas")?;
                self.deltas.extend(deltas);
            }
            ProjectReadNodeEvent::SlotRoot(root) => {
                self.ensure_open("slot root")?;
                self.roots.push(root);
            }
            ProjectReadNodeEvent::End => {
                self.ensure_open("node end")?;
                self.ended = true;
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn apply_to(
        self,
        view: &mut ProjectView,
        revision: Revision,
    ) -> Result<(), ProjectReadApplyStreamError> {
        let mut removed_nodes: Vec<NodeId> = Vec::new();
        apply_tree_deltas_collecting_removed(
            &mut view.tree,
            &self.deltas,
            revision,
            &mut removed_nodes,
        )
        .map_err(ProjectReadApplyError::from)?;
        // Upsert the roots present in the payload; a gated read sends only
        // changed roots, so unchanged roots must survive.
        view.slots
            .apply_roots_snapshot(WireSlotRootsSnapshot { roots: self.roots })
            .map_err(ProjectReadApplyError::from)?;
        // Drop slot roots owned by nodes removed via the tree deltas.
        if !removed_nodes.is_empty() {
            view.slots.drop_roots_for_nodes(&removed_nodes);
        }
        Ok(())
    }

    fn ensure_open(&self, label: &str) -> Result<(), ProjectReadApplyStreamError> {
        if self.level.is_none() {
            return Err(protocol(format!("{label} arrived before node begin")));
        }
        if self.ended {
            return Err(protocol(format!("{label} arrived after node end")));
        }
        Ok(())
    }
}

// ---- Resources ----

#[derive(Debug, Default)]
struct ResourceState {
    level: Option<ReadLevel>,
    summaries: Vec<WireResourceSummary>,
    payloads: Vec<WireRuntimeBufferPayload>,
    membership: Option<Vec<ResourceRef>>,
    pending_payloads: VecMap<ResourceRef, PendingRuntimeBufferPayload>,
    ended: bool,
}

impl ResourceState {
    fn accept(
        &mut self,
        event: ProjectReadResourceEvent,
    ) -> Result<bool, ProjectReadApplyStreamError> {
        match event {
            ProjectReadResourceEvent::Begin { level } => {
                if self.level.replace(level).is_some() {
                    return Err(protocol("resource query began twice"));
                }
            }
            ProjectReadResourceEvent::Summary(summary) => {
                self.ensure_open("resource summary")?;
                self.summaries.push(summary);
            }
            ProjectReadResourceEvent::RuntimeBufferPayload(payload) => {
                self.ensure_open("runtime buffer payload")?;
                self.payloads.push(payload);
            }
            ProjectReadResourceEvent::RuntimeBufferPayloadBegin {
                resource_ref,
                revision,
                metadata,
                byte_length,
            } => {
                self.ensure_open("runtime buffer payload begin")?;
                if self
                    .pending_payloads
                    .insert(
                        resource_ref,
                        PendingRuntimeBufferPayload {
                            revision,
                            metadata,
                            byte_length,
                            bytes: Vec::new(),
                        },
                    )
                    .is_some()
                {
                    return Err(protocol(format!(
                        "runtime buffer payload {resource_ref:?} began twice"
                    )));
                }
            }
            ProjectReadResourceEvent::RuntimeBufferPayloadBytes {
                resource_ref,
                offset,
                bytes,
            } => {
                self.ensure_open("runtime buffer payload bytes")?;
                let pending = self
                    .pending_payloads
                    .get_mut(&resource_ref)
                    .ok_or_else(|| {
                        protocol(format!(
                            "runtime buffer payload bytes for {resource_ref:?} arrived before begin"
                        ))
                    })?;
                if usize::try_from(offset).ok() != Some(pending.bytes.len()) {
                    return Err(protocol(format!(
                        "runtime buffer payload {:?} expected offset {}, got {}",
                        resource_ref,
                        pending.bytes.len(),
                        offset
                    )));
                }
                pending.bytes.extend(bytes);
            }
            ProjectReadResourceEvent::RuntimeBufferPayloadEnd { resource_ref } => {
                self.ensure_open("runtime buffer payload end")?;
                let pending = self.pending_payloads.remove(&resource_ref).ok_or_else(|| {
                    protocol(format!(
                        "runtime buffer payload end for {resource_ref:?} arrived before begin"
                    ))
                })?;
                if usize::try_from(pending.byte_length).ok() != Some(pending.bytes.len()) {
                    return Err(protocol(format!(
                        "runtime buffer payload {:?} expected {} bytes, got {}",
                        resource_ref,
                        pending.byte_length,
                        pending.bytes.len()
                    )));
                }
                self.payloads.push(WireRuntimeBufferPayload {
                    resource_ref,
                    revision: pending.revision,
                    metadata: pending.metadata,
                    bytes: pending.bytes,
                });
            }
            ProjectReadResourceEvent::Membership { refs } => {
                self.ensure_open("resource membership")?;
                if self.membership.replace(refs).is_some() {
                    return Err(protocol("resource membership sent twice"));
                }
            }
            ProjectReadResourceEvent::End => {
                self.ensure_open("resource end")?;
                if !self.pending_payloads.is_empty() {
                    return Err(protocol("resource query ended with pending payload chunks"));
                }
                self.ended = true;
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn apply_to(self, view: &mut ProjectView) {
        // Additively upsert summaries + payloads, then prune to membership.
        view.resource_cache.apply_summaries(&self.summaries);
        view.resource_cache
            .apply_runtime_buffer_payloads(&self.payloads);
        if let Some(membership) = &self.membership {
            view.resource_cache.prune_to_membership(membership);
        }
    }

    fn ensure_open(&self, label: &str) -> Result<(), ProjectReadApplyStreamError> {
        if self.level.is_none() {
            return Err(protocol(format!("{label} arrived before resource begin")));
        }
        if self.ended {
            return Err(protocol(format!("{label} arrived after resource end")));
        }
        Ok(())
    }
}

#[derive(Debug)]
struct PendingRuntimeBufferPayload {
    revision: Revision,
    metadata: WireRuntimeBufferMetadataPayload,
    byte_length: u32,
    bytes: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::{String, ToString};
    use alloc::vec;
    use lpc_model::{
        LpType, LpValue, ResourceRef, RuntimeBufferId, SlotData, SlotShape, SlotShapeId,
        SlotShapeRegistry, TreePath, WithRevision,
    };
    use lpc_wire::{
        NodeRuntimeStatus, WireChannelSampleFormat, WireChildKind, WireEntryState,
        WireResourceAvailability, WireResourceKindSummary, WireResourceMetadataSummary,
        WireResourceSummary, WireRuntimeBufferKind, WireRuntimeBufferMetadataPayload,
        WireSlotIndex, WireSlotRootSnapshot, wire_slot_data_from_slot_access,
    };

    // ---- shared fixture builders ----

    fn value_shape_id(name: &str) -> SlotShapeId {
        SlotShapeId::from_static_name(name)
    }

    fn value_registry(names: &[&str], rev: i64) -> SlotShapeRegistry {
        let mut registry = SlotShapeRegistry::default();
        for name in names {
            registry
                .register_shape_with_version(
                    Revision::new(rev),
                    value_shape_id(name),
                    SlotShape::value(LpType::F32),
                )
                .unwrap();
        }
        registry
    }

    fn value_root(
        registry: &SlotShapeRegistry,
        name: &str,
        shape: SlotShapeId,
        v: f32,
    ) -> WireSlotRootSnapshot {
        let data = SlotData::Value(WithRevision::new(Revision::new(1), LpValue::F32(v)));
        WireSlotRootSnapshot {
            name: String::from(name),
            shape,
            data: wire_slot_data_from_slot_access(registry, shape, data.access()),
        }
    }

    fn buffer_summary(id: u32, rev: i64) -> WireResourceSummary {
        WireResourceSummary {
            resource_ref: ResourceRef::runtime_buffer(RuntimeBufferId::new(id)),
            owner: None,
            revision: Revision::new(rev),
            kind: WireResourceKindSummary::RuntimeBuffer(WireRuntimeBufferKind::OutputChannels),
            metadata: WireResourceMetadataSummary::OutputChannels {
                channels: 1,
                sample_format: WireChannelSampleFormat::U8,
            },
            byte_length_hint: Some(1),
            availability: WireResourceAvailability::Available,
        }
    }

    fn created_node(
        id: u32,
        parent: Option<NodeId>,
        path: &str,
        children: Vec<NodeId>,
    ) -> WireTreeDelta {
        WireTreeDelta::Created {
            id: NodeId::new(id),
            path: TreePath::parse(path).unwrap(),
            parent,
            child_kind: parent.map(|_| WireChildKind::Input {
                source: WireSlotIndex(0),
            }),
            children,
            status: NodeRuntimeStatus::Created,
            state: WireEntryState::Pending,
            created_frame: Revision::new(1),
            change_frame: Revision::new(1),
            children_ver: Revision::new(1),
        }
    }

    fn f32_root_value(view: &ProjectView, root: &str) -> f32 {
        match view.slots.roots.get(root) {
            Some(SlotData::Value(v)) => match v.value() {
                LpValue::F32(f) => *f,
                other => panic!("root {root} not f32: {other:?}"),
            },
            other => panic!("root {root} missing/not value: {other:?}"),
        }
    }

    /// Drive a whole stream through a fresh applier over `view`.
    fn apply_stream(
        view: &mut ProjectView,
        events: Vec<ProjectReadEvent>,
    ) -> Result<Revision, ProjectReadApplyStreamError> {
        let mut applier = ProjectReadApplier::new(view);
        let mut done = None;
        for event in events {
            match applier.apply(event)? {
                ApplyStatus::Continue => {}
                ApplyStatus::Complete { revision } => {
                    done = Some(revision);
                }
            }
        }
        done.ok_or_else(|| protocol("stream did not complete"))
    }

    // Convenience event constructors.
    fn begin(rev: i64) -> ProjectReadEvent {
        ProjectReadEvent::Begin {
            revision: Revision::new(rev),
        }
    }
    fn end(rev: i64) -> ProjectReadEvent {
        ProjectReadEvent::End {
            revision: Revision::new(rev),
        }
    }
    fn q(index: u32, event: ProjectReadQueryEvent) -> ProjectReadEvent {
        ProjectReadEvent::Query { index, event }
    }
    fn shapes(event: ProjectReadShapeEvent) -> ProjectReadQueryEvent {
        ProjectReadQueryEvent::Shapes(event)
    }
    fn nodes(event: ProjectReadNodeEvent) -> ProjectReadQueryEvent {
        ProjectReadQueryEvent::Nodes(event)
    }
    fn resources(event: ProjectReadResourceEvent) -> ProjectReadQueryEvent {
        ProjectReadQueryEvent::Resources(event)
    }

    // ---- Baseline: a full read applied progressively to a fresh view ----

    /// Build the event stream for one full read (shapes + nodes/slots +
    /// resources) so tests can drive it through the applier and assert on the
    /// resulting [`ProjectView`].
    fn baseline_events() -> Vec<ProjectReadEvent> {
        let registry = value_registry(&["shape.a", "shape.b"], 1);
        let shape_a = value_shape_id("shape.a");
        let entry_a = registry.entry(&shape_a).unwrap().clone();
        let entry_b = registry.entry(&value_shape_id("shape.b")).unwrap().clone();

        let events = vec![
            begin(1),
            // Query 0: shapes
            q(
                0,
                shapes(ProjectReadShapeEvent::Begin {
                    level: ReadLevel::Detail,
                    ids_revision: Revision::new(1),
                }),
            ),
            q(
                0,
                shapes(ProjectReadShapeEvent::Entry {
                    id: shape_a,
                    entry: entry_a,
                }),
            ),
            q(
                0,
                shapes(ProjectReadShapeEvent::Entry {
                    id: value_shape_id("shape.b"),
                    entry: entry_b,
                }),
            ),
            q(
                0,
                shapes(ProjectReadShapeEvent::Membership {
                    ids: vec![value_shape_id("shape.a"), value_shape_id("shape.b")],
                }),
            ),
            q(0, shapes(ProjectReadShapeEvent::End)),
            // Query 1: nodes
            q(
                1,
                nodes(ProjectReadNodeEvent::Begin {
                    level: ReadLevel::Detail,
                }),
            ),
            q(
                1,
                nodes(ProjectReadNodeEvent::TreeDeltas {
                    deltas: vec![
                        created_node(0, None, "/root.show", vec![NodeId::new(1)]),
                        created_node(1, Some(NodeId::new(0)), "/root.show/child.vis", vec![]),
                    ],
                }),
            ),
            q(
                1,
                nodes(ProjectReadNodeEvent::SlotRoot(value_root(
                    &registry,
                    "node.1.def",
                    shape_a,
                    1.0,
                ))),
            ),
            q(
                1,
                nodes(ProjectReadNodeEvent::SlotRoot(value_root(
                    &registry,
                    "node.1.state",
                    shape_a,
                    2.0,
                ))),
            ),
            q(1, nodes(ProjectReadNodeEvent::End)),
            // Query 2: resources
            q(
                2,
                resources(ProjectReadResourceEvent::Begin {
                    level: ReadLevel::Summary,
                }),
            ),
            q(
                2,
                resources(ProjectReadResourceEvent::Summary(buffer_summary(1, 1))),
            ),
            q(
                2,
                resources(ProjectReadResourceEvent::Summary(buffer_summary(2, 1))),
            ),
            q(
                2,
                resources(ProjectReadResourceEvent::Membership {
                    refs: vec![
                        ResourceRef::runtime_buffer(RuntimeBufferId::new(1)),
                        ResourceRef::runtime_buffer(RuntimeBufferId::new(2)),
                    ],
                }),
            ),
            q(2, resources(ProjectReadResourceEvent::End)),
            end(1),
        ];

        events
    }

    #[test]
    fn full_stream_applies_to_view() {
        let events = baseline_events();

        let mut streamed = ProjectView::new();
        let revision = apply_stream(&mut streamed, events).unwrap();
        assert_eq!(revision, Revision::new(1));

        // Progressive apply reaches the expected concrete view state.
        assert_eq!(streamed.revision, Revision::new(1));
        assert_eq!(streamed.resource_cache.summary_count(), 2);
        assert_eq!(f32_root_value(&streamed, "node.1.def"), 1.0);
        assert_eq!(f32_root_value(&streamed, "node.1.state"), 2.0);
        assert!(streamed.tree.get(NodeId::new(1)).is_some());
        assert!(
            streamed
                .slots
                .registry
                .entry(&value_shape_id("shape.a"))
                .is_some()
        );
        assert!(
            streamed
                .slots
                .registry
                .entry(&value_shape_id("shape.b"))
                .is_some()
        );
    }

    // ---- Gated stream onto a populated view (M5 vocabulary) ----

    fn populated_view() -> ProjectView {
        let events = baseline_events();
        let mut view = ProjectView::new();
        apply_stream(&mut view, events).unwrap();
        view
    }

    #[test]
    fn gated_stream_applies_onto_populated_view() {
        let mut view = populated_view();
        assert_eq!(view.resource_cache.summary_count(), 2);

        // Gated read at revision 6: shape.a re-stamped, shape.b removed (absent
        // from membership); node.1.state changed; buffer 2 removed.
        let registry_state = value_registry(&["shape.a"], 1);
        let shape_a = value_shape_id("shape.a");
        let changed = value_registry(&["shape.a"], 6);
        let changed_entry = changed.entry(&shape_a).unwrap().clone();

        let events = vec![
            begin(6),
            q(
                0,
                shapes(ProjectReadShapeEvent::Begin {
                    level: ReadLevel::Detail,
                    ids_revision: Revision::new(6),
                }),
            ),
            q(
                0,
                shapes(ProjectReadShapeEvent::Entry {
                    id: shape_a,
                    entry: changed_entry,
                }),
            ),
            q(
                0,
                shapes(ProjectReadShapeEvent::Membership {
                    ids: vec![value_shape_id("shape.a")],
                }),
            ),
            q(0, shapes(ProjectReadShapeEvent::End)),
            q(
                1,
                nodes(ProjectReadNodeEvent::Begin {
                    level: ReadLevel::Detail,
                }),
            ),
            q(
                1,
                nodes(ProjectReadNodeEvent::SlotRoot(value_root(
                    &registry_state,
                    "node.1.state",
                    shape_a,
                    42.0,
                ))),
            ),
            q(1, nodes(ProjectReadNodeEvent::End)),
            q(
                2,
                resources(ProjectReadResourceEvent::Begin {
                    level: ReadLevel::Summary,
                }),
            ),
            q(
                2,
                resources(ProjectReadResourceEvent::Summary(buffer_summary(1, 6))),
            ),
            q(
                2,
                resources(ProjectReadResourceEvent::Membership {
                    refs: vec![ResourceRef::runtime_buffer(RuntimeBufferId::new(1))],
                }),
            ),
            q(2, resources(ProjectReadResourceEvent::End)),
            end(6),
        ];

        apply_stream(&mut view, events).unwrap();

        // shape.b pruned, shape.a retained; node.1.def retained, node.1.state updated.
        assert!(
            view.slots
                .registry
                .entry(&value_shape_id("shape.b"))
                .is_none()
        );
        assert!(view.slots.registry.entry(&shape_a).is_some());
        assert_eq!(f32_root_value(&view, "node.1.def"), 1.0);
        assert_eq!(f32_root_value(&view, "node.1.state"), 42.0);
        // buffer 2 pruned, buffer 1 updated.
        assert!(
            view.resource_cache
                .summary(ResourceRef::runtime_buffer(RuntimeBufferId::new(2)))
                .is_none()
        );
        assert_eq!(
            view.resource_cache
                .summary(ResourceRef::runtime_buffer(RuntimeBufferId::new(1)))
                .map(|s| s.revision),
            Some(Revision::new(6))
        );
        assert_eq!(view.revision, Revision::new(6));
    }

    // ---- Membership pruning of removed nodes ----

    #[test]
    fn removed_node_roots_are_dropped() {
        // Start with two children (1 & 2), then a stream that removes node 2 via
        // ChildrenChanged must drop node.2.* roots.
        let registry = value_registry(&["shape.a"], 1);
        let shape_a = value_shape_id("shape.a");
        let entry_a = registry.entry(&shape_a).unwrap().clone();
        let mut view = ProjectView::new();
        apply_stream(
            &mut view,
            vec![
                begin(1),
                q(
                    0,
                    shapes(ProjectReadShapeEvent::Begin {
                        level: ReadLevel::Detail,
                        ids_revision: Revision::new(1),
                    }),
                ),
                q(
                    0,
                    shapes(ProjectReadShapeEvent::Entry {
                        id: shape_a,
                        entry: entry_a,
                    }),
                ),
                q(0, shapes(ProjectReadShapeEvent::End)),
                q(
                    1,
                    nodes(ProjectReadNodeEvent::Begin {
                        level: ReadLevel::Detail,
                    }),
                ),
                q(
                    1,
                    nodes(ProjectReadNodeEvent::TreeDeltas {
                        deltas: vec![
                            created_node(
                                0,
                                None,
                                "/root.show",
                                vec![NodeId::new(1), NodeId::new(2)],
                            ),
                            created_node(1, Some(NodeId::new(0)), "/root.show/a.vis", vec![]),
                            created_node(2, Some(NodeId::new(0)), "/root.show/b.vis", vec![]),
                        ],
                    }),
                ),
                q(
                    1,
                    nodes(ProjectReadNodeEvent::SlotRoot(value_root(
                        &registry,
                        "node.2.def",
                        shape_a,
                        9.0,
                    ))),
                ),
                q(1, nodes(ProjectReadNodeEvent::End)),
                end(1),
            ],
        )
        .unwrap();
        assert_eq!(f32_root_value(&view, "node.2.def"), 9.0);

        // Now remove node 2.
        apply_stream(
            &mut view,
            vec![
                begin(2),
                q(
                    0,
                    nodes(ProjectReadNodeEvent::Begin {
                        level: ReadLevel::Detail,
                    }),
                ),
                q(
                    0,
                    nodes(ProjectReadNodeEvent::TreeDeltas {
                        deltas: vec![WireTreeDelta::ChildrenChanged {
                            id: NodeId::new(0),
                            children: vec![NodeId::new(1)],
                            children_ver: Revision::new(2),
                        }],
                    }),
                ),
                q(0, nodes(ProjectReadNodeEvent::End)),
                end(2),
            ],
        )
        .unwrap();

        assert!(view.slots.roots.get("node.2.def").is_none());
        assert!(view.tree.get(NodeId::new(2)).is_none());
        assert!(view.tree.get(NodeId::new(1)).is_some());
    }

    // ---- Runtime replace ----

    #[test]
    fn runtime_event_replaces_view_runtime() {
        let mut view = ProjectView::new();
        let runtime = lpc_wire::RuntimeReadResult {
            project: lpc_wire::ProjectRuntimeStatus {
                revision: Revision::new(5),
                frame_num: 42,
                frame_delta_ms: 16,
                frame_total_ms: 17,
                demand_root_count: 2,
                runtime_buffer_count: 3,
            },
            server: None,
        };
        apply_stream(
            &mut view,
            vec![
                begin(5),
                q(0, ProjectReadQueryEvent::Runtime(runtime)),
                end(5),
            ],
        )
        .unwrap();
        assert_eq!(view.runtime.as_ref().unwrap().project.frame_num, 42);
        assert_eq!(view.revision, Revision::new(5));
    }

    // ---- Chunked runtime-buffer payload reassembly ----

    #[test]
    fn chunked_payload_reassembles() {
        let resource_ref = ResourceRef::runtime_buffer(RuntimeBufferId::new(9));
        let mut view = ProjectView::new();
        apply_stream(
            &mut view,
            vec![
                begin(7),
                q(
                    0,
                    resources(ProjectReadResourceEvent::Begin {
                        level: ReadLevel::Detail,
                    }),
                ),
                q(
                    0,
                    resources(ProjectReadResourceEvent::Summary(buffer_summary(9, 7))),
                ),
                q(
                    0,
                    resources(ProjectReadResourceEvent::RuntimeBufferPayloadBegin {
                        resource_ref,
                        revision: Revision::new(5),
                        metadata: WireRuntimeBufferMetadataPayload::Raw,
                        byte_length: 4,
                    }),
                ),
                q(
                    0,
                    resources(ProjectReadResourceEvent::RuntimeBufferPayloadBytes {
                        resource_ref,
                        offset: 0,
                        bytes: vec![1, 2],
                    }),
                ),
                q(
                    0,
                    resources(ProjectReadResourceEvent::RuntimeBufferPayloadBytes {
                        resource_ref,
                        offset: 2,
                        bytes: vec![3, 4],
                    }),
                ),
                q(
                    0,
                    resources(ProjectReadResourceEvent::RuntimeBufferPayloadEnd { resource_ref }),
                ),
                q(0, resources(ProjectReadResourceEvent::End)),
                end(7),
            ],
        )
        .unwrap();

        assert_eq!(
            view.resource_cache.runtime_buffer_bytes(resource_ref),
            Some([1u8, 2, 3, 4].as_slice())
        );
    }

    #[test]
    fn chunked_payload_offset_gap_errors() {
        let resource_ref = ResourceRef::runtime_buffer(RuntimeBufferId::new(9));
        let mut view = ProjectView::new();
        let mut applier = ProjectReadApplier::new(&mut view);
        applier.apply(begin(7)).unwrap();
        applier
            .apply(q(
                0,
                resources(ProjectReadResourceEvent::Begin {
                    level: ReadLevel::Detail,
                }),
            ))
            .unwrap();
        applier
            .apply(q(
                0,
                resources(ProjectReadResourceEvent::RuntimeBufferPayloadBegin {
                    resource_ref,
                    revision: Revision::new(5),
                    metadata: WireRuntimeBufferMetadataPayload::Raw,
                    byte_length: 4,
                }),
            ))
            .unwrap();
        applier
            .apply(q(
                0,
                resources(ProjectReadResourceEvent::RuntimeBufferPayloadBytes {
                    resource_ref,
                    offset: 0,
                    bytes: vec![1, 2],
                }),
            ))
            .unwrap();
        // Gap: next offset should be 2, send 3.
        let err = applier
            .apply(q(
                0,
                resources(ProjectReadResourceEvent::RuntimeBufferPayloadBytes {
                    resource_ref,
                    offset: 3,
                    bytes: vec![4],
                }),
            ))
            .unwrap_err();
        assert!(matches!(err, ProjectReadApplyStreamError::Protocol(_)));
        assert!(err.to_string().contains("expected offset 2"));
    }

    // ---- Protocol invariant errors ----

    #[test]
    fn begin_twice_errors() {
        let mut view = ProjectView::new();
        let mut applier = ProjectReadApplier::new(&mut view);
        applier.apply(begin(1)).unwrap();
        let err = applier.apply(begin(1)).unwrap_err();
        assert!(err.to_string().contains("began twice"));
    }

    #[test]
    fn event_before_begin_errors() {
        let mut view = ProjectView::new();
        let mut applier = ProjectReadApplier::new(&mut view);
        let err = applier
            .apply(q(
                0,
                nodes(ProjectReadNodeEvent::Begin {
                    level: ReadLevel::Detail,
                }),
            ))
            .unwrap_err();
        assert!(err.to_string().contains("before begin"));
    }

    #[test]
    fn end_revision_mismatch_errors() {
        let mut view = ProjectView::new();
        let mut applier = ProjectReadApplier::new(&mut view);
        applier.apply(begin(3)).unwrap();
        let err = applier.apply(end(4)).unwrap_err();
        assert!(err.to_string().contains("did not match begin revision"));
        // The view revision must not have advanced on the failed stream.
        assert_eq!(view.revision, Revision::default());
    }

    #[test]
    fn double_completion_errors() {
        let mut view = ProjectView::new();
        let mut applier = ProjectReadApplier::new(&mut view);
        applier.apply(begin(1)).unwrap();
        assert_eq!(
            applier.apply(end(1)).unwrap(),
            ApplyStatus::Complete {
                revision: Revision::new(1)
            }
        );
        let err = applier.apply(end(1)).unwrap_err();
        assert!(err.to_string().contains("already complete"));
    }

    #[test]
    fn remote_error_is_terminal() {
        let mut view = ProjectView::new();
        let mut applier = ProjectReadApplier::new(&mut view);
        applier.apply(begin(1)).unwrap();
        let err = applier
            .apply(ProjectReadEvent::Error {
                message: String::from("bad read"),
            })
            .unwrap_err();
        assert_eq!(
            err,
            ProjectReadApplyStreamError::Remote(String::from("bad read"))
        );
        // Terminal: nothing may follow.
        let err2 = applier.apply(end(1)).unwrap_err();
        assert!(err2.to_string().contains("already complete"));
    }

    #[test]
    fn non_contiguous_query_index_errors() {
        // Query index 1 present without index 0 -> contiguity failure at finish.
        let mut view = ProjectView::new();
        let err = apply_stream(
            &mut view,
            vec![
                begin(1),
                q(
                    1,
                    nodes(ProjectReadNodeEvent::Begin {
                        level: ReadLevel::Detail,
                    }),
                ),
                q(1, nodes(ProjectReadNodeEvent::End)),
                end(1),
            ],
        )
        .unwrap_err();
        assert!(err.to_string().contains("missing query result index 0"));
    }

    #[test]
    fn query_kind_mismatch_errors() {
        let mut view = ProjectView::new();
        let mut applier = ProjectReadApplier::new(&mut view);
        applier.apply(begin(1)).unwrap();
        applier
            .apply(q(
                0,
                nodes(ProjectReadNodeEvent::Begin {
                    level: ReadLevel::Detail,
                }),
            ))
            .unwrap();
        let err = applier
            .apply(q(
                0,
                shapes(ProjectReadShapeEvent::Begin {
                    level: ReadLevel::Detail,
                    ids_revision: Revision::new(1),
                }),
            ))
            .unwrap_err();
        assert!(err.to_string().contains("mixed"));
    }

    #[test]
    fn unended_query_errors_at_finish() {
        let mut view = ProjectView::new();
        let err = apply_stream(
            &mut view,
            vec![
                begin(1),
                q(
                    0,
                    nodes(ProjectReadNodeEvent::Begin {
                        level: ReadLevel::Detail,
                    }),
                ),
                // No node End before stream End.
                end(1),
            ],
        )
        .unwrap_err();
        assert!(err.to_string().contains("did not end"));
    }

    #[test]
    fn runtime_emitted_twice_errors() {
        let runtime = || lpc_wire::RuntimeReadResult {
            project: lpc_wire::ProjectRuntimeStatus {
                revision: Revision::new(1),
                frame_num: 1,
                frame_delta_ms: 1,
                frame_total_ms: 1,
                demand_root_count: 0,
                runtime_buffer_count: 0,
            },
            server: None,
        };
        let mut view = ProjectView::new();
        let mut applier = ProjectReadApplier::new(&mut view);
        applier.apply(begin(1)).unwrap();
        applier
            .apply(q(0, ProjectReadQueryEvent::Runtime(runtime())))
            .unwrap();
        // A runtime index completes on its single event (no End), so a second
        // runtime for the same index is rejected as emitted-after-end — the
        // same "runtime at most once per index" invariant the collector's
        // `runtime emitted twice` guard enforces.
        let err = applier
            .apply(q(0, ProjectReadQueryEvent::Runtime(runtime())))
            .unwrap_err();
        assert!(matches!(err, ProjectReadApplyStreamError::Protocol(_)));
        assert!(err.to_string().contains("emitted after its end"));
    }

    #[test]
    fn duplicate_probe_index_errors() {
        use lpc_wire::{ProjectProbeResult, RenderProductProbeResult};
        let probe = || {
            ProjectReadProbeEvent::Result(ProjectProbeResult::RenderProduct(
                RenderProductProbeResult::Unsupported {
                    product: lpc_model::VisualProduct::new(lpc_model::NodeId::new(1), 0),
                    reason: String::from("unsupported"),
                },
            ))
        };
        let mut view = ProjectView::new();
        let mut applier = ProjectReadApplier::new(&mut view);
        applier.apply(begin(1)).unwrap();
        applier
            .apply(ProjectReadEvent::Probe {
                index: 0,
                event: probe(),
            })
            .unwrap();
        let err = applier
            .apply(ProjectReadEvent::Probe {
                index: 0,
                event: probe(),
            })
            .unwrap_err();
        assert!(err.to_string().contains("probe 0 emitted twice"));
    }

    // ---- Probe result exposure + chunk reassembly ----

    fn render_header() -> ProjectProbeResultHeader {
        use lpc_wire::RenderProductProbeResultHeader;
        ProjectProbeResultHeader::RenderProduct(RenderProductProbeResultHeader {
            product: lpc_model::VisualProduct::new(lpc_model::NodeId::new(3), 1),
            revision: Revision::new(2),
            width: 2,
            height: 1,
            format: lpc_wire::WireTextureFormat::Rgba16,
        })
    }

    #[test]
    fn whole_probe_result_is_exposed_after_complete() {
        use lpc_wire::{ProjectProbeResult, RenderProductProbeResult};
        let result = ProjectProbeResult::RenderProduct(RenderProductProbeResult::Unsupported {
            product: lpc_model::VisualProduct::new(lpc_model::NodeId::new(1), 0),
            reason: String::from("nope"),
        });
        let mut view = ProjectView::new();
        let mut applier = ProjectReadApplier::new(&mut view);
        applier.apply(begin(1)).unwrap();
        applier
            .apply(ProjectReadEvent::Probe {
                index: 0,
                event: ProjectReadProbeEvent::Result(result.clone()),
            })
            .unwrap();
        assert_eq!(
            applier.apply(end(1)).unwrap(),
            ApplyStatus::Complete {
                revision: Revision::new(1)
            }
        );
        assert_eq!(applier.take_completed_probe_results(), vec![result]);
    }

    #[test]
    fn chunked_probe_result_reassembles_to_whole_result() {
        // A chunked texture probe reassembles byte-identically to what the
        // whole-`Result` path would have carried.
        let bytes = vec![10u8, 20, 30, 40];
        let expected = render_header().into_result(bytes.clone());

        let mut view = ProjectView::new();
        let mut applier = ProjectReadApplier::new(&mut view);
        applier.apply(begin(9)).unwrap();
        applier
            .apply(ProjectReadEvent::Probe {
                index: 0,
                event: ProjectReadProbeEvent::ResultBegin {
                    byte_length: 4,
                    header: render_header(),
                },
            })
            .unwrap();
        applier
            .apply(ProjectReadEvent::Probe {
                index: 0,
                event: ProjectReadProbeEvent::ResultBytes {
                    offset: 0,
                    bytes: vec![10, 20],
                },
            })
            .unwrap();
        applier
            .apply(ProjectReadEvent::Probe {
                index: 0,
                event: ProjectReadProbeEvent::ResultBytes {
                    offset: 2,
                    bytes: vec![30, 40],
                },
            })
            .unwrap();
        applier
            .apply(ProjectReadEvent::Probe {
                index: 0,
                event: ProjectReadProbeEvent::ResultEnd,
            })
            .unwrap();
        applier.apply(end(9)).unwrap();

        assert_eq!(applier.take_completed_probe_results(), vec![expected]);
    }

    #[test]
    fn chunked_probe_offset_gap_errors() {
        let mut view = ProjectView::new();
        let mut applier = ProjectReadApplier::new(&mut view);
        applier.apply(begin(1)).unwrap();
        applier
            .apply(ProjectReadEvent::Probe {
                index: 0,
                event: ProjectReadProbeEvent::ResultBegin {
                    byte_length: 4,
                    header: render_header(),
                },
            })
            .unwrap();
        applier
            .apply(ProjectReadEvent::Probe {
                index: 0,
                event: ProjectReadProbeEvent::ResultBytes {
                    offset: 0,
                    bytes: vec![1, 2],
                },
            })
            .unwrap();
        // Gap: expected offset 2, send 3.
        let err = applier
            .apply(ProjectReadEvent::Probe {
                index: 0,
                event: ProjectReadProbeEvent::ResultBytes {
                    offset: 3,
                    bytes: vec![4],
                },
            })
            .unwrap_err();
        assert!(matches!(err, ProjectReadApplyStreamError::Protocol(_)));
        assert!(err.to_string().contains("expected offset 2"));
    }

    #[test]
    fn chunked_probe_length_mismatch_errors() {
        let mut view = ProjectView::new();
        let mut applier = ProjectReadApplier::new(&mut view);
        applier.apply(begin(1)).unwrap();
        applier
            .apply(ProjectReadEvent::Probe {
                index: 0,
                event: ProjectReadProbeEvent::ResultBegin {
                    byte_length: 4,
                    header: render_header(),
                },
            })
            .unwrap();
        applier
            .apply(ProjectReadEvent::Probe {
                index: 0,
                event: ProjectReadProbeEvent::ResultBytes {
                    offset: 0,
                    bytes: vec![1, 2],
                },
            })
            .unwrap();
        // End early: only 2 of 4 bytes received.
        let err = applier
            .apply(ProjectReadEvent::Probe {
                index: 0,
                event: ProjectReadProbeEvent::ResultEnd,
            })
            .unwrap_err();
        assert!(err.to_string().contains("expected 4 bytes, got 2"));
    }

    #[test]
    fn unended_chunked_probe_errors_at_finish() {
        let mut view = ProjectView::new();
        let mut applier = ProjectReadApplier::new(&mut view);
        applier.apply(begin(1)).unwrap();
        applier
            .apply(ProjectReadEvent::Probe {
                index: 0,
                event: ProjectReadProbeEvent::ResultBegin {
                    byte_length: 0,
                    header: render_header(),
                },
            })
            .unwrap();
        // Stream ends without ResultEnd for probe 0.
        let err = applier.apply(end(1)).unwrap_err();
        assert!(err.to_string().contains("probe 0 result did not end"));
    }
}
