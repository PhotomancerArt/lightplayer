//! Compatibility collector for project-read event streams.
//!
//! The wire protocol now treats project reads as an event stream. This
//! collector rebuilds the aggregate [`ProjectReadResponse`] shape while Studio
//! and other clients still consume aggregate project reads.
//!
//! New low-memory servers should stream [`ProjectReadEvent`] values and let the
//! transport batch them into frames. Client-side code can use this collector as
//! a compatibility adapter, preserving the old "await one project read response"
//! ergonomics without requiring firmware to allocate that response.

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use lp_collection::VecMap;
use lpc_model::{ResourceRef, Revision, SlotShapeEntry, SlotShapeId, SlotShapeRegistrySnapshot};

use crate::project::{
    WireResourceSummary, WireRuntimeBufferMetadataPayload, WireRuntimeBufferPayload,
};
use crate::slot::{WireSlotRootSnapshot, WireSlotRootsSnapshot};
use crate::tree::WireTreeDelta;

use super::{
    ProjectProbeResult, ProjectReadEvent, ProjectReadFrame, ProjectReadNodeEvent,
    ProjectReadProbeEvent, ProjectReadQueryEvent, ProjectReadResourceEvent, ProjectReadResponse,
    ProjectReadResult, ProjectReadShapeEvent, ReadLevel, ResourceReadResult, RuntimeReadResult,
    ShapeReadResult,
};

/// Result of applying one project-read frame to a collector.
///
/// `Complete` is returned exactly once, when an accepted frame carries the
/// stream-ending event and the aggregate response can be built.
#[derive(Debug, Clone, PartialEq)]
pub enum ProjectReadCollectStatus {
    Continue,
    Complete(ProjectReadResponse),
}

/// Error while collecting a project-read event stream.
///
/// `Remote` is a server-side read failure carried by
/// [`ProjectReadEvent::Error`]. `Protocol` means the event stream itself was
/// malformed, such as skipped frame sequences or events before `Begin`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectReadCollectError {
    Remote(String),
    Protocol(String),
}

impl core::fmt::Display for ProjectReadCollectError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Remote(message) => write!(f, "project read failed: {message}"),
            Self::Protocol(message) => write!(f, "project read stream protocol error: {message}"),
        }
    }
}

impl core::error::Error for ProjectReadCollectError {}

/// Collects project-read frames into one compatibility response.
///
/// The collector enforces frame sequence order and basic stream structure. It
/// does not know the original request shape; query and probe indexes are
/// collected sparsely and then compacted in index order when the stream ends.
#[derive(Debug, Default)]
pub struct ProjectReadCollector {
    next_sequence: u32,
    revision: Option<Revision>,
    queries: BTreeMap<u32, QueryCollectState>,
    probes: BTreeMap<u32, ProjectProbeResult>,
    complete: bool,
}

impl ProjectReadCollector {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn accept_frame(
        &mut self,
        frame: ProjectReadFrame,
    ) -> Result<ProjectReadCollectStatus, ProjectReadCollectError> {
        if self.complete {
            return Err(protocol("project read stream is already complete"));
        }
        if frame.sequence != self.next_sequence {
            return Err(protocol(format!(
                "expected project read frame {}, got {}",
                self.next_sequence, frame.sequence
            )));
        }
        self.next_sequence = self.next_sequence.saturating_add(1);

        for event in frame.events {
            if let Some(response) = self.accept_event(event)? {
                return Ok(ProjectReadCollectStatus::Complete(response));
            }
        }

        Ok(ProjectReadCollectStatus::Continue)
    }

    pub fn accept_event(
        &mut self,
        event: ProjectReadEvent,
    ) -> Result<Option<ProjectReadResponse>, ProjectReadCollectError> {
        match event {
            ProjectReadEvent::Begin { revision } => {
                if self.revision.replace(revision).is_some() {
                    return Err(protocol("project read stream began twice"));
                }
            }
            ProjectReadEvent::Query { index, event } => {
                self.ensure_started()?;
                self.accept_query_event(index, event)?;
            }
            ProjectReadEvent::Probe { index, event } => {
                self.ensure_started()?;
                self.accept_probe_event(index, event)?;
            }
            ProjectReadEvent::End { revision } => {
                self.ensure_started()?;
                self.complete = true;
                return self.finish(revision).map(Some);
            }
            ProjectReadEvent::Error { message } => {
                self.complete = true;
                return Err(ProjectReadCollectError::Remote(message));
            }
        }
        Ok(None)
    }

