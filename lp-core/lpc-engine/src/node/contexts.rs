//! Narrow contexts passed into [`super::NodeRuntime`] hooks.
//!
//! [`TickContext`] resolves through the active [`ResolveSession`] and [`ResolveHost`] using
//! [`QueryKey`] (not the legacy slot resolver cache).

use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;

use crate::artifact::ArtifactId;
use crate::bus::Bus;
use crate::gfx::LpGraphics;
use crate::render_product::{
    NativeTexturePayload, RenderProduct, RenderProductId, RenderProductStore, RenderSampleBatch,
    RenderSampleBatchResult, RenderTextureRequest, TextureRenderProduct,
};
use crate::resolver::{Production, QueryKey, ResolveError, TickResolver};
use crate::runtime_buffer::{RuntimeBuffer, RuntimeBufferId, RuntimeBufferStore};
use lpc_model::{NodeId, Revision, WithRevision, bus::ChannelName};
use lps_shared::LpsValueF32;

use super::node_error::NodeError;

/// Narrow store access for allocating node-owned render products and runtime buffers at attach time.
///
/// Passed to [`super::super::NodeRuntime::init_resources`] before the node payload is [`crate::node::NodeEntryState::Alive`].
pub struct NodeResourceInitContext<'a> {
    render_products: &'a mut RenderProductStore,
    runtime_buffers: &'a mut RuntimeBufferStore,
}

impl<'a> NodeResourceInitContext<'a> {
    pub fn new(
        render_products: &'a mut RenderProductStore,
        runtime_buffers: &'a mut RuntimeBufferStore,
    ) -> Self {
        Self {
            render_products,
            runtime_buffers,
        }
    }

    pub fn insert_render_product(&mut self, product: Box<dyn RenderProduct>) -> RenderProductId {
        self.render_products.insert(product)
    }

    pub fn insert_runtime_buffer(
        &mut self,
        buffer: WithRevision<RuntimeBuffer>,
    ) -> RuntimeBufferId {
        self.runtime_buffers.insert(buffer)
    }
}

/// Pending uploads to [`crate::render_product::RenderProductStore`] applied after the current
/// node's [`super::Node::tick`](super::NodeRuntime::tick) returns (see [`TickContext::defer_render_product_replace`]).
pub type PendingRenderProductReplaces<'r> = &'r mut Vec<(RenderProductId, Box<dyn RenderProduct>)>;

/// Context for [`super::Node::tick`](super::NodeRuntime::tick).
///
/// Demand-style reads go through [`TickResolver`] (typically [`crate::resolver::SessionHostResolver`]).
pub struct TickContext<'r> {
    node_id: NodeId,
    revision: Revision,
    artifact_ref: ArtifactId,
    artifact_content_frame: Revision,
    resolver: &'r mut dyn TickResolver,
    deferred_render_replaces: Option<PendingRenderProductReplaces<'r>>,
    graphics: Option<Arc<dyn LpGraphics>>,
    frame_time_seconds: f32,
}

impl<'r> TickContext<'r> {
    pub fn new(
        node_id: NodeId,
        frame_id: Revision,
        artifact_ref: ArtifactId,
        artifact_content_frame: Revision,
        resolver: &'r mut dyn TickResolver,
    ) -> Self {
        Self::with_render_services(
            node_id,
            frame_id,
            artifact_ref,
            artifact_content_frame,
            resolver,
            None,
            None,
            0.0,
        )
    }

    /// [`TickContext`] with graphics, frame time, and optional deferred render-product replaces.
    pub fn with_render_services(
        node_id: NodeId,
        frame_id: Revision,
        artifact_ref: ArtifactId,
        artifact_content_frame: Revision,
        resolver: &'r mut dyn TickResolver,
        deferred_render_replaces: Option<PendingRenderProductReplaces<'r>>,
        graphics: Option<Arc<dyn LpGraphics>>,
        frame_time_seconds: f32,
    ) -> Self {
        Self {
            node_id,
            revision: frame_id,
            artifact_ref,
            artifact_content_frame,
            resolver,
            deferred_render_replaces,
            graphics,
            frame_time_seconds,
        }
    }

    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    pub fn revision(&self) -> Revision {
        self.revision
    }

    /// Resolve a [`QueryKey`] for this frame (cache, bindings, optional host production).
    pub fn resolve(&mut self, query: QueryKey) -> Result<Production, ResolveError> {
        self.resolver.resolve(query)
    }

