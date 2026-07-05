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
                // The overlay is registry state; surface its `changed_at` on
                // every runtime status so clients learn the overlay revision
                // without fetching the overlay itself.
                let overlay_changed_at = self.registry.overlay().changed_at();
                let result = self.engine.read_project_runtime(
                    query,
                    overlay_changed_at,
                    self.server_status.clone(),
                );
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
        stream_probe_result(index, result, sink).await
    }
}

/// Emit one probe result under the enclosing probe `index`, chunking its bulk
/// bytes when they are large enough that a single `Result` event could exceed
/// the frame budget.
///
/// The split threshold and chunk size both come from
/// [`lpc_wire::PROJECT_READ_RUNTIME_CHUNK_BYTES`]: a result carrying no more raw
/// bytes than one chunk is proven (by that constant's compile-time budget
/// assertion) to fit an empty frame once base64-encoded, so it travels whole as
/// `Result`. A larger payload streams as `ResultBegin` → N × `ResultBytes` →
/// `ResultEnd`, each `ResultBytes` bounded to one chunk. Mirrors
/// [`stream_runtime_buffer_payload`].
async fn stream_probe_result<S>(
    index: u32,
    result: ProjectProbeResult,
    sink: &mut S,
) -> Result<(), ProjectReadEventStreamError<S::Error>>
where
    S: ProjectReadEventSink,
{
    const PROBE_RESULT_CHUNK_BYTES: usize = lpc_wire::PROJECT_READ_RUNTIME_CHUNK_BYTES;

    // Only the two bulk-bearing result variants (render texture / control
    // samples) are splittable; everything else is small and always sent whole.
    // A splittable result whose payload still fits one chunk also travels whole,
    // so the small-result path is byte-identical to before for the common case.
    let (header, bytes) = match result.into_chunked_parts() {
        Ok((header, bytes)) if bytes.len() > PROBE_RESULT_CHUNK_BYTES => (header, bytes),
        Ok((header, bytes)) => {
            return send_probe_event(
                sink,
                index,
                ProjectReadProbeEvent::Result(header.into_result(bytes)),
            )
            .await;
        }
        Err(result) => {
            return send_probe_event(sink, index, ProjectReadProbeEvent::Result(result)).await;
        }
    };

    let byte_length = u32::try_from(bytes.len()).map_err(|_| {
        ProjectReadEventStreamError::Protocol("probe result payload is too large".into())
    })?;
    send_probe_event(
        sink,
        index,
        ProjectReadProbeEvent::ResultBegin {
            byte_length,
            header,
        },
    )
    .await?;
    for (chunk_index, chunk) in bytes.chunks(PROBE_RESULT_CHUNK_BYTES).enumerate() {
        let offset = u32::try_from(chunk_index * PROBE_RESULT_CHUNK_BYTES).map_err(|_| {
            ProjectReadEventStreamError::Protocol("probe result offset overflow".into())
        })?;
        send_probe_event(
            sink,
            index,
            ProjectReadProbeEvent::ResultBytes {
                offset,
                bytes: chunk.to_vec(),
            },
        )
        .await?;
    }
    send_probe_event(sink, index, ProjectReadProbeEvent::ResultEnd).await
}