    fn accept_query_event(
        &mut self,
        index: u32,
        event: ProjectReadQueryEvent,
    ) -> Result<(), ProjectReadCollectError> {
        match event {
            ProjectReadQueryEvent::Shapes(event) => {
                let state = self.query_state(index, QueryKind::Shapes)?;
                state.accept_shape_event(event)
            }
            ProjectReadQueryEvent::Nodes(event) => {
                let state = self.query_state(index, QueryKind::Nodes)?;
                state.accept_node_event(event)
            }
            ProjectReadQueryEvent::Resources(event) => {
                let state = self.query_state(index, QueryKind::Resources)?;
                state.accept_resource_event(event)
            }
            ProjectReadQueryEvent::Runtime(runtime) => {
                let state = self.query_state(index, QueryKind::Runtime)?;
                let QueryCollectState::Runtime(runtime_state) = state else {
                    return Err(protocol("internal query kind mismatch"));
                };
                if runtime_state.replace(runtime).is_some() {
                    return Err(protocol(format!("query {index} runtime emitted twice")));
                }
                Ok(())
            }
        }
    }

    fn accept_probe_event(
        &mut self,
        index: u32,
        event: ProjectReadProbeEvent,
    ) -> Result<(), ProjectReadCollectError> {
        match event {
            ProjectReadProbeEvent::Result(result) => {
                if self.probes.insert(index, result).is_some() {
                    return Err(protocol(format!("probe {index} emitted twice")));
                }
                Ok(())
            }
        }
    }

    fn query_state(
        &mut self,
        index: u32,
        kind: QueryKind,
    ) -> Result<&mut QueryCollectState, ProjectReadCollectError> {
        if let Some(existing) = self.queries.get(&index)
            && existing.kind() != kind
        {
            return Err(protocol(format!(
                "query {index} mixed {:?} and {:?} events",
                existing.kind(),
                kind
            )));
        }
        Ok(self
            .queries
            .entry(index)
            .or_insert_with(|| kind.empty_state()))
    }

    fn ensure_started(&self) -> Result<(), ProjectReadCollectError> {
        if self.revision.is_none() {
            return Err(protocol("project read stream event arrived before begin"));
        }
        Ok(())
    }

