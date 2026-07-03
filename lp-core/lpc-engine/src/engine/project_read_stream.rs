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
                let result = self.engine.read_project_shapes(query);
                if let Some(registry) = result.registry {
                    send_query_event(
                        sink,
                        index,
                        ProjectReadQueryEvent::Shapes(ProjectReadShapeEvent::Begin {
                            level: result.level,
                            ids_revision: registry.ids_revision,
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
                let result = self.engine.read_project_resources(query);
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
    use lpc_model::{Revision, TreePath, WithRevision};
    use lpc_wire::{
        ProjectReadCollector, ProjectReadEvent, ProjectReadResourceEvent, ProjectReadResponse,
        ProjectReadResult, ResourcePayloadRead, ResourceReadQuery,
    };

    use crate::engine::test_support::{EngineTestBuilder, output};
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