async fn send_probe_event<S>(
    sink: &mut S,
    index: u32,
    event: ProjectReadProbeEvent,
) -> Result<(), ProjectReadEventStreamError<S::Error>>
where
    S: ProjectReadEventSink,
{
    send_project_read_event(sink, ProjectReadEvent::Probe { index, event }).await
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
    use lpc_model::{NodeDef, NodeName, Revision, TextureDef, TreePath, WithRevision};
    use lpc_wire::{
        NodeReadQuery, ProjectReadEvent, ProjectReadNodeEvent, ProjectReadResourceEvent,
        ResourcePayloadRead, ResourceReadQuery, WireChildKind, WireSlotIndex,
    };

    use crate::engine::project_read_nodes::{node_def_root_name, node_state_root_name};
    use crate::engine::test_support::{
        CollectingEventSink, EngineTestBuilder, block_on, collect_read_events, output,
        read_into_view,
    };
    use crate::node::test_placeholder_spine;
    use crate::nodes::TextureNode;
    use crate::resource::RuntimeBuffer;

    // ---- Probe result chunking (M6 P6) ----

    use lpc_wire::{
        ControlProductProbeResult, ProjectProbeResult, ProjectReadProbeEvent,
        RenderProductProbeResult,
    };

    /// Drive `stream_probe_result` for one probe result and return the emitted
    /// events plus the result the progressive applier reassembles from them.
    fn stream_one_probe(result: ProjectProbeResult) -> (Vec<ProjectReadEvent>, ProjectProbeResult) {
        let mut sink = CollectingEventSink::default();
        block_on(async {
            stream_probe_result(0, result, &mut sink).await.unwrap();
        });
        let events = sink.events;

        // Wrap the probe events in a minimal Begin/End stream and drive them
        // through the applier, which reassembles chunked probe results identically
        // to the whole-result path.
        let mut view = lpc_view::ProjectView::new();
        let mut applier = lpc_view::ProjectReadApplier::new(&mut view);
        applier
            .apply(ProjectReadEvent::Begin {
                revision: Revision::new(1),
            })
            .unwrap();
        for event in events.clone() {
            applier.apply(event).unwrap();
        }
        applier
            .apply(ProjectReadEvent::End {
                revision: Revision::new(1),
            })
            .unwrap();
        let probe = applier
            .take_completed_probe_results()
            .into_iter()
            .next()
            .expect("one probe");
        (events, probe)
    }

    fn probe_result_bytes_count(events: &[ProjectReadEvent]) -> usize {
        events
            .iter()
            .filter(|event| {
                matches!(
                    event,
                    ProjectReadEvent::Probe {
                        event: ProjectReadProbeEvent::ResultBytes { .. },
                        ..
                    }
                )
            })
            .count()
    }

    fn render_texture_result(byte_len: usize) -> ProjectProbeResult {
        ProjectProbeResult::RenderProduct(RenderProductProbeResult::Texture {
            product: lpc_model::VisualProduct::new(lpc_model::NodeId::new(1), 0),
            revision: Revision::new(1),
            width: 4,
            height: 4,
            format: lpc_wire::WireTextureFormat::Rgba16,
            bytes: vec![7u8; byte_len],
        })
    }

    #[test]
    fn small_probe_result_uses_single_result_event() {
        // A payload no larger than one chunk travels whole as `Result`.
        let (events, probe) = stream_one_probe(render_texture_result(64));

        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0],
            ProjectReadEvent::Probe {
                event: ProjectReadProbeEvent::Result(_),
                ..
            }
        ));
        assert_eq!(probe, render_texture_result(64));
    }

    #[test]
    fn large_probe_result_chunks_and_reassembles_byte_identically() {
        // A payload several chunks large is split; the collector reassembles the
        // exact original result.
        let byte_len = 3 * lpc_wire::PROJECT_READ_RUNTIME_CHUNK_BYTES + 17;
        let original = render_texture_result(byte_len);
        let (events, probe) = stream_one_probe(original.clone());

        // ResultBegin + N ResultBytes + ResultEnd, all under probe index 0.
        assert!(matches!(
            events.first(),
            Some(ProjectReadEvent::Probe {
                event: ProjectReadProbeEvent::ResultBegin { .. },
                ..
            })
        ));
        assert!(matches!(
            events.last(),
            Some(ProjectReadEvent::Probe {
                event: ProjectReadProbeEvent::ResultEnd,
                ..
            })
        ));
        assert!(
            probe_result_bytes_count(&events) > 1,
            "expected multiple chunk events, events: {events:?}"
        );
        assert_eq!(probe, original);
    }

    #[test]
    fn unsupported_probe_result_never_chunks() {
        // Non-bulk variants have no bytes to split and always go whole.
        let result = ProjectProbeResult::ControlProduct(ControlProductProbeResult::Unsupported {
            product: lpc_model::ControlProduct::new(
                lpc_model::NodeId::new(1),
                0,
                lpc_model::ControlExtent::new(1, 1),
            ),
            reason: alloc::string::String::from("nope"),
        });
        let (events, probe) = stream_one_probe(result.clone());
        assert_eq!(events.len(), 1);
        assert_eq!(probe, result);
    }

    // The engine-side identity tests (`event_stream_matches_full_debug_response`,
    // `event_stream_matches_resource_payload_response`) that compared the stream
    // against the aggregate `ProjectReadResponse` were retired in M6/P5 when the
    // aggregate was deleted. `lpc-view`'s `ProjectReadApplier` equivalence tests
    // now guard that events apply to the same view state, and the tests below
    // assert directly on the streamed events / applied view.

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
        let (view, _) = read_into_view(&mut h.engine, &h.registry, request);

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
        let buffer_id = engine.runtime_buffers_mut().insert(WithRevision::new(
            Revision::new(1),
            RuntimeBuffer::raw(vec![42; 5 * 1024]),
        ));
        let buffer_ref = lpc_model::ResourceRef::runtime_buffer(buffer_id);
        let registry = ProjectRegistry::new();
        let mut request = ProjectReadRequest::default_debug(None);
        request.queries[2] = ProjectReadQuery::Resources(ResourceReadQuery {
            level: lpc_wire::ReadLevel::Detail,
            payloads: ResourcePayloadRead::All,
        });

        let (view, events) = read_into_view(&mut engine, &registry, request);
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

        // The payload streamed as multiple chunk events and the applier
        // reassembled it byte-completely into the view.
        assert!(chunk_events > 1);
        assert_eq!(
            view.resource_cache.runtime_buffer_bytes(buffer_ref),
            Some(&[42u8; 5 * 1024][..])
        );
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

        let events = collect_read_events(
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
    fn studio_steady_state_read_carries_no_payload() {
        // The M6 payoff: Studio's live refresh sends `since = view.revision`
        // with the default-debug query set (shapes/nodes+slots/resources/
        // runtime) — byte-identical to `lpa-studio-core`'s
        // `project_read_request`. When the project has not advanced since the
        // last read, an idle refresh must transfer *no* mirrorable payload:
        // zero shape entries/membership, zero resource summaries/membership,
        // zero slot roots, zero tree deltas. Only the revision-carrying spine
        // (Begin/Runtime/End at R) rides the wire. This is the studio-request
        // analogue of `read_at_since_r_sends_no_payload_items` and stands in
        // for a studio-core integration test (studio-core has no engine dep).
        let mut h = build_all_families_project();
        let r = h.engine.revision();

        // Exactly the request `lpa-studio-core` builds for a gated refresh.
        let request = ProjectReadRequest {
            since: Some(r),
            queries: ProjectReadQuery::default_debug(),
            probes: Vec::new(),
        };
        let events = collect_read_events(&mut h.engine, &h.registry, request);

        assert_eq!(count_shape_entries(&events), 0, "zero shape entries");
        assert_eq!(count_shape_membership(&events), 0, "zero shape membership");
        assert_eq!(count_resource_summaries(&events), 0, "zero summaries");
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
        assert!(
            has_begin_end_with_revision(&events, r),
            "Begin and End carry R"
        );
        assert_eq!(runtime_revisions(&events), vec![r], "Runtime carries R");
    }

    #[test]
    fn fresh_client_receives_everything() {
        // `since == None` (≡ 0) is a bulk sync: every family sends its full set
        // regardless of per-item `changed_at` (G2 bulk-sync guard).
        let mut h = build_all_families_project();

        let events = collect_read_events(
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

        let events = collect_read_events(
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

    #[test]
    fn runtime_status_reports_overlay_changed_at() {
        // The runtime status surfaces the registry overlay's `changed_at` on
        // every read: zero while no overlay exists, then the mutation revision
        // once the overlay changed.
        let mut h = build_all_families_project();

        let events = collect_read_events(
            &mut h.engine,
            &h.registry,
            full_debug_with_probe(None, None),
        );
        assert_eq!(
            overlay_changed_ats(&events),
            vec![Revision::default()],
            "fresh registry overlay reports revision zero"
        );

        // Mutate the overlay at revision 9; the next read must report it.
        let fs = lpfs::LpFsMemory::new();
        let ctx = lpc_registry::ParseCtx {
            shapes: h.engine.slot_shapes(),
        };
        h.registry
            .mutate(
                &fs,
                lpc_model::MutationOp::PutSlotEdit {
                    artifact: lpc_model::ArtifactLocation::file("/project.json"),
                    edit: lpc_model::SlotEdit::ensure_present(
                        lpc_model::SlotPath::parse("nodes[clock]").expect("slot path"),
                    ),
                },
                Revision::new(9),
                &ctx,
            )
            .expect("mutate overlay");

        let events = collect_read_events(
            &mut h.engine,
            &h.registry,
            full_debug_with_probe(None, None),
        );
        assert_eq!(overlay_changed_ats(&events), vec![Revision::new(9)]);
    }

    fn overlay_changed_ats(events: &[ProjectReadEvent]) -> Vec<Revision> {
        events
            .iter()
            .filter_map(|event| match event {
                ProjectReadEvent::Query {
                    event: ProjectReadQueryEvent::Runtime(result),
                    ..
                } => Some(result.project.overlay_changed_at),
                _ => None,
            })
            .collect()
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
        let events = collect_read_events(engine, registry, shapes_request(since));
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
        let events = collect_read_events(engine, registry, request);
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
        let (view, events) = read_into_view(&mut engine, &registry, request);

        // The targeted payload was delivered and reassembled into the view.
        assert_eq!(
            view.resource_cache
                .runtime_buffer_bytes(lpc_model::ResourceRef::runtime_buffer(id)),
            Some(&[7u8, 8, 9][..])
        );
        // Summaries are still gated: none at since == R.
        assert_eq!(resource_summary_events(&events).count(), 0);
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

    /// The resource `Summary` events carried by a streamed read.
    fn resource_summary_events(
        events: &[ProjectReadEvent],
    ) -> impl Iterator<Item = &lpc_wire::WireResourceSummary> {
        events.iter().filter_map(|event| match event {
            ProjectReadEvent::Query {
                event: ProjectReadQueryEvent::Resources(ProjectReadResourceEvent::Summary(summary)),
                ..
            } => Some(summary),
            _ => None,
        })
    }

    /// The summaries a summary-level resource read streams for `since`.
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
        let events = collect_read_events(engine, registry, request);
        resource_summary_events(&events).cloned().collect()
    }

    /// The resource membership list a read streams for `since`, or `None` when
    /// the read carries no `Membership` event (bulk sync / unchanged id set).
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
        let events = collect_read_events(engine, registry, request);
        events.into_iter().find_map(|event| match event {
            ProjectReadEvent::Query {
                event:
                    ProjectReadQueryEvent::Resources(ProjectReadResourceEvent::Membership { refs }),
                ..
            } => Some(refs),
            _ => None,
        })
    }

    fn assert_detailed_slot_roots_read_through_sync_codec(
        engine: &mut Engine,
        registry: &ProjectRegistry,
        request: ProjectReadRequest,
    ) {
        let events = collect_read_events(engine, registry, request);
        let roots: Vec<&lpc_wire::WireSlotRootSnapshot> = events
            .iter()
            .filter_map(|event| match event {
                ProjectReadEvent::Query {
                    event: ProjectReadQueryEvent::Nodes(ProjectReadNodeEvent::SlotRoot(root)),
                    ..
                } => Some(root),
                _ => None,
            })
            .collect();

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
}