    fn finish(
        &mut self,
        revision: Revision,
    ) -> Result<ProjectReadResponse, ProjectReadCollectError> {
        let begin_revision = self
            .revision
            .ok_or_else(|| protocol("project read stream ended before begin"))?;
        if begin_revision != revision {
            return Err(protocol(format!(
                "project read end revision {} did not match begin revision {}",
                revision.0, begin_revision.0
            )));
        }

        let results = collect_indexed_results(&self.queries)?;
        let probes = collect_indexed_probes(&self.probes)?;
        Ok(ProjectReadResponse {
            revision,
            results,
            probes,
        })
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum QueryKind {
    Shapes,
    Nodes,
    Resources,
    Runtime,
}

impl QueryKind {
    fn empty_state(self) -> QueryCollectState {
        match self {
            Self::Shapes => QueryCollectState::Shapes(ShapeCollectState::default()),
            Self::Nodes => QueryCollectState::Nodes(NodeCollectState::default()),
            Self::Resources => QueryCollectState::Resources(ResourceCollectState::default()),
            Self::Runtime => QueryCollectState::Runtime(None),
        }
    }
}

#[derive(Debug)]
enum QueryCollectState {
    Shapes(ShapeCollectState),
    Nodes(NodeCollectState),
    Resources(ResourceCollectState),
    Runtime(Option<RuntimeReadResult>),
}

impl QueryCollectState {
    fn kind(&self) -> QueryKind {
        match self {
            Self::Shapes(_) => QueryKind::Shapes,
            Self::Nodes(_) => QueryKind::Nodes,
            Self::Resources(_) => QueryKind::Resources,
            Self::Runtime(_) => QueryKind::Runtime,
        }
    }

    fn accept_shape_event(
        &mut self,
        event: ProjectReadShapeEvent,
    ) -> Result<(), ProjectReadCollectError> {
        let Self::Shapes(state) = self else {
            return Err(protocol("internal query kind mismatch"));
        };
        state.accept(event)
    }

    fn accept_node_event(
        &mut self,
        event: ProjectReadNodeEvent,
    ) -> Result<(), ProjectReadCollectError> {
        let Self::Nodes(state) = self else {
            return Err(protocol("internal query kind mismatch"));
        };
        state.accept(event)
    }

    fn accept_resource_event(
        &mut self,
        event: ProjectReadResourceEvent,
    ) -> Result<(), ProjectReadCollectError> {
        let Self::Resources(state) = self else {
            return Err(protocol("internal query kind mismatch"));
        };
        state.accept(event)
    }

    fn finish(&self) -> Result<ProjectReadResult, ProjectReadCollectError> {
        match self {
            Self::Shapes(state) => state.finish().map(ProjectReadResult::Shapes),
            Self::Nodes(state) => state.finish().map(ProjectReadResult::Nodes),
            Self::Resources(state) => state.finish().map(ProjectReadResult::Resources),
            Self::Runtime(runtime) => runtime
                .clone()
                .map(ProjectReadResult::Runtime)
                .ok_or_else(|| protocol("runtime query missing result")),
        }
    }
}

#[derive(Debug, Default)]
struct ShapeCollectState {
    level: Option<ReadLevel>,
    ids_revision: Option<Revision>,
    shapes: VecMap<SlotShapeId, SlotShapeEntry>,
    ended: bool,
}

impl ShapeCollectState {
    fn accept(&mut self, event: ProjectReadShapeEvent) -> Result<(), ProjectReadCollectError> {
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
                // Prune any collected entry whose id is not in the current
                // membership list. Harmless on a full stream (the list names
                // every id already collected); on a gated stream it drops shapes
                // that were removed since `since`. The collector is deleted in
                // M6; this is the minimal tolerance the contract requires.
                self.shapes.retain(|id, _| ids.contains(id));
            }
            ProjectReadShapeEvent::End => {
                self.ensure_open("shape end")?;
                self.ended = true;
            }
        }
        Ok(())
    }

    fn finish(&self) -> Result<ShapeReadResult, ProjectReadCollectError> {
        if !self.ended {
            return Err(protocol("shape query did not end"));
        }
        Ok(ShapeReadResult {
            level: self
                .level
                .ok_or_else(|| protocol("shape query missing begin"))?,
            registry: Some(SlotShapeRegistrySnapshot {
                ids_revision: self
                    .ids_revision
                    .ok_or_else(|| protocol("shape query missing ids revision"))?,
                shapes: self.shapes.clone(),
            }),
        })
    }

    fn ensure_open(&self, label: &str) -> Result<(), ProjectReadCollectError> {
        if self.level.is_none() {
            return Err(protocol(format!("{label} arrived before shape begin")));
        }
        if self.ended {
            return Err(protocol(format!("{label} arrived after shape end")));
        }
        Ok(())
    }
}

#[derive(Debug, Default)]
struct NodeCollectState {
    level: Option<ReadLevel>,
    deltas: Vec<WireTreeDelta>,
    roots: Vec<WireSlotRootSnapshot>,
    ended: bool,
}

impl NodeCollectState {
    fn accept(&mut self, event: ProjectReadNodeEvent) -> Result<(), ProjectReadCollectError> {
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
            }
        }
        Ok(())
    }

    fn finish(&self) -> Result<super::NodeReadResult, ProjectReadCollectError> {
        if !self.ended {
            return Err(protocol("node query did not end"));
        }
        Ok(super::NodeReadResult {
            level: self
                .level
                .ok_or_else(|| protocol("node query missing begin"))?,
            tree_deltas: self.deltas.clone(),
            slots: Some(WireSlotRootsSnapshot {
                roots: self.roots.clone(),
            }),
        })
    }

    fn ensure_open(&self, label: &str) -> Result<(), ProjectReadCollectError> {
        if self.level.is_none() {
            return Err(protocol(format!("{label} arrived before node begin")));
        }
        if self.ended {
            return Err(protocol(format!("{label} arrived after node end")));
        }
        Ok(())
    }
}