    pub fn artifact_ref(&self) -> ArtifactId {
        self.artifact_ref
    }

    pub fn artifact_content_frame(&self) -> Revision {
        self.artifact_content_frame
    }

    pub fn artifact_changed_since(&self, since: Revision) -> bool {
        self.artifact_content_frame.0 > since.0
    }

    /// Monotonic shader time in seconds for the current engine frame.
    pub fn time_seconds(&self) -> f32 {
        self.frame_time_seconds
    }

    /// Graphics backend for shader compile and output buffers, when the engine has one installed.
    pub fn graphics(&self) -> Option<&dyn LpGraphics> {
        self.graphics.as_ref().map(|g| g.as_ref())
    }

    /// Stage a texture (or other) render product to replace `id` after this tick returns.
    pub fn defer_render_product_replace(
        &mut self,
        id: RenderProductId,
        product: Box<dyn RenderProduct>,
    ) -> Result<(), NodeError> {
        let Some(buf) = self.deferred_render_replaces.as_deref_mut() else {
            return Err(NodeError::msg(
                "tick context cannot defer render products (internal engine bug)",
            ));
        };
        buf.push((id, product));
        Ok(())
    }

    /// Samples a [`RenderProductId`] via the engine-owned store (immutable borrow only).
    pub fn sample_render_product(
        &mut self,
        id: RenderProductId,
        batch: &RenderSampleBatch,
    ) -> Result<RenderSampleBatchResult, NodeError> {
        self.resolver.sample_render_product(id, batch).map_err(|e| {
            NodeError::msg(alloc::format!("render product sample_batch: {}", e.message))
        })
    }

    /// Materializes a render product into a full texture through the engine-owned store.
    pub fn render_texture(
        &mut self,
        id: RenderProductId,
        request: &RenderTextureRequest,
    ) -> Result<TextureRenderProduct, NodeError> {
        self.resolver
            .render_texture(id, request)
            .map_err(|e| NodeError::msg(alloc::format!("render texture: {}", e.message)))
    }

    /// Borrows a CPU-backed native texture payload when the render product can expose one.
    pub fn with_native_texture_payload(
        &mut self,
        id: RenderProductId,
        visitor: &mut dyn FnMut(NativeTexturePayload<'_>),
    ) -> Result<(), NodeError> {
        self.resolver
            .with_native_texture_payload(id, visitor)
            .map_err(|e| NodeError::msg(alloc::format!("native texture payload: {}", e.message)))
    }

    /// Mutates a single existing runtime buffer in place and marks it changed for `frame`.
    pub fn with_runtime_buffer_mut<F>(
        &mut self,
        id: RuntimeBufferId,
        frame: Revision,
        write: F,
    ) -> Result<(), NodeError>
    where
        F: FnOnce(&mut RuntimeBuffer) -> Result<(), NodeError>,
    {
        let buffer = self
            .resolver
            .runtime_buffer_mut(id, frame)
            .map_err(|e| NodeError::msg(alloc::format!("runtime buffer mut: {}", e.message)))?;
        write(buffer)
    }
}

/// Context for [`super::Node::destroy`](super::NodeRuntime::destroy).
pub struct DestroyCtx<'a> {
    node_id: NodeId,
    revision: Revision,
    bus: &'a Bus,
}

impl<'a> DestroyCtx<'a> {
    /// Create a new destroy context.
    pub fn new(node_id: NodeId, frame_id: Revision, bus: &'a Bus) -> Self {
        Self {
            node_id,
            revision: frame_id,
            bus,
        }
    }

    /// Node being destroyed.
    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    /// Frame at which destruction is occurring.
    pub fn frame_id(&self) -> Revision {
        self.revision
    }

    /// Read the current value from a bus channel.
    pub fn bus_read(&self, channel: &ChannelName) -> Option<&LpsValueF32> {
        self.bus.read(channel)
    }
}

/// Context for [`super::Node::handle_memory_pressure`](super::NodeRuntime::handle_memory_pressure).
pub struct MemPressureCtx<'a> {
    node_id: NodeId,
    revision: Revision,
    bus: &'a Bus,
}

impl<'a> MemPressureCtx<'a> {
    /// Create a new memory pressure context.
    pub fn new(node_id: NodeId, frame_id: Revision, bus: &'a Bus) -> Self {
        Self {
            node_id,
            revision: frame_id,
            bus,
        }
    }

