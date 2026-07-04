//! Streaming project-read response/event writer for [`Engine`].

use alloc::string::String;
use lpc_registry::ProjectRegistry;
use lpc_shared::transport::ProjectReadEventSink;
use lpc_wire::{
    ProjectProbeRequest, ProjectProbeResult, ProjectReadEvent, ProjectReadNodeEvent,
    ProjectReadProbeEvent, ProjectReadQuery, ProjectReadQueryEvent, ProjectReadRequest,
    ProjectReadResourceEvent, ProjectReadShapeEvent, ServerRuntimeStatus,
};

use super::Engine;

pub struct EngineProjectReadSource<'a> {
    engine: &'a mut Engine,
    registry: &'a ProjectRegistry,
    server_status: Option<ServerRuntimeStatus>,
}

impl<'a> EngineProjectReadSource<'a> {
    pub fn new(engine: &'a mut Engine, registry: &'a ProjectRegistry) -> Self {
        Self {
            engine,
            registry,
            server_status: None,
        }
    }

    pub fn with_server_status(
        engine: &'a mut Engine,
        registry: &'a ProjectRegistry,
        server_status: Option<ServerRuntimeStatus>,
    ) -> Self {
        Self {
            engine,
            registry,
            server_status,
        }
    }

    pub async fn stream_project_read_events<S>(
        &mut self,
        request: ProjectReadRequest,
        sink: &mut S,
    ) -> Result<(), ProjectReadEventStreamError<S::Error>>
    where
        S: ProjectReadEventSink,
    {
        let revision = self.engine.revision();
        send_project_read_event(sink, ProjectReadEvent::Begin { revision }).await?;

        let since = request.since;
        for (index, query) in request.queries.into_iter().enumerate() {
            self.stream_query_events(index as u32, since, query, sink)
                .await?;
        }

        for (index, probe) in request.probes.into_iter().enumerate() {
            self.stream_probe_event(index as u32, probe, sink).await?;
        }

        send_project_read_event(sink, ProjectReadEvent::End { revision }).await
    }

    async fn stream_query_events<S>(
        &mut self,
        index: u32,
        since: Option<lpc_model::Revision>,
        query: ProjectReadQuery,
        sink: &mut S,
    ) -> Result<(), ProjectReadEventStreamError<S::Error>>
    where
        S: ProjectReadEventSink,
    {
        match query {
            ProjectReadQuery::Shapes(query) => {
                let result = self.engine.read_project_shapes(query, since);
                if let Some(registry) = result.registry {
                    let ids_revision = registry.ids_revision;
                    send_query_event(
                        sink,
                        index,
                        ProjectReadQueryEvent::Shapes(ProjectReadShapeEvent::Begin {
                            level: result.level,
                            ids_revision,
                        }),
                    )
                    .await?;
                    for (id, entry) in registry.shapes {
                        send_query_event(
                            sink,
                            index,
                            ProjectReadQueryEvent::Shapes(ProjectReadShapeEvent::Entry {
                                id,
                                entry,
                            }),
                        )
                        .await?;
                    }
                    // Membership sync (G7): when the id set changed after `since`,
                    // send the full current id list so the client can prune shapes
                    // that vanished from a gated stream. A `None`/`0` since is
                    // always older than any real `ids_revision`, so a fresh read
                    // still carries the confirming list.
                    if ids_revision > since.unwrap_or_default() {
                        let ids = self.engine.project_shape_membership_ids();
                        send_query_event(
                            sink,
                            index,
                            ProjectReadQueryEvent::Shapes(ProjectReadShapeEvent::Membership {
                                ids,
                            }),
                        )
                        .await?;
                    }
                } else {
                    send_query_event(
                        sink,
                        index,
                        ProjectReadQueryEvent::Shapes(ProjectReadShapeEvent::Begin {
                            level: result.level,
                            ids_revision: lpc_model::Revision::default(),
                        }),
                    )
                    .await?;
                }
                send_query_event(
                    sink,
                    index,
                    ProjectReadQueryEvent::Shapes(ProjectReadShapeEvent::End),
                )
                .await
            }
            ProjectReadQuery::Nodes(query) => {
                let result = self.engine.read_project_nodes(self.registry, since, query);
                send_query_event(
                    sink,
                    index,
                    ProjectReadQueryEvent::Nodes(ProjectReadNodeEvent::Begin {
                        level: result.level,
                    }),
                )
                .await?;
                if !result.tree_deltas.is_empty() {
                    send_query_event(
                        sink,
                        index,
                        ProjectReadQueryEvent::Nodes(ProjectReadNodeEvent::TreeDeltas {
                            deltas: result.tree_deltas,
                        }),
                    )
                    .await?;
                }
                if let Some(slots) = result.slots {
                    for root in slots.roots {
                        send_query_event(
                            sink,
                            index,
                            ProjectReadQueryEvent::Nodes(ProjectReadNodeEvent::SlotRoot(root)),
                        )
                        .await?;
                    }
                }
                send_query_event(
                    sink,
                    index,
                    ProjectReadQueryEvent::Nodes(ProjectReadNodeEvent::End),
                )
                .await
            }
            ProjectReadQuery::Resources(query) => {
                let result = self.engine.read_project_resources(since, query);
                send_query_event(
                    sink,
                    index,
                    ProjectReadQueryEvent::Resources(ProjectReadResourceEvent::Begin {
                        level: result.level,
                    }),
                )
                .await?;
                for summary in result.summaries {
                    send_query_event(
                        sink,
                        index,
                        ProjectReadQueryEvent::Resources(ProjectReadResourceEvent::Summary(
                            summary,
                        )),
                    )
                    .await?;
                }
                for payload in result.runtime_buffer_payloads {
                    stream_runtime_buffer_payload(index, payload, sink).await?;
                }
                // Membership rides after summaries/payloads and before `End`, only when the store's
                // id set changed since `since` (the engine returns `Some` exactly then).
                if let Some(refs) = result.membership {
                    send_query_event(
                        sink,
                        index,
                        ProjectReadQueryEvent::Resources(ProjectReadResourceEvent::Membership {
                            refs,
                        }),
                    )
                    .await?;
                }
                send_query_event(
                    sink,
                    index,
                    ProjectReadQueryEvent::Resources(ProjectReadResourceEvent::End),
                )
                .await
            }
            ProjectReadQuery::Runtime(query) => {
                let result = self
                    .engine
                    .read_project_runtime(query, self.server_status.clone());
                send_query_event(sink, index, ProjectReadQueryEvent::Runtime(result)).await
            }
        }
    }