#[derive(Debug, Default)]
struct ResourceCollectState {
    level: Option<ReadLevel>,
    summaries: Vec<WireResourceSummary>,
    payloads: Vec<WireRuntimeBufferPayload>,
    membership: Option<Vec<ResourceRef>>,
    pending_payloads: BTreeMap<ResourceRef, PendingRuntimeBufferPayload>,
    ended: bool,
}

impl ResourceCollectState {
    fn accept(&mut self, event: ProjectReadResourceEvent) -> Result<(), ProjectReadCollectError> {
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
                self.ended = true;
            }
        }
        Ok(())
    }

    fn finish(&self) -> Result<ResourceReadResult, ProjectReadCollectError> {
        if !self.ended {
            return Err(protocol("resource query did not end"));
        }
        if !self.pending_payloads.is_empty() {
            return Err(protocol("resource query ended with pending payload chunks"));
        }
        Ok(ResourceReadResult {
            level: self
                .level
                .ok_or_else(|| protocol("resource query missing begin"))?,
            summaries: self.summaries.clone(),
            runtime_buffer_payloads: self.payloads.clone(),
            membership: self.membership.clone(),
        })
    }

    fn ensure_open(&self, label: &str) -> Result<(), ProjectReadCollectError> {
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

fn collect_indexed_results(
    queries: &BTreeMap<u32, QueryCollectState>,
) -> Result<Vec<ProjectReadResult>, ProjectReadCollectError> {
    let mut results = Vec::new();
    for (expected, (index, state)) in (0_u32..).zip(queries.iter()) {
        if *index != expected {
            return Err(protocol(format!(
                "missing query result index {expected}; next index was {index}"
            )));
        }
        results.push(state.finish()?);
    }
    Ok(results)
}

fn collect_indexed_probes(
    probes: &BTreeMap<u32, ProjectProbeResult>,
) -> Result<Vec<ProjectProbeResult>, ProjectReadCollectError> {
    let mut results = Vec::new();
    for (expected, (index, probe)) in (0_u32..).zip(probes.iter()) {
        if *index != expected {
            return Err(protocol(format!(
                "missing probe result index {expected}; next index was {index}"
            )));
        }
        results.push(probe.clone());
    }
    Ok(results)
}

fn protocol(message: impl Into<String>) -> ProjectReadCollectError {
    ProjectReadCollectError::Protocol(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ProjectRuntimeStatus;
    use alloc::string::ToString;
    use alloc::vec;
    use lpc_model::{LpType, ResourceRef, RuntimeBufferId, SlotShape, SlotShapeEntry, SlotShapeId};

    #[test]
    fn collects_complete_project_read_response() {
        let shape_id = SlotShapeId::from_static_name("TestShape");
        let response = collect(vec![ProjectReadFrame::new(
            0,
            vec![
                ProjectReadEvent::Begin {
                    revision: Revision::new(7),
                },
                ProjectReadEvent::Query {
                    index: 0,
                    event: ProjectReadQueryEvent::Shapes(ProjectReadShapeEvent::Begin {
                        level: ReadLevel::Detail,
                        ids_revision: Revision::new(3),
                    }),
                },
                ProjectReadEvent::Query {
                    index: 0,
                    event: ProjectReadQueryEvent::Shapes(ProjectReadShapeEvent::Entry {
                        id: shape_id,
                        entry: SlotShapeEntry::new(
                            Revision::new(3),
                            SlotShape::value(LpType::Bool),
                        ),
                    }),
                },
                ProjectReadEvent::Query {
                    index: 0,
                    event: ProjectReadQueryEvent::Shapes(ProjectReadShapeEvent::End),
                },
                ProjectReadEvent::Query {
                    index: 1,
                    event: ProjectReadQueryEvent::Runtime(runtime_result()),
                },
                ProjectReadEvent::End {
                    revision: Revision::new(7),
                },
            ],
        )])
        .unwrap();

        assert_eq!(response.revision, Revision::new(7));
        assert_eq!(response.results.len(), 2);
        let ProjectReadResult::Shapes(shapes) = &response.results[0] else {
            panic!("first result should be shapes");
        };
        assert_eq!(shapes.level, ReadLevel::Detail);
        assert_eq!(
            shapes
                .registry
                .as_ref()
                .expect("shape registry")
                .shapes
                .get(&shape_id)
                .expect("shape entry")
                .changed_at,
            Revision::new(3)
        );
        assert!(matches!(response.results[1], ProjectReadResult::Runtime(_)));
    }

    #[test]
    fn sequence_mismatch_errors() {
        let mut collector = ProjectReadCollector::new();

        let error = collector
            .accept_frame(ProjectReadFrame::new(1, Vec::new()))
            .unwrap_err();

        assert!(error.to_string().contains("expected project read frame 0"));
    }

    #[test]
    fn remote_error_is_terminal_error() {
        let mut collector = ProjectReadCollector::new();

        let error = collector
            .accept_frame(ProjectReadFrame::new(
                0,
                vec![ProjectReadEvent::Error {
                    message: "bad read".into(),
                }],
            ))
            .unwrap_err();

        assert_eq!(error, ProjectReadCollectError::Remote("bad read".into()));
    }

    #[test]
    fn collects_runtime_buffer_payload_chunks() {
        let resource_ref = ResourceRef::runtime_buffer(RuntimeBufferId::new(9));
        let response = collect(vec![ProjectReadFrame::new(
            0,
            vec![
                ProjectReadEvent::Begin {
                    revision: Revision::new(7),
                },
                ProjectReadEvent::Query {
                    index: 0,
                    event: ProjectReadQueryEvent::Resources(ProjectReadResourceEvent::Begin {
                        level: ReadLevel::Detail,
                    }),
                },
                ProjectReadEvent::Query {
                    index: 0,
                    event: ProjectReadQueryEvent::Resources(
                        ProjectReadResourceEvent::RuntimeBufferPayloadBegin {
                            resource_ref,
                            revision: Revision::new(5),
                            metadata: WireRuntimeBufferMetadataPayload::Raw,
                            byte_length: 4,
                        },
                    ),
                },
                ProjectReadEvent::Query {
                    index: 0,
                    event: ProjectReadQueryEvent::Resources(
                        ProjectReadResourceEvent::RuntimeBufferPayloadBytes {
                            resource_ref,
                            offset: 0,
                            bytes: vec![1, 2],
                        },
                    ),
                },
                ProjectReadEvent::Query {
                    index: 0,
                    event: ProjectReadQueryEvent::Resources(
                        ProjectReadResourceEvent::RuntimeBufferPayloadBytes {
                            resource_ref,
                            offset: 2,
                            bytes: vec![3, 4],
                        },
                    ),
                },
                ProjectReadEvent::Query {
                    index: 0,
                    event: ProjectReadQueryEvent::Resources(
                        ProjectReadResourceEvent::RuntimeBufferPayloadEnd { resource_ref },
                    ),
                },
                ProjectReadEvent::Query {
                    index: 0,
                    event: ProjectReadQueryEvent::Resources(ProjectReadResourceEvent::End),
                },
                ProjectReadEvent::End {
                    revision: Revision::new(7),
                },
            ],
        )])
        .unwrap();

        let ProjectReadResult::Resources(resources) = &response.results[0] else {
            panic!("first result should be resources");
        };
        assert_eq!(resources.runtime_buffer_payloads.len(), 1);
        assert_eq!(resources.runtime_buffer_payloads[0].bytes, vec![1, 2, 3, 4]);
    }

    #[test]
    fn frame_round_trips() {
        let frame = ProjectReadFrame::new(
            3,
            vec![
                ProjectReadEvent::Begin {
                    revision: Revision::new(7),
                },
                ProjectReadEvent::End {
                    revision: Revision::new(7),
                },
            ],
        );

        let json = serde_json::to_string(&frame).unwrap();
        let decoded: ProjectReadFrame = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, frame);
    }

    fn collect(
        frames: Vec<ProjectReadFrame>,
    ) -> Result<ProjectReadResponse, ProjectReadCollectError> {
        let mut collector = ProjectReadCollector::new();
        for frame in frames {
            if let ProjectReadCollectStatus::Complete(response) = collector.accept_frame(frame)? {
                return Ok(response);
            }
        }
        Err(ProjectReadCollectError::Protocol(
            "test stream did not complete".into(),
        ))
    }

    fn runtime_result() -> RuntimeReadResult {
        RuntimeReadResult {
            project: ProjectRuntimeStatus {
                revision: Revision::new(7),
                frame_num: 11,
                frame_delta_ms: 16,
                frame_total_ms: 176,
                demand_root_count: 2,
                runtime_buffer_count: 1,
            },
            server: None,
        }
    }
}