    /// Node under pressure.
    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    /// Current frame.
    pub fn revision(&self) -> Revision {
        self.revision
    }

    /// Read the current value from a bus channel.
    pub fn bus_read(&self, channel: &ChannelName) -> Option<&LpsValueF32> {
        self.bus.read(channel)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binding::{
        BindingDraft, BindingPriority, BindingRegistry, BindingSource, BindingTarget,
    };
    use crate::node::NodeRuntime;
    use crate::resolver::resolve_trace::ResolveLogLevel;
    use crate::resolver::{
        Production, QueryKey, ResolveHost, ResolveSession, ResolveTrace, Resolver,
        SessionHostResolver, TickResolver,
    };
    use crate::runtime_product::RuntimeProduct;
    use alloc::string::String;
    use lpc_model::Kind;
    use lpc_model::SlotPath;

    struct PanicProduceHost;

    impl ResolveHost for PanicProduceHost {
        fn produce(
            &mut self,
            _query: &QueryKey,
            _session: &mut ResolveSession<'_>,
        ) -> Result<Production, crate::resolver::SessionResolveError> {
            Err(crate::resolver::SessionResolveError::other(
                "unexpected produce in TickContext test",
            ))
        }
    }

    fn session_bundle<'a>(
        resolver: &'a mut Resolver,
        registry: &'a BindingRegistry,
        frame: Revision,
    ) -> ResolveSession<'a> {
        ResolveSession::new(
            frame,
            resolver,
            registry,
            ResolveTrace::new(ResolveLogLevel::Off),
        )
    }

    #[test]
    fn tick_context_accessors() {
        let registry = BindingRegistry::new();
        let mut resolver = Resolver::new();
        let frame = Revision::new(10);
        let mut session = session_bundle(&mut resolver, &registry, frame);
        let mut host = PanicProduceHost;
        let artifact_ref = ArtifactId::from_raw(1);

        let mut bridge = SessionHostResolver {
            session: &mut session,
            host: &mut host,
        };
        let ctx = TickContext::new(
            NodeId::new(7),
            Revision::new(3),
            artifact_ref,
            Revision::new(5),
            &mut bridge as &mut dyn TickResolver,
        );

        assert_eq!(ctx.node_id(), NodeId::new(7));
        assert_eq!(ctx.revision(), Revision::new(3));
        assert_eq!(ctx.artifact_ref(), artifact_ref);
        assert_eq!(ctx.artifact_content_frame(), Revision::new(5));
    }

    #[test]
    fn tick_context_resolve_bus_query() {
        let mut registry = BindingRegistry::new();
        let frame = Revision::new(10);
        let channel = lpc_model::ChannelName(String::from("level_bus"));
        registry
            .register(
                BindingDraft {
                    source: BindingSource::Literal(lpc_model::LpValue::F32(7.8)),
                    target: BindingTarget::BusChannel(channel.clone()),
                    priority: BindingPriority::new(0),
                    kind: lpc_model::Kind::Amplitude,
                    owner: NodeId::new(1),
                },
                frame,
            )
            .expect("register");

        let mut resolver = Resolver::new();
        let mut session = session_bundle(&mut resolver, &registry, frame);
        let mut host = PanicProduceHost;
        let mut bridge = SessionHostResolver {
            session: &mut session,
            host: &mut host,
        };
        let mut ctx = TickContext::new(
            NodeId::new(1),
            frame,
            ArtifactId::from_raw(1),
            Revision::new(1),
            &mut bridge as &mut dyn TickResolver,
        );
        let pv = ctx
            .resolve(QueryKey::Bus(channel.clone()))
            .expect("resolve bus");
        assert!(pv.as_value().expect("value").eq(&LpsValueF32::F32(7.8)));
    }

    #[test]
    fn tick_context_resolve_consumed_slot_query() {
        let mut registry = BindingRegistry::new();
        let frame = Revision::new(10);
        let node = NodeId::new(3);
        let input = SlotPath::parse("in").unwrap();
        registry
            .register(
                BindingDraft {
                    source: BindingSource::Literal(lpc_model::LpValue::F32(4.25)),
                    target: BindingTarget::ConsumedSlot {
                        node,
                        slot: input.clone(),
                    },
                    priority: BindingPriority::new(0),
                    kind: Kind::Amplitude,
                    owner: node,
                },
                frame,
            )
            .expect("register");

        let mut resolver = Resolver::new();
        let mut session = session_bundle(&mut resolver, &registry, frame);
        let mut host = PanicProduceHost;
        let mut bridge = SessionHostResolver {
            session: &mut session,
            host: &mut host,
        };
        let mut ctx = TickContext::new(
            node,
            frame,
            ArtifactId::from_raw(1),
            Revision::new(1),
            &mut bridge as &mut dyn TickResolver,
        );

        let pv = ctx
            .resolve(QueryKey::ConsumedSlot {
                node,
                slot: input.clone(),
            })
            .expect("resolve consumed slot");
        assert!(pv.as_value().expect("value").eq(&LpsValueF32::F32(4.25)));
    }

    #[test]
    fn tick_context_artifact_changed_since_compares_content_frame() {
        let registry = BindingRegistry::new();
        let mut resolver = Resolver::new();
        let frame = Revision::new(10);
        let mut session = session_bundle(&mut resolver, &registry, frame);
        let mut host = PanicProduceHost;

        let mut bridge = SessionHostResolver {
            session: &mut session,
            host: &mut host,
        };
        let ctx = TickContext::new(
            NodeId::new(1),
            frame,
            ArtifactId::from_raw(1),
            Revision::new(5),
            &mut bridge as &mut dyn TickResolver,
        );

        assert!(ctx.artifact_changed_since(Revision::new(4)));
        assert!(!ctx.artifact_changed_since(Revision::new(5)));
        assert!(!ctx.artifact_changed_since(Revision::new(6)));
    }

    struct FixtureProduceHost {
        node: NodeId,
        out_path: SlotPath,
    }

    impl ResolveHost for FixtureProduceHost {
        fn produce(
            &mut self,
            query: &QueryKey,
            session: &mut ResolveSession<'_>,
        ) -> Result<Production, crate::resolver::SessionResolveError> {
            match query {
                QueryKey::ConsumedSlot { node, slot }
                    if *node == self.node && *slot == self.out_path =>
                {
                    Ok(Production::value(
                        lpc_model::WithRevision::new(session.revision(), LpsValueF32::F32(11.0)),
                        crate::resolver::ProductionSource::Default,
                    )?)
                }
                _ => Err(crate::resolver::SessionResolveError::other(
                    "fixture produce mismatch",
                )),
            }
        }
    }

    /// Dummy node that uses [`TickContext::resolve`](TickContext::resolve) from [`super::super::NodeRuntime::tick`].
    struct QueryResolvingNode {
        query: QueryKey,
        resolved_value: Option<f32>,
    }

    impl super::super::NodeRuntime for QueryResolvingNode {
        fn tick(&mut self, ctx: &mut TickContext<'_>) -> Result<(), crate::node::NodeError> {
            let pv = ctx.resolve(self.query.clone()).map_err(|e| {
                crate::node::NodeError::msg(alloc::format!("resolve failed: {}", e.message))
            })?;
            if let LpsValueF32::F32(v) = *pv.as_value().expect("value") {
                self.resolved_value = Some(v);
            }
            Ok(())
        }

        fn destroy(
            &mut self,
            _ctx: &mut super::DestroyCtx<'_>,
        ) -> Result<(), crate::node::NodeError> {
            Ok(())
        }

        fn handle_memory_pressure(
            &mut self,
            _level: super::super::PressureLevel,
            _ctx: &mut super::MemPressureCtx<'_>,
        ) -> Result<(), crate::node::NodeError> {
            Ok(())
        }

        fn produced(&self) -> &dyn crate::prop::ProducedSlotAccess {
            struct EmptyProps;
            impl crate::prop::ProducedSlotAccess for EmptyProps {
                fn get(&self, _path: &SlotPath) -> Option<(RuntimeProduct, Revision)> {
                    None
                }
                fn iter_changed_since<'b>(
                    &'b self,
                    _since: Revision,
                ) -> alloc::boxed::Box<dyn Iterator<Item = (SlotPath, RuntimeProduct, Revision)> + 'b>
                {
                    alloc::boxed::Box::new(alloc::vec::Vec::new().into_iter())
                }
                fn snapshot<'b>(
                    &'b self,
                ) -> alloc::boxed::Box<dyn Iterator<Item = (SlotPath, RuntimeProduct, Revision)> + 'b>
                {
                    alloc::boxed::Box::new(alloc::vec::Vec::new().into_iter())
                }
            }
            static EMPTY_PROPS: EmptyProps = EmptyProps;
            &EMPTY_PROPS
        }
    }

    #[test]
    fn dummy_node_can_resolve_bus_query_from_tick() {
        let mut registry = BindingRegistry::new();
        let frame = Revision::new(10);
        let channel = lpc_model::ChannelName(String::from("in"));
        registry
            .register(
                BindingDraft {
                    source: BindingSource::Literal(lpc_model::LpValue::F32(8.8)),
                    target: BindingTarget::BusChannel(channel.clone()),
                    priority: BindingPriority::new(0),
                    kind: Kind::Amplitude,
                    owner: NodeId::new(2),
                },
                frame,
            )
            .expect("register");

        let mut resolver = Resolver::new();
        let mut session = session_bundle(&mut resolver, &registry, frame);
        let mut host = PanicProduceHost;

        let mut node = QueryResolvingNode {
            query: QueryKey::Bus(channel),
            resolved_value: None,
        };

        let mut bridge = SessionHostResolver {
            session: &mut session,
            host: &mut host,
        };
        let mut ctx = TickContext::new(
            NodeId::new(2),
            frame,
            ArtifactId::from_raw(1),
            Revision::new(1),
            &mut bridge as &mut dyn TickResolver,
        );

        node.tick(&mut ctx).expect("tick should succeed");
        assert_eq!(node.resolved_value, Some(8.8));
    }

    #[test]
    fn dummy_node_can_resolve_consumed_slot_via_host_from_tick() {
        let registry = BindingRegistry::new();
        let frame = Revision::new(10);
        let node_id = NodeId::new(2);
        let input_path = SlotPath::parse("fixture_in").unwrap();

        let mut resolver = Resolver::new();
        let mut session = session_bundle(&mut resolver, &registry, frame);
        let mut host = FixtureProduceHost {
            node: node_id,
            out_path: input_path.clone(),
        };

        let mut node = QueryResolvingNode {
            query: QueryKey::ConsumedSlot {
                node: node_id,
                slot: input_path,
            },
            resolved_value: None,
        };

        let mut bridge = SessionHostResolver {
            session: &mut session,
            host: &mut host,
        };
        let mut ctx = TickContext::new(
            node_id,
            frame,
            ArtifactId::from_raw(1),
            Revision::new(1),
            &mut bridge as &mut dyn TickResolver,
        );

        node.tick(&mut ctx).expect("tick should succeed");
        assert_eq!(node.resolved_value, Some(11.0));
    }

    #[test]
    fn destroy_ctx_accessors() {
        let bus = Bus::new();
        let ctx = DestroyCtx::new(NodeId::new(1), Revision::new(99), &bus);
        assert_eq!(ctx.node_id(), NodeId::new(1));
        assert_eq!(ctx.frame_id(), Revision::new(99));
    }

    #[test]
    fn destroy_ctx_bus_read() {
        let mut bus = Bus::new();
        let channel = ChannelName(String::from("test"));

        bus.claim_writer(
            &channel,
            NodeId::new(1),
            SlotPath::parse("outputs[0]").unwrap(),
            lpc_model::Kind::Amplitude,
        )
        .unwrap();
        bus.publish(&channel, LpsValueF32::F32(2.5), Revision::new(5));

        let ctx = DestroyCtx::new(NodeId::new(1), Revision::new(99), &bus);
        let val = ctx.bus_read(&channel).unwrap();
        assert!(matches!(val, LpsValueF32::F32(2.5)));
    }

    #[test]
    fn mem_pressure_ctx_accessors() {
        let bus = Bus::new();
        let ctx = MemPressureCtx::new(NodeId::new(2), Revision::new(100), &bus);
        assert_eq!(ctx.node_id(), NodeId::new(2));
        assert_eq!(ctx.revision(), Revision::new(100));
    }

    #[test]
    fn mem_pressure_ctx_bus_read() {
        let mut bus = Bus::new();
        let channel = ChannelName(String::from("pressure"));

        bus.claim_writer(
            &channel,
            NodeId::new(1),
            SlotPath::parse("outputs[0]").unwrap(),
            lpc_model::Kind::Amplitude,
        )
        .unwrap();
        bus.publish(&channel, LpsValueF32::F32(0.8), Revision::new(2));

        let ctx = MemPressureCtx::new(NodeId::new(2), Revision::new(100), &bus);
        let val = ctx.bus_read(&channel).unwrap();
        assert!(matches!(val, LpsValueF32::F32(0.8)));
    }
}