    async fn stream_probe_event<S>(
        &mut self,
        index: u32,
        probe: ProjectProbeRequest,
        sink: &mut S,
    ) -> Result<(), ProjectReadEventStreamError<S::Error>>
    where
        S: ProjectReadEventSink,
    {
        let result = match probe {
            ProjectProbeRequest::RenderProduct(request) => ProjectProbeResult::RenderProduct(
                self.engine
                    .read_project_render_product_probe(self.registry, request),
            ),
            ProjectProbeRequest::ControlProduct(request) => ProjectProbeResult::ControlProduct(
                self.engine
                    .read_project_control_product_probe(self.registry, request),
            ),
            ProjectProbeRequest::ExplainSlot(request) => ProjectProbeResult::ExplainSlot(
                self.engine.read_project_explain_slot_probe(request),
            ),
        };
        send_project_read_event(
            sink,
            ProjectReadEvent::Probe {
                index,
                event: ProjectReadProbeEvent::Result(result),
            },
        )
        .await
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectReadEventStreamError<E> {
    Sink(E),
    Protocol(String),
}

async fn stream_runtime_buffer_payload<S>(
    index: u32,
    payload: lpc_wire::WireRuntimeBufferPayload,
    sink: &mut S,
) -> Result<(), ProjectReadEventStreamError<S::Error>>
where
    S: ProjectReadEventSink,
{
    // Derived from the frame budget in one place (`lpc-wire`) so a chunk's
    // base64 always fits an empty project-read frame; see
    // `PROJECT_READ_RUNTIME_CHUNK_BYTES` and its compile-time budget assertion.
    const RUNTIME_BUFFER_PAYLOAD_CHUNK_BYTES: usize = lpc_wire::PROJECT_READ_RUNTIME_CHUNK_BYTES;

    let lpc_wire::WireRuntimeBufferPayload {
        resource_ref,
        revision,
        metadata,
        bytes,
    } = payload;
    let byte_length = u32::try_from(bytes.len()).map_err(|_| {
        ProjectReadEventStreamError::Protocol("runtime buffer payload is too large".into())
    })?;
    send_query_event(
        sink,
        index,
        ProjectReadQueryEvent::Resources(ProjectReadResourceEvent::RuntimeBufferPayloadBegin {
            resource_ref,
            revision,
            metadata,
            byte_length,
        }),
    )
    .await?;
    for (chunk_index, chunk) in bytes.chunks(RUNTIME_BUFFER_PAYLOAD_CHUNK_BYTES).enumerate() {
        let offset =
            u32::try_from(chunk_index * RUNTIME_BUFFER_PAYLOAD_CHUNK_BYTES).map_err(|_| {
                ProjectReadEventStreamError::Protocol("runtime buffer offset overflow".into())
            })?;
        send_query_event(
            sink,
            index,
            ProjectReadQueryEvent::Resources(ProjectReadResourceEvent::RuntimeBufferPayloadBytes {
                resource_ref,
                offset,
                bytes: chunk.to_vec(),
            }),
        )
        .await?;
    }
    send_query_event(
        sink,
        index,
        ProjectReadQueryEvent::Resources(ProjectReadResourceEvent::RuntimeBufferPayloadEnd {
            resource_ref,
        }),
    )
    .await
}

async fn send_query_event<S>(
    sink: &mut S,
    index: u32,
    event: ProjectReadQueryEvent,
) -> Result<(), ProjectReadEventStreamError<S::Error>>
where
    S: ProjectReadEventSink,
{
    send_project_read_event(sink, ProjectReadEvent::Query { index, event }).await
}

async fn send_project_read_event<S>(
    sink: &mut S,
    event: ProjectReadEvent,
) -> Result<(), ProjectReadEventStreamError<S::Error>>
where
    S: ProjectReadEventSink,
{
    sink.send_project_read_event(event)
        .await
        .map_err(ProjectReadEventStreamError::Sink)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::boxed::Box;
    use alloc::vec;
    use alloc::vec::Vec;
    use core::future::Future;
    use core::pin::Pin;
    use core::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
    use lpc_model::{NodeDef, NodeName, Revision, TextureDef, TreePath, WithRevision};
    use lpc_wire::{
        NodeReadQuery, ProjectReadCollector, ProjectReadEvent, ProjectReadNodeEvent,
        ProjectReadResourceEvent, ProjectReadResponse, ProjectReadResult, ResourcePayloadRead,
        ResourceReadQuery, WireChildKind, WireSlotIndex,
    };

    use crate::engine::project_read_nodes::{node_def_root_name, node_state_root_name};
    use crate::engine::test_support::{EngineTestBuilder, output};
    use crate::node::test_placeholder_spine;
    use crate::nodes::TextureNode;
    use crate::resource::RuntimeBuffer;

    #[test]
    fn event_stream_matches_full_debug_response() {
        let mut h = EngineTestBuilder::new().output_node("output").build();
        let request = ProjectReadRequest::default_debug(None);

        assert_events_collect_to_full_response(&mut h.engine, &h.registry, request);
    }

    #[test]
    fn event_stream_matches_resource_payload_response() {
        let mut engine = Engine::new(TreePath::parse("/basic.project").unwrap());
        engine.runtime_buffers_mut().insert(WithRevision::new(
            Revision::new(1),
            RuntimeBuffer::raw(vec![1, 2, 3, 253, 254, 255]),
        ));
        let registry = ProjectRegistry::new();
        let mut request = ProjectReadRequest::default_debug(None);
        request.queries[2] = ProjectReadQuery::Resources(ResourceReadQuery {
            level: lpc_wire::ReadLevel::Detail,
            payloads: ResourcePayloadRead::All,
        });

        assert_events_collect_to_full_response(&mut engine, &registry, request);
    }

    #[test]
    fn event_stream_slot_payloads_read_through_sync_codec() {
        let mut h = EngineTestBuilder::new()
            .shader("shader", output("outputs[0]", 0.75))
            .build();
        let request = ProjectReadRequest::default_debug(None);

        assert_detailed_slot_roots_read_through_sync_codec(&mut h.engine, &h.registry, request);
    }

    #[test]
    fn event_stream_slot_payloads_apply_to_view() {
        let mut h = EngineTestBuilder::new()
            .shader("shader", output("outputs[0]", 0.75))
            .build();
        let request = ProjectReadRequest::default_debug(None);
        let (decoded, _) = collect_event_response(&mut h.engine, &h.registry, request);
        let mut view = lpc_view::ProjectView::new();

        lpc_view::apply_project_read_response(&mut view, decoded).expect("apply project read");

        assert!(!view.slots.roots.is_empty());
        assert!(
            view.slots
                .roots
                .keys()
                .any(|root| root.starts_with("node."))
        );
    }

    #[test]
    fn event_stream_chunks_runtime_buffer_payloads() {
        let mut engine = Engine::new(TreePath::parse("/basic.project").unwrap());
        engine.runtime_buffers_mut().insert(WithRevision::new(
            Revision::new(1),
            RuntimeBuffer::raw(vec![42; 5 * 1024]),
        ));
        let registry = ProjectRegistry::new();
        let mut request = ProjectReadRequest::default_debug(None);
        request.queries[2] = ProjectReadQuery::Resources(ResourceReadQuery {
            level: lpc_wire::ReadLevel::Detail,
            payloads: ResourcePayloadRead::All,
        });

        let (decoded, events) = collect_event_response(&mut engine, &registry, request);
        let chunk_events = events
            .iter()
            .filter(|event| {
                matches!(
                    event,
                    ProjectReadEvent::Query {
                        event: ProjectReadQueryEvent::Resources(
                            ProjectReadResourceEvent::RuntimeBufferPayloadBytes { .. }
                        ),
                        ..
                    }
                )
            })
            .count();

        assert_eq!(decoded.results.len(), 4);
        assert!(chunk_events > 1);
    }

    // ---- Cross-family revision-gating contract (M5 P5) --------------------------------------
    //
    // These exercise the whole `default_debug` query set (shapes + nodes/slots +
    // resources + runtime) in one read, proving the per-family gates compose:
    // a read at the current revision transfers no mirrorable payload, a fresh
    // read transfers everything, and probes run regardless of `since`.

    #[test]
    fn read_at_since_r_sends_no_payload_items() {
        // Project fully settled at revision `R`: shape entry, two slot-bearing
        // nodes, and a runtime buffer, all last changed at `R`. A read with
        // `since == R` must carry the revision (Begin/End/Runtime) but zero
        // mirrorable payload for every family (G1).
        let mut h = build_all_families_project();
        let r = h.engine.revision();

        let (_, events) = collect_event_response(
            &mut h.engine,
            &h.registry,
            full_debug_with_probe(Some(r), None),
        );

        // No payload events for any gated family.
        assert_eq!(count_shape_entries(&events), 0, "zero shape entries");
        assert_eq!(count_shape_membership(&events), 0, "zero shape membership");
        assert_eq!(
            count_resource_summaries(&events),
            0,
            "zero resource summaries"
        );
        assert_eq!(
            count_resource_membership(&events),
            0,
            "zero resource membership"
        );
        assert_eq!(count_slot_roots(&events), 0, "zero slot roots");
        assert_eq!(
            count_nonempty_tree_deltas(&events),
            0,
            "zero non-empty tree deltas"
        );

        // The revision-carrying spine is still present at `R`.
        assert!(
            has_begin_end_with_revision(&events, r),
            "Begin and End carry R"
        );
        assert_eq!(
            runtime_revisions(&events),
            vec![r],
            "Runtime status carries R"
        );
    }

    #[test]
    fn fresh_client_receives_everything() {
        // `since == None` (≡ 0) is a bulk sync: every family sends its full set
        // regardless of per-item `changed_at` (G2 bulk-sync guard).
        let mut h = build_all_families_project();

        let (_, events) = collect_event_response(
            &mut h.engine,
            &h.registry,
            full_debug_with_probe(None, None),
        );

        assert!(count_shape_entries(&events) > 0, "shapes streamed in full");
        assert_eq!(
            count_resource_summaries(&events),
            1,
            "the runtime buffer summary is streamed"
        );
        // Both nodes' `.state` slot roots plus their tree entries arrive.
        assert_eq!(count_slot_roots(&events), 2, "both slot roots streamed");
        assert!(
            count_nonempty_tree_deltas(&events) > 0,
            "tree deltas streamed"
        );
    }

    #[test]
    fn probes_run_regardless_of_since() {
        // Probes are live work, not mirror state: a read at `since == R` (which
        // gates out every family) must still execute and return the probe result
        // (G9).
        let mut h = build_all_families_project();
        let r = h.engine.revision();
        let node_a = h.node("a");

        let (_, events) = collect_event_response(
            &mut h.engine,
            &h.registry,
            full_debug_with_probe(Some(r), Some(node_a)),
        );

        let probe_results = events
            .iter()
            .filter(|event| matches!(event, ProjectReadEvent::Probe { .. }))
            .count();
        assert_eq!(probe_results, 1, "probe result present at since == R");
        // And gating still held: the probe rode alongside an otherwise-empty read.
        assert_eq!(count_shape_entries(&events), 0);
        assert_eq!(count_resource_summaries(&events), 0);
        assert_eq!(count_slot_roots(&events), 0);
    }

    fn count_shape_entries(events: &[ProjectReadEvent]) -> usize {
        events
            .iter()
            .filter(|event| {
                matches!(
                    event,
                    ProjectReadEvent::Query {
                        event: ProjectReadQueryEvent::Shapes(ProjectReadShapeEvent::Entry { .. }),
                        ..
                    }
                )
            })
            .count()
    }

    fn count_shape_membership(events: &[ProjectReadEvent]) -> usize {
        events
            .iter()
            .filter(|event| {
                matches!(
                    event,
                    ProjectReadEvent::Query {
                        event: ProjectReadQueryEvent::Shapes(
                            ProjectReadShapeEvent::Membership { .. }
                        ),
                        ..
                    }
                )
            })
            .count()
    }

    fn count_resource_summaries(events: &[ProjectReadEvent]) -> usize {
        events
            .iter()
            .filter(|event| {
                matches!(
                    event,
                    ProjectReadEvent::Query {
                        event: ProjectReadQueryEvent::Resources(ProjectReadResourceEvent::Summary(
                            _
                        )),
                        ..
                    }
                )
            })
            .count()
    }

    fn count_resource_membership(events: &[ProjectReadEvent]) -> usize {
        events
            .iter()
            .filter(|event| {
                matches!(
                    event,
                    ProjectReadEvent::Query {
                        event: ProjectReadQueryEvent::Resources(
                            ProjectReadResourceEvent::Membership { .. }
                        ),
                        ..
                    }
                )
            })
            .count()
    }

    fn count_slot_roots(events: &[ProjectReadEvent]) -> usize {
        events
            .iter()
            .filter(|event| {
                matches!(
                    event,
                    ProjectReadEvent::Query {
                        event: ProjectReadQueryEvent::Nodes(ProjectReadNodeEvent::SlotRoot(_)),
                        ..
                    }
                )
            })
            .count()
    }

    /// Count tree-delta events that actually carry deltas. The stream only emits
    /// a `TreeDeltas` event when the delta set is non-empty, so any present event
    /// is a real payload; this guards that invariant explicitly.
    fn count_nonempty_tree_deltas(events: &[ProjectReadEvent]) -> usize {
        events
            .iter()
            .filter(|event| match event {
                ProjectReadEvent::Query {
                    event: ProjectReadQueryEvent::Nodes(ProjectReadNodeEvent::TreeDeltas { deltas }),
                    ..
                } => !deltas.is_empty(),
                _ => false,
            })
            .count()
    }

    fn has_begin_end_with_revision(events: &[ProjectReadEvent], revision: Revision) -> bool {
        let begin = events.iter().any(
            |event| matches!(event, ProjectReadEvent::Begin { revision: r } if *r == revision),
        );
        let end = events
            .iter()
            .any(|event| matches!(event, ProjectReadEvent::End { revision: r } if *r == revision));
        begin && end
    }

    fn runtime_revisions(events: &[ProjectReadEvent]) -> Vec<Revision> {
        events
            .iter()
            .filter_map(|event| match event {
                ProjectReadEvent::Query {
                    event: ProjectReadQueryEvent::Runtime(result),
                    ..
                } => Some(result.project.revision),
                _ => None,
            })
            .collect()
    }

    /// Build a project exercising every mirrorable family: a shape entry, two
    /// state-bearing nodes (tree + slots), and a runtime buffer resource — all
    /// stamped at revision 1 — then tick once so the engine's project revision
    /// equals the revision at which everything last changed.
    fn build_all_families_project() -> crate::engine::test_support::EngineTestHarness {
        let mut h = EngineTestBuilder::new()
            .shader("a", output("outputs[0]", 0.5))
            .shader("b", output("outputs[0]", 0.5))
            .build();
        h.engine
            .slot_shapes_mut()
            .register_shape_with_version(
                Revision::new(1),
                SlotShapeId::new(0x7000_0001),
                SlotShape::value(LpType::Bool),
            )
            .expect("register shape");
        h.engine.runtime_buffers_mut().insert(WithRevision::new(
            Revision::new(1),
            RuntimeBuffer::raw(vec![1, 2, 3]),
        ));
        h.tick(10).expect("tick to advance project revision");
        h
    }

    /// A full debug read plus an optional explain-slot probe, at the given
    /// `since`.
    fn full_debug_with_probe(
        since: Option<Revision>,
        probe_node: Option<lpc_model::NodeId>,
    ) -> ProjectReadRequest {
        let probes = match probe_node {
            Some(node) => vec![lpc_wire::ProjectProbeRequest::ExplainSlot(
                lpc_wire::ExplainSlotProbeRequest {
                    node,
                    slot: lpc_model::SlotPath::parse("in").expect("slot path"),
                    include_trace: false,
                },
            )],
            None => Vec::new(),
        };
        ProjectReadRequest {
            since,
            queries: ProjectReadQuery::default_debug(),
            probes,
        }
    }

    // ---- Shapes revision-gating contract (M5 P1) ----

    use lpc_model::{LpType, SlotShape, SlotShapeEntry, SlotShapeId};
    use lpc_wire::{ProjectReadQuery, ReadLevel, ShapeReadQuery};

    fn shapes_request(since: Option<Revision>) -> ProjectReadRequest {
        ProjectReadRequest {
            since,
            queries: vec![ProjectReadQuery::Shapes(ShapeReadQuery {
                level: ReadLevel::Detail,
            })],
            probes: Vec::new(),
        }
    }

    fn collect_shape_events(
        engine: &mut Engine,
        registry: &ProjectRegistry,
        since: Option<Revision>,
    ) -> Vec<ProjectReadShapeEvent> {
        let (_, events) = collect_event_response(engine, registry, shapes_request(since));
        events
            .into_iter()
            .filter_map(|event| match event {
                ProjectReadEvent::Query {
                    event: ProjectReadQueryEvent::Shapes(shape_event),
                    ..
                } => Some(shape_event),
                _ => None,
            })
            .collect()
    }

    // ---- M5 G6a: per-root slot revision gating ----

    #[test]
    fn fresh_read_includes_all_roots() {
        // Two state-bearing nodes attached at revision 1. A fresh read
        // (since == 0, the bulk-sync guard) must include both `.state` roots.
        let mut h = EngineTestBuilder::new()
            .shader("a", output("outputs[0]", 0.5))
            .shader("b", output("outputs[0]", 0.5))
            .build();
        let a = h.node("a");
        let b = h.node("b");

        let roots = slot_root_names(&mut h.engine, &h.registry, None);

        assert!(roots.contains(&node_state_root_name(a)), "roots: {roots:?}");
        assert!(roots.contains(&node_state_root_name(b)), "roots: {roots:?}");
    }

    #[test]
    fn unchanged_slots_send_no_roots() {
        // Both nodes last changed at revision 1. A read with since == 1 (>= every
        // root revision, strict `>` gate) must send zero slot-root snapshots.
        let mut h = EngineTestBuilder::new()
            .shader("a", output("outputs[0]", 0.5))
            .shader("b", output("outputs[0]", 0.5))
            .build();

        let roots = slot_root_names(&mut h.engine, &h.registry, Some(Revision::new(1)));

        assert!(roots.is_empty(), "expected no slot roots, got {roots:?}");
    }

    #[test]
    fn mutating_one_slot_sends_exactly_that_root() {
        // Snapshot at R_before == 1, then bump only node "a"'s runtime entry to
        // revision 2. Reading since == 1 must send exactly a's `.state` root and
        // no other node's root.
        let mut h = EngineTestBuilder::new()
            .shader("a", output("outputs[0]", 0.5))
            .shader("b", output("outputs[0]", 0.5))
            .build();
        let a = h.node("a");
        let b = h.node("b");

        // Bump a's runtime `changed_at` to revision 2 (state root gate source).
        h.engine
            .tree_mut()
            .get_mut(a)
            .expect("node a entry")
            .set_status(lpc_wire::NodeRuntimeStatus::Ok, Revision::new(2));

        let roots = slot_root_names(&mut h.engine, &h.registry, Some(Revision::new(1)));

        assert_eq!(
            roots,
            Vec::from([node_state_root_name(a)]),
            "expected only a's state root; b={:?}",
            node_state_root_name(b)
        );
    }

    #[test]
    fn def_change_resends_def_root() {
        // A def-backed node whose `.def` root revision (3) is newer than its
        // runtime `.state` revision (1). Reading with since == 2 re-sends the
        // `.def` root (3 > 2) but not the `.state` root (1 is not > 2).
        let mut engine = Engine::new(TreePath::parse("/t.show").expect("path"));
        let mut registry = ProjectRegistry::new();
        let root = engine.tree().root();
        let tid = engine
            .tree_mut()
            .add_child(
                root,
                NodeName::parse("tex").expect("name"),
                NodeName::parse("texture").expect("ty"),
                WireChildKind::Input {
                    source: WireSlotIndex(0),
                },
                test_placeholder_spine(),
                Revision::new(1),
            )
            .expect("add texture node");
        // Runtime state stamped at revision 1.
        engine
            .attach_runtime_node(tid, Box::new(TextureNode::new(tid)), Revision::new(1))
            .expect("attach texture node");
        // Def entry revision stamped at revision 3 (newer than state).
        engine
            .load_test_node_defs(
                &mut registry,
                &[(tid, NodeDef::Texture(TextureDef::new(16, 16)))],
                Revision::new(3),
            )
            .expect("load texture def");

        let roots = slot_root_names(&mut engine, &registry, Some(Revision::new(2)));

        assert_eq!(
            roots,
            Vec::from([node_def_root_name(tid)]),
            "expected only the def root re-sent"
        );
    }

    /// Collect the `name`s of every `SlotRoot` event from a nodes-detail read at
    /// the given `since`.
    fn slot_root_names(
        engine: &mut Engine,
        registry: &ProjectRegistry,
        since: Option<Revision>,
    ) -> Vec<String> {
        let request = ProjectReadRequest {
            since,
            queries: Vec::from([ProjectReadQuery::Nodes(NodeReadQuery::detail_all())]),
            probes: Vec::new(),
        };
        let (_, events) = collect_event_response(engine, registry, request);
        events
            .into_iter()
            .filter_map(|event| match event {
                ProjectReadEvent::Query {
                    event: ProjectReadQueryEvent::Nodes(ProjectReadNodeEvent::SlotRoot(root)),
                    ..
                } => Some(root.name),
                _ => None,
            })
            .collect()
    }

    fn shape_entry_ids(events: &[ProjectReadShapeEvent]) -> Vec<SlotShapeId> {
        events
            .iter()
            .filter_map(|event| match event {
                ProjectReadShapeEvent::Entry { id, .. } => Some(*id),
                _ => None,
            })
            .collect()
    }

    fn shape_membership(events: &[ProjectReadShapeEvent]) -> Option<Vec<SlotShapeId>> {
        events.iter().find_map(|event| match event {
            ProjectReadShapeEvent::Membership { ids } => Some(ids.clone()),
            _ => None,
        })
    }

    fn empty_registry_engine() -> (Engine, ProjectRegistry) {
        (
            Engine::new(TreePath::parse("/shapes.project").unwrap()),
            ProjectRegistry::new(),
        )
    }

    #[test]
    fn mutating_one_shape_sends_exactly_that_shape() {
        let (mut engine, registry) = empty_registry_engine();
        let stable = SlotShapeId::new(0x7000_0001);
        let mutated = SlotShapeId::new(0x7000_0002);
        engine
            .slot_shapes_mut()
            .register_shape_with_version(Revision::new(3), stable, SlotShape::value(LpType::Bool))
            .expect("register stable shape");
        engine
            .slot_shapes_mut()
            .register_shape_with_version(Revision::new(3), mutated, SlotShape::value(LpType::Bool))
            .expect("register mutated shape");
        let r_before = Revision::new(3);
        // Re-stamp only `mutated`'s content past `r_before` while leaving the id
        // set (and thus `ids_revision`) at `r_before`. Applying a partial
        // snapshot keeps `ids_revision` fixed, isolating the per-entry
        // `changed_at` gate from the membership gate.
        let mut snapshot = engine.slot_shapes().snapshot();
        snapshot.ids_revision = r_before;
        snapshot.shapes.insert(
            mutated,
            SlotShapeEntry::new(Revision::new(5), SlotShape::value(LpType::F32)),
        );
        engine.slot_shapes_mut().apply_partial_snapshot(snapshot);

        let events = collect_shape_events(&mut engine, &registry, Some(r_before));

        assert_eq!(shape_entry_ids(&events), vec![mutated]);
        assert_eq!(
            shape_membership(&events),
            None,
            "id set unchanged: no membership event"
        );
    }

    #[test]
    fn removing_a_shape_emits_membership_without_it() {
        let (mut engine, registry) = empty_registry_engine();
        let kept = SlotShapeId::new(0x7000_0001);
        let removed = SlotShapeId::new(0x7000_0002);
        engine
            .slot_shapes_mut()
            .register_shape_with_version(Revision::new(3), kept, SlotShape::value(LpType::Bool))
            .expect("register kept shape");
        engine
            .slot_shapes_mut()
            .register_shape_with_version(Revision::new(3), removed, SlotShape::value(LpType::Bool))
            .expect("register removed shape");
        let r_before = Revision::new(3);
        engine
            .slot_shapes_mut()
            .unregister_shape_with_version(Revision::new(5), &removed);

        let events = collect_shape_events(&mut engine, &registry, Some(r_before));

        let membership = shape_membership(&events).expect("membership event present");
        assert!(
            !membership.contains(&removed),
            "removed id absent from membership"
        );
        assert!(membership.contains(&kept), "kept id present in membership");
        assert!(
            !shape_entry_ids(&events).contains(&removed),
            "no entry for removed shape"
        );
    }

    #[test]
    fn unchanged_shapes_send_no_entries_or_membership() {
        let (mut engine, registry) = empty_registry_engine();
        let id = SlotShapeId::new(0x7000_0001);
        engine
            .slot_shapes_mut()
            .register_shape_with_version(Revision::new(4), id, SlotShape::value(LpType::Bool))
            .expect("register shape");
        let r = Revision::new(4);

        let events = collect_shape_events(&mut engine, &registry, Some(r));

        assert!(
            shape_entry_ids(&events).is_empty(),
            "zero entries at since==R"
        );
        assert_eq!(shape_membership(&events), None, "no membership at since==R");
    }

    #[test]
    fn fresh_read_includes_all_shapes() {
        let (mut engine, registry) = empty_registry_engine();
        let first = SlotShapeId::new(0x7000_0001);
        let second = SlotShapeId::new(0x7000_0002);
        engine
            .slot_shapes_mut()
            .register_shape_with_version(Revision::new(2), first, SlotShape::value(LpType::Bool))
            .expect("register first shape");
        engine
            .slot_shapes_mut()
            .register_shape_with_version(Revision::new(5), second, SlotShape::value(LpType::F32))
            .expect("register second shape");

        let events = collect_shape_events(&mut engine, &registry, None);

        let ids = shape_entry_ids(&events);
        assert!(ids.contains(&first), "fresh read includes first shape");
        assert!(ids.contains(&second), "fresh read includes second shape");
    }

    // ---- Revision-gated resource read contract (M5 P2) --------------------------------------

    #[test]
    fn mutating_one_resource_sends_exactly_that_resource() {
        use lpc_model::set_current_revision;

        let mut engine = Engine::new(TreePath::parse("/basic.project").unwrap());
        let registry = ProjectRegistry::new();

        set_current_revision(Revision::new(5));
        let stable = engine.runtime_buffers_mut().insert(WithRevision::new(
            Revision::new(5),
            RuntimeBuffer::raw(vec![1]),
        ));
        let changed = engine.runtime_buffers_mut().insert(WithRevision::new(
            Revision::new(5),
            RuntimeBuffer::raw(vec![2]),
        ));

        // A later mutation bumps only `changed`'s revision; `stable` stays at 5.
        set_current_revision(Revision::new(9));
        engine
            .runtime_buffers_mut()
            .replace(
                changed,
                WithRevision::new(Revision::new(9), RuntimeBuffer::raw(vec![3])),
            )
            .expect("replace changed buffer");

        let summaries = resource_summaries(&mut engine, &registry, Some(Revision::new(5)));

        assert_eq!(summaries.len(), 1);
        assert_eq!(
            summaries[0].resource_ref,
            lpc_model::ResourceRef::runtime_buffer(changed)
        );
        // No summary for the unchanged buffer; no membership (id set unchanged since 5).
        assert!(
            summaries
                .iter()
                .all(|s| s.resource_ref != lpc_model::ResourceRef::runtime_buffer(stable))
        );
        assert!(resource_membership(&mut engine, &registry, Some(Revision::new(5))).is_none());
    }

    #[test]
    fn removing_a_resource_emits_membership_without_it() {
        use lpc_model::set_current_revision;

        let mut engine = Engine::new(TreePath::parse("/basic.project").unwrap());
        let registry = ProjectRegistry::new();

        set_current_revision(Revision::new(5));
        let owner_a = lpc_model::NodeId::new(1);
        let owner_b = lpc_model::NodeId::new(2);
        let kept = engine.runtime_buffers_mut().insert_owned(
            owner_a,
            WithRevision::new(Revision::new(5), RuntimeBuffer::raw(vec![1])),
        );
        let removed = engine.runtime_buffers_mut().insert_owned(
            owner_b,
            WithRevision::new(Revision::new(5), RuntimeBuffer::raw(vec![2])),
        );

        // Remove `removed` at a later revision: bumps the store's ids_revision to 8.
        set_current_revision(Revision::new(8));
        engine.runtime_buffers_mut().remove_owned_by(owner_b);

        let since = Some(Revision::new(5));
        let membership =
            resource_membership(&mut engine, &registry, since).expect("membership present");
        assert!(
            membership
                .iter()
                .any(|r| *r == lpc_model::ResourceRef::runtime_buffer(kept))
        );
        assert!(
            membership
                .iter()
                .all(|r| *r != lpc_model::ResourceRef::runtime_buffer(removed)),
            "removed ref must be absent from membership"
        );
        // No summary is sent for the removed buffer (it no longer exists).
        let summaries = resource_summaries(&mut engine, &registry, since);
        assert!(
            summaries
                .iter()
                .all(|s| s.resource_ref != lpc_model::ResourceRef::runtime_buffer(removed))
        );
    }

    #[test]
    fn unchanged_resources_send_no_summaries_or_membership() {
        use lpc_model::set_current_revision;

        let mut engine = Engine::new(TreePath::parse("/basic.project").unwrap());
        let registry = ProjectRegistry::new();

        set_current_revision(Revision::new(5));
        engine.runtime_buffers_mut().insert(WithRevision::new(
            Revision::new(5),
            RuntimeBuffer::raw(vec![1]),
        ));
        engine.runtime_buffers_mut().insert(WithRevision::new(
            Revision::new(5),
            RuntimeBuffer::raw(vec![2]),
        ));

        // Read at since == R (the revision everything was stamped at): nothing to send.
        let since = Some(Revision::new(5));
        assert!(resource_summaries(&mut engine, &registry, since).is_empty());
        assert!(resource_membership(&mut engine, &registry, since).is_none());
    }

    #[test]
    fn by_refs_payload_bypasses_since() {
        use lpc_model::set_current_revision;

        let mut engine = Engine::new(TreePath::parse("/basic.project").unwrap());
        let registry = ProjectRegistry::new();

        set_current_revision(Revision::new(5));
        let id = engine.runtime_buffers_mut().insert(WithRevision::new(
            Revision::new(5),
            RuntimeBuffer::raw(vec![7, 8, 9]),
        ));

        // since == R so the buffer's revision (5) is NOT > since; a gated read sends no payload,
        // but an explicit ByRefs request is a targeted fetch and delivers it anyway.
        let request = resources_request(
            Some(Revision::new(5)),
            ResourceReadQuery {
                level: lpc_wire::ReadLevel::Detail,
                payloads: ResourcePayloadRead::ByRefs(vec![
                    lpc_model::ResourceRef::runtime_buffer(id),
                ]),
            },
        );
        let (decoded, _) = collect_event_response(&mut engine, &registry, request);
        let resources = decoded_resources(&decoded);

        assert_eq!(resources.runtime_buffer_payloads.len(), 1);
        assert_eq!(
            resources.runtime_buffer_payloads[0].resource_ref,
            lpc_model::ResourceRef::runtime_buffer(id)
        );
        assert_eq!(resources.runtime_buffer_payloads[0].bytes, vec![7, 8, 9]);
        // Summaries are still gated: none at since == R.
        assert!(resources.summaries.is_empty());
    }

    #[test]
    fn fresh_read_includes_all_resources() {
        use lpc_model::set_current_revision;

        let mut engine = Engine::new(TreePath::parse("/basic.project").unwrap());
        let registry = ProjectRegistry::new();

        set_current_revision(Revision::new(5));
        engine.runtime_buffers_mut().insert(WithRevision::new(
            Revision::new(5),
            RuntimeBuffer::raw(vec![1]),
        ));
        engine.runtime_buffers_mut().insert(WithRevision::new(
            Revision::new(5),
            RuntimeBuffer::raw(vec![2]),
        ));

        // Fresh read (since == None ⇒ 0): bulk-sync guard sends every live summary and, being a
        // full sync, no membership list is needed.
        let summaries = resource_summaries(&mut engine, &registry, None);
        assert_eq!(summaries.len(), 2);
        assert!(resource_membership(&mut engine, &registry, None).is_none());
    }

    fn resources_request(since: Option<Revision>, query: ResourceReadQuery) -> ProjectReadRequest {
        ProjectReadRequest {
            since,
            queries: vec![ProjectReadQuery::Resources(query)],
            probes: vec![],
        }
    }

    fn decoded_resources(response: &ProjectReadResponse) -> &lpc_wire::ResourceReadResult {
        response
            .results
            .iter()
            .find_map(|result| match result {
                ProjectReadResult::Resources(resources) => Some(resources),
                _ => None,
            })
            .expect("resources result")
    }

    fn resource_summaries(
        engine: &mut Engine,
        registry: &ProjectRegistry,
        since: Option<Revision>,
    ) -> Vec<lpc_wire::WireResourceSummary> {
        let request = resources_request(
            since,
            ResourceReadQuery {
                level: lpc_wire::ReadLevel::Summary,
                payloads: ResourcePayloadRead::None,
            },
        );
        let (decoded, _) = collect_event_response(engine, registry, request);
        decoded_resources(&decoded).summaries.clone()
    }

    fn resource_membership(
        engine: &mut Engine,
        registry: &ProjectRegistry,
        since: Option<Revision>,
    ) -> Option<Vec<lpc_model::ResourceRef>> {
        let request = resources_request(
            since,
            ResourceReadQuery {
                level: lpc_wire::ReadLevel::Summary,
                payloads: ResourcePayloadRead::None,
            },
        );
        let (decoded, _) = collect_event_response(engine, registry, request);
        decoded_resources(&decoded).membership.clone()
    }

    fn assert_events_collect_to_full_response(
        engine: &mut Engine,
        registry: &ProjectRegistry,
        request: ProjectReadRequest,
    ) {
        let full = engine.read_project(registry, request.clone());
        let (decoded, _) = collect_event_response(engine, registry, request);

        assert_eq!(decoded, full);

        let resources = decoded
            .results
            .iter()
            .find_map(|result| match result {
                ProjectReadResult::Resources(resources) => Some(resources),
                _ => None,
            })
            .expect("resources result");
        for payload in &resources.runtime_buffer_payloads {
            assert!(!payload.bytes.is_empty());
        }
    }

    fn assert_detailed_slot_roots_read_through_sync_codec(
        engine: &mut Engine,
        registry: &ProjectRegistry,
        request: ProjectReadRequest,
    ) {
        let (decoded, _) = collect_event_response(engine, registry, request);
        let roots = detailed_node_slot_roots(&decoded);

        assert!(!roots.is_empty(), "expected detailed node slot roots");

        for root in roots {
            lpc_model::slot_sync_codec::read_slot_snapshot_json(
                engine.slot_shapes(),
                root.shape,
                root.data.get(),
            )
            .expect("slot root data should read through slot sync codec");
        }
    }

    fn detailed_node_slot_roots(
        response: &ProjectReadResponse,
    ) -> &[lpc_wire::WireSlotRootSnapshot] {
        response
            .results
            .iter()
            .find_map(|result| match result {
                ProjectReadResult::Nodes(nodes) => nodes.slots.as_ref(),
                _ => None,
            })
            .map(|slots| slots.roots.as_slice())
            .expect("detailed node slot roots")
    }

    fn collect_event_response(
        engine: &mut Engine,
        registry: &ProjectRegistry,
        request: ProjectReadRequest,
    ) -> (ProjectReadResponse, Vec<ProjectReadEvent>) {
        let mut sink = CollectingEventSink::default();
        block_on(async {
            EngineProjectReadSource::new(engine, registry)
                .stream_project_read_events(request, &mut sink)
                .await
                .unwrap();
        });
        let mut collector = ProjectReadCollector::new();
        let events = sink.events;
        for event in events.clone() {
            if let Some(response) = collector.accept_event(event).unwrap() {
                return (response, events);
            }
        }
        panic!("event stream did not complete");
    }

    #[derive(Default)]
    struct CollectingEventSink {
        events: Vec<ProjectReadEvent>,
    }

    impl ProjectReadEventSink for CollectingEventSink {
        type Error = core::convert::Infallible;

        async fn send_project_read_event(
            &mut self,
            event: ProjectReadEvent,
        ) -> Result<(), Self::Error> {
            self.events.push(event);
            Ok(())
        }
    }

    fn block_on<F: Future>(future: F) -> F::Output {
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);
        let mut future = Box::pin(future);
        loop {
            match Future::poll(Pin::as_mut(&mut future), &mut cx) {
                Poll::Ready(output) => return output,
                Poll::Pending => {}
            }
        }
    }

    fn noop_waker() -> Waker {
        unsafe fn clone(_: *const ()) -> RawWaker {
            RawWaker::new(core::ptr::null(), &VTABLE)
        }
        unsafe fn wake(_: *const ()) {}
        unsafe fn wake_by_ref(_: *const ()) {}
        unsafe fn drop(_: *const ()) {}
        static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, wake, wake_by_ref, drop);

        unsafe { Waker::from_raw(RawWaker::new(core::ptr::null(), &VTABLE)) }
    }
}
