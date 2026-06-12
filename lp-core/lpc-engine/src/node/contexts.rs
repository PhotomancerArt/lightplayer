//! Narrow contexts passed into [`super::NodeRuntime`] hooks.
//!
//! [`TickContext`] resolves through the active [`ResolveSession`] and [`ResolveHost`] using
//! [`QueryKey`] (not the legacy slot resolver cache).

use alloc::rc::Rc;
use alloc::sync::Arc;

use crate::dataflow::bus::Bus;
use crate::dataflow::resolver::{
    Production, ProductionSource, QueryKey, ResolveError, TickResolver,
};
use crate::engine::{ButtonService, RadioService};
use crate::gfx::LpGraphics;
use crate::products::control::{
    ControlLayout, ControlProduct, ControlRenderRequest, ControlRenderTarget,
};
use crate::products::visual::{
    RenderTextureRequest, TextureRenderProduct, VisualProduct, VisualSampleBufferRequest,
    VisualSampleTarget,
};
use crate::resource::{RuntimeBuffer, RuntimeBufferId, RuntimeBufferStore};
use lpc_model::{
    FromLpValue, NodeId, Revision, SlotAccess, SlotAccessor, SlotPath, SlotShapeRegistry,
    WithRevision, bus::ChannelName, lookup_slot_data_and_shape,
};
use lpc_shared::time::TimeProvider;
use lps_shared::LpsValueF32;

use super::node_error::NodeError;

/// Narrow store access for allocating node-owned visual products and runtime buffers at attach time.
///
/// Passed to [`super::super::NodeRuntime::init_resources`] before the node payload is [`crate::node::NodeEntryState::Alive`].
pub struct NodeResourceInitContext<'a> {
    node_id: NodeId,
    runtime_buffers: &'a mut RuntimeBufferStore,
}

impl<'a> NodeResourceInitContext<'a> {
    pub fn new(node_id: NodeId, runtime_buffers: &'a mut RuntimeBufferStore) -> Self {
        Self {
            node_id,
            runtime_buffers,
        }
    }

    pub fn insert_runtime_buffer(
        &mut self,
        buffer: WithRevision<RuntimeBuffer>,
    ) -> RuntimeBufferId {
        self.runtime_buffers.insert_owned(self.node_id, buffer)
    }
}

/// Context for [`super::NodeRuntime::produce`] and [`super::NodeRuntime::consume`].
///
/// Demand-style reads go through [`TickResolver`] (typically [`crate::dataflow::resolver::SessionHostResolver`]).
pub struct TickContext<'r> {
    node_id: NodeId,
    revision: Revision,
    resolver: &'r mut dyn TickResolver,
    slot_shapes: &'r SlotShapeRegistry,
    graphics: Option<Arc<dyn LpGraphics>>,
    time_provider: Option<Rc<dyn TimeProvider>>,
    button_service: Option<Rc<dyn ButtonService>>,
    radio_service: Option<Rc<dyn RadioService>>,
    frame_time_seconds: f32,
}

impl<'r> TickContext<'r> {
    pub fn new(
        node_id: NodeId,
        frame_id: Revision,
        resolver: &'r mut dyn TickResolver,
        slot_shapes: &'r SlotShapeRegistry,
    ) -> Self {
        Self::with_render_services(node_id, frame_id, resolver, slot_shapes, None, None, 0.0)
    }

    /// [`TickContext`] with graphics and frame time.
    pub fn with_render_services(
        node_id: NodeId,
        frame_id: Revision,
        resolver: &'r mut dyn TickResolver,
        slot_shapes: &'r SlotShapeRegistry,
        graphics: Option<Arc<dyn LpGraphics>>,
        time_provider: Option<Rc<dyn TimeProvider>>,
        frame_time_seconds: f32,
    ) -> Self {
        Self::with_engine_services(
            node_id,
            frame_id,
            resolver,
            slot_shapes,
            graphics,
            time_provider,
            None,
            None,
            frame_time_seconds,
        )
    }

    /// [`TickContext`] with graphics, time, and hardware input services.
    pub fn with_engine_services(
        node_id: NodeId,
        frame_id: Revision,
        resolver: &'r mut dyn TickResolver,
        slot_shapes: &'r SlotShapeRegistry,
        graphics: Option<Arc<dyn LpGraphics>>,
        time_provider: Option<Rc<dyn TimeProvider>>,
        button_service: Option<Rc<dyn ButtonService>>,
        radio_service: Option<Rc<dyn RadioService>>,
        frame_time_seconds: f32,
    ) -> Self {
        Self {
            node_id,
            revision: frame_id,
            resolver,
            slot_shapes,
            graphics,
            time_provider,
            button_service,
            radio_service,
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

    pub fn publish_runtime_slot(
        &mut self,
        state_root: &dyn SlotAccess,
        slot: SlotPath,
    ) -> Result<(), NodeError> {
        let (data, shape) = lookup_slot_data_and_shape(state_root, self.slot_shapes, &slot)
            .map_err(|e| NodeError::msg(alloc::format!("runtime slot lookup {slot}: {e}")))?;
        let snapshot = lpc_wire::snapshot_slot_shape(shape, data, self.slot_shapes);
        let production = Production::new(
            snapshot,
            ProductionSource::ProducedSlot {
                node: self.node_id,
                slot: slot.clone(),
            },
        );
        self.resolver
            .publish_produced_slot(self.node_id, slot, production)
            .map_err(|e| NodeError::msg(alloc::format!("publish runtime slot: {}", e.message)))
    }

    /// Resolve one of this node's consumed slots and parse it as a typed model value.
    pub fn resolve_consumed_slot_value<T>(&mut self, slot: &SlotPath) -> Result<T, NodeError>
    where
        T: FromLpValue,
    {
        let production = self
            .resolve(QueryKey::ConsumedSlot {
                node: self.node_id,
                slot: slot.clone(),
            })
            .map_err(|e| NodeError::msg(alloc::format!("resolve consumed slot {slot}: {e:?}")))?;
        let value = production
            .value_leaf()
            .ok_or_else(|| NodeError::msg("resolved slot is not a value"))?;
        T::from_lp_value(value.value()).map_err(|e| {
            NodeError::msg(alloc::format!(
                "consumed slot {slot} has incompatible value: {e}"
            ))
        })
    }

    /// Resolve one of this node's consumed slots through a compiled accessor.
    pub fn resolve_consumed_slot_accessor_value<T>(
        &mut self,
        accessor: &SlotAccessor,
    ) -> Result<T, NodeError>
    where
        T: FromLpValue,
    {
        let production = self
            .resolve(QueryKey::ConsumedSlotAccessor {
                node: self.node_id,
                accessor: accessor.clone(),
            })
            .map_err(|e| {
                NodeError::msg(alloc::format!(
                    "resolve consumed slot {}: {e:?}",
                    accessor.path()
                ))
            })?;
        let value = production
            .value_leaf()
            .ok_or_else(|| NodeError::msg("resolved slot is not a value"))?;
        T::from_lp_value(value.value()).map_err(|e| {
            NodeError::msg(alloc::format!(
                "consumed slot {} has incompatible value: {e}",
                accessor.path()
            ))
        })
    }

    pub fn slot_shapes(&self) -> &SlotShapeRegistry {
        self.slot_shapes
    }

    /// Monotonic shader time in seconds for the current engine frame.
    pub fn time_seconds(&self) -> f32 {
        self.frame_time_seconds
    }

    /// Graphics backend for shader compile and output buffers, when the engine has one installed.
    pub fn graphics(&self) -> Option<&dyn LpGraphics> {
        self.graphics.as_ref().map(|g| g.as_ref())
    }

    pub fn now_ms(&self) -> Option<u64> {
        self.time_provider
            .as_ref()
            .map(|provider| provider.now_ms())
    }

    pub fn elapsed_ms(&self, start_ms: u64) -> Option<u64> {
        self.time_provider
            .as_ref()
            .map(|provider| provider.elapsed_ms(start_ms))
    }

    pub fn button_service(&self) -> Option<Rc<dyn ButtonService>> {
        self.button_service.clone()
    }

    pub fn radio_service(&self) -> Option<Rc<dyn RadioService>> {
        self.radio_service.clone()
    }

    /// Materializes a visual product into a full texture through the active engine session.
    pub fn render_texture(
        &mut self,
        product: VisualProduct,
        request: &RenderTextureRequest,
    ) -> Result<TextureRenderProduct, NodeError> {
        self.resolver
            .render_texture(product, request)
            .map_err(|e| NodeError::msg(alloc::format!("render texture: {}", e.message)))
    }

    /// Renders a control product into an output-owned target through the active engine session.
    pub fn render_control(
        &mut self,
        product: ControlProduct,
        request: &ControlRenderRequest,
        target: ControlRenderTarget<'_>,
    ) -> Result<ControlLayout, NodeError> {
        self.resolver
            .render_control(product, request, target)
            .map_err(|e| NodeError::msg(alloc::format!("render control: {}", e.message)))
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

impl lpc_model::SlotReadContext for TickContext<'_> {
    type Error = NodeError;

    fn read_slot_value<T>(&mut self, accessor: &SlotAccessor) -> Result<T, Self::Error>
    where
        T: FromLpValue,
    {
        self.resolve_consumed_slot_accessor_value(accessor)
    }

    fn is_optional_none_error(error: &Self::Error) -> bool {
        match error {
            NodeError::Message(message) => message.contains("option slot is none"),
        }
    }
}

/// Context passed to [`super::ControlNode`] materialization hooks.
pub struct ControlRenderContext<'a> {
    node_id: NodeId,
    revision: Revision,
    graphics: Option<Arc<dyn LpGraphics>>,
    frame_time_seconds: f32,
    services: &'a mut dyn ControlRenderServices,
}

impl<'a> ControlRenderContext<'a> {
    pub fn new(
        node_id: NodeId,
        revision: Revision,
        graphics: Option<Arc<dyn LpGraphics>>,
        frame_time_seconds: f32,
        services: &'a mut dyn ControlRenderServices,
    ) -> Self {
        Self {
            node_id,
            revision,
            graphics,
            frame_time_seconds,
            services,
        }
    }

    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    pub fn revision(&self) -> Revision {
        self.revision
    }

    pub fn graphics(&self) -> Option<&dyn LpGraphics> {
        self.graphics.as_ref().map(|g| g.as_ref())
    }

    pub fn time_seconds(&self) -> f32 {
        self.frame_time_seconds
    }

    pub fn render_texture(
        &mut self,
        product: VisualProduct,
        request: &RenderTextureRequest,
    ) -> Result<TextureRenderProduct, NodeError> {
        self.services.render_texture(product, request)
    }

    pub fn render_texture_into(
        &mut self,
        product: VisualProduct,
        request: &RenderTextureRequest,
        target: &mut lp_shader::LpsTextureBuf,
    ) -> Result<(), NodeError> {
        self.services.render_texture_into(product, request, target)
    }

    pub fn sample_visual_into(
        &mut self,
        product: VisualProduct,
        request: VisualSampleBufferRequest<'_>,
        target: VisualSampleTarget<'_>,
    ) -> Result<(), NodeError> {
        self.services.sample_visual_into(product, request, target)
    }
}

/// Services available while materializing a [`crate::products::control::ControlProduct`].
pub trait ControlRenderServices {
    fn render_texture(
        &mut self,
        product: VisualProduct,
        request: &RenderTextureRequest,
    ) -> Result<TextureRenderProduct, NodeError>;

    fn render_texture_into(
        &mut self,
        product: VisualProduct,
        request: &RenderTextureRequest,
        target: &mut lp_shader::LpsTextureBuf,
    ) -> Result<(), NodeError>;

    fn sample_visual_into(
        &mut self,
        product: VisualProduct,
        request: VisualSampleBufferRequest<'_>,
        target: VisualSampleTarget<'_>,
    ) -> Result<(), NodeError>;
}

/// Services available while materializing a [`crate::products::visual::VisualProduct`].
pub trait VisualRenderServices {
    fn render_texture(
        &mut self,
        product: VisualProduct,
        request: &RenderTextureRequest,
    ) -> Result<TextureRenderProduct, NodeError>;

    fn render_texture_into(
        &mut self,
        product: VisualProduct,
        request: &RenderTextureRequest,
        target: &mut lp_shader::LpsTextureBuf,
    ) -> Result<(), NodeError>;

    fn sample_visual_into(
        &mut self,
        product: VisualProduct,
        request: VisualSampleBufferRequest<'_>,
        target: VisualSampleTarget<'_>,
    ) -> Result<(), NodeError>;
}

/// Context passed to [`super::RenderNode`] materialization hooks.
pub struct RenderContext<'a> {
    node_id: NodeId,
    revision: Revision,
    graphics: Option<Arc<dyn LpGraphics>>,
    time_provider: Option<Rc<dyn TimeProvider>>,
    frame_time_seconds: f32,
    services: Option<&'a mut dyn VisualRenderServices>,
}

impl<'a> RenderContext<'a> {
    pub fn new(
        node_id: NodeId,
        revision: Revision,
        graphics: Option<Arc<dyn LpGraphics>>,
        time_provider: Option<Rc<dyn TimeProvider>>,
        frame_time_seconds: f32,
    ) -> Self {
        Self {
            node_id,
            revision,
            graphics,
            time_provider,
            frame_time_seconds,
            services: None,
        }
    }

    pub fn with_services(
        node_id: NodeId,
        revision: Revision,
        graphics: Option<Arc<dyn LpGraphics>>,
        time_provider: Option<Rc<dyn TimeProvider>>,
        frame_time_seconds: f32,
        services: &'a mut dyn VisualRenderServices,
    ) -> Self {
        Self {
            node_id,
            revision,
            graphics,
            time_provider,
            frame_time_seconds,
            services: Some(services),
        }
    }

    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    pub fn revision(&self) -> Revision {
        self.revision
    }

    pub fn graphics(&self) -> Option<&dyn LpGraphics> {
        self.graphics.as_ref().map(|g| g.as_ref())
    }

    pub fn now_ms(&self) -> Option<u64> {
        self.time_provider
            .as_ref()
            .map(|provider| provider.now_ms())
    }

    pub fn elapsed_ms(&self, start_ms: u64) -> Option<u64> {
        self.time_provider
            .as_ref()
            .map(|provider| provider.elapsed_ms(start_ms))
    }

    pub fn time_seconds(&self) -> f32 {
        self.frame_time_seconds
    }

    pub fn render_texture(
        &mut self,
        product: VisualProduct,
        request: &RenderTextureRequest,
    ) -> Result<TextureRenderProduct, NodeError> {
        self.services
            .as_mut()
            .ok_or_else(|| NodeError::msg("render context has no visual render services"))?
            .render_texture(product, request)
    }

    pub fn render_texture_into(
        &mut self,
        product: VisualProduct,
        request: &RenderTextureRequest,
        target: &mut lp_shader::LpsTextureBuf,
    ) -> Result<(), NodeError> {
        self.services
            .as_mut()
            .ok_or_else(|| NodeError::msg("render context has no visual render services"))?
            .render_texture_into(product, request, target)
    }

    pub fn sample_visual_into(
        &mut self,
        product: VisualProduct,
        request: VisualSampleBufferRequest<'_>,
        target: VisualSampleTarget<'_>,
    ) -> Result<(), NodeError> {
        self.services
            .as_mut()
            .ok_or_else(|| NodeError::msg("render context has no visual render services"))?
            .sample_visual_into(product, request, target)
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
    use crate::dataflow::binding::{
        BindingDraft, BindingEntry, BindingPriority, BindingRef, BindingSource, BindingTarget,
    };
    use crate::dataflow::resolver::resolve_trace::ResolveLogLevel;
    use crate::dataflow::resolver::{
        Production, QueryKey, ResolveHost, ResolveSession, ResolveTrace, Resolver,
        SessionHostResolver, TickResolver,
    };
    use crate::node::{NodeRuntime, RuntimeStateShape};
    use alloc::string::String;
    use alloc::vec::Vec;
    use lpc_model::{Kind, LpValue, SlotPath, SlotShapeRegistry, Slotted, ValueSlot};

    #[derive(Default, Slotted)]
    struct TestRuntimeState {
        #[slot(produced)]
        pub value: ValueSlot<f32>,
    }

    #[derive(Default)]
    struct TestBindings {
        entries: Vec<(BindingRef, BindingEntry)>,
    }

    impl TestBindings {
        fn add(&mut self, draft: BindingDraft, revision: Revision) {
            let binding_ref = BindingRef::new(draft.owner, self.entries.len());
            self.entries.push((
                binding_ref,
                BindingEntry {
                    source: draft.source,
                    target: draft.target,
                    priority: draft.priority,
                    kind: draft.kind,
                    version: revision,
                    owner: draft.owner,
                },
            ));
        }

        fn binding_for_consumed_slot(
            &self,
            node: NodeId,
            slot: &SlotPath,
        ) -> Option<(BindingRef, BindingEntry)> {
            self.entries.iter().find_map(|(binding_ref, entry)| {
                matches!(
                    &entry.target,
                    BindingTarget::ConsumedSlot { node: n, slot: p } if *n == node && p == slot
                )
                .then(|| (*binding_ref, entry.clone()))
            })
        }

        fn providers_for_bus(
            &self,
            channel: &lpc_model::ChannelName,
        ) -> Vec<(BindingRef, BindingEntry)> {
            self.entries
                .iter()
                .filter_map(|(binding_ref, entry)| {
                    matches!(&entry.target, BindingTarget::BusChannel(c) if c == channel)
                        .then(|| (*binding_ref, entry.clone()))
                })
                .collect()
        }
    }

    #[derive(Default)]
    struct PanicProduceHost {
        bindings: TestBindings,
    }

    impl ResolveHost for PanicProduceHost {
        fn produce(
            &mut self,
            _query: &QueryKey,
            _session: &mut ResolveSession<'_>,
        ) -> Result<Production, crate::dataflow::resolver::SessionResolveError> {
            Err(crate::dataflow::resolver::SessionResolveError::other(
                "unexpected produce in TickContext test",
            ))
        }

        fn binding_for_consumed_slot(
            &self,
            node: NodeId,
            slot: &SlotPath,
        ) -> Option<(BindingRef, BindingEntry)> {
            self.bindings.binding_for_consumed_slot(node, slot)
        }

        fn providers_for_bus(
            &self,
            channel: &lpc_model::ChannelName,
        ) -> Vec<(BindingRef, BindingEntry)> {
            self.bindings.providers_for_bus(channel)
        }
    }

    fn session_bundle(resolver: &mut Resolver, frame: Revision) -> ResolveSession<'_> {
        ResolveSession::new(frame, resolver, ResolveTrace::new(ResolveLogLevel::Off))
    }

    #[test]
    fn tick_context_accessors() {
        let mut resolver = Resolver::new();
        let frame = Revision::new(10);
        let mut session = session_bundle(&mut resolver, frame);
        let mut host = PanicProduceHost::default();
        let slot_shapes = SlotShapeRegistry::default();

        let mut bridge = SessionHostResolver {
            session: &mut session,
            host: &mut host,
        };
        let ctx = TickContext::new(
            NodeId::new(7),
            Revision::new(3),
            &mut bridge as &mut dyn TickResolver,
            &slot_shapes,
        );

        assert_eq!(ctx.node_id(), NodeId::new(7));
        assert_eq!(ctx.revision(), Revision::new(3));
    }

    #[test]
    fn tick_context_resolve_bus_query() {
        let mut bindings = TestBindings::default();
        let frame = Revision::new(10);
        let channel = lpc_model::ChannelName(String::from("level_bus"));
        bindings.add(
            BindingDraft {
                source: BindingSource::Literal(lpc_model::LpValue::F32(7.8)),
                target: BindingTarget::BusChannel(channel.clone()),
                priority: BindingPriority::new(0),
                kind: lpc_model::Kind::Amplitude,
                owner: NodeId::new(1),
            },
            frame,
        );

        let mut resolver = Resolver::new();
        let mut session = session_bundle(&mut resolver, frame);
        let mut host = PanicProduceHost { bindings };
        let slot_shapes = SlotShapeRegistry::default();
        let mut bridge = SessionHostResolver {
            session: &mut session,
            host: &mut host,
        };
        let mut ctx = TickContext::new(
            NodeId::new(1),
            frame,
            &mut bridge as &mut dyn TickResolver,
            &slot_shapes,
        );
        let pv = ctx
            .resolve(QueryKey::Bus(channel.clone()))
            .expect("resolve bus");
        assert!(pv.as_value().expect("value").eq(&LpsValueF32::F32(7.8)));
    }

    #[test]
    fn tick_context_resolve_consumed_slot_query() {
        let mut bindings = TestBindings::default();
        let frame = Revision::new(10);
        let node = NodeId::new(3);
        let input = SlotPath::parse("in").unwrap();
        bindings.add(
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
        );

        let mut resolver = Resolver::new();
        let mut session = session_bundle(&mut resolver, frame);
        let mut host = PanicProduceHost { bindings };
        let slot_shapes = SlotShapeRegistry::default();
        let mut bridge = SessionHostResolver {
            session: &mut session,
            host: &mut host,
        };
        let mut ctx = TickContext::new(
            node,
            frame,
            &mut bridge as &mut dyn TickResolver,
            &slot_shapes,
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
    fn tick_context_publish_runtime_slot_satisfies_same_frame_cache() {
        let node = NodeId::new(7);
        let frame = Revision::new(10);
        let mut resolver = Resolver::new();
        let mut session = session_bundle(&mut resolver, frame);
        let mut host = PanicProduceHost::default();
        let mut slot_shapes = SlotShapeRegistry::default();
        TestRuntimeState::register_runtime_state_shape(&mut slot_shapes).expect("state shape");
        let mut bridge = SessionHostResolver {
            session: &mut session,
            host: &mut host,
        };
        let mut ctx = TickContext::new(
            node,
            frame,
            &mut bridge as &mut dyn TickResolver,
            &slot_shapes,
        );
        let state = TestRuntimeState {
            value: ValueSlot::new(3.5),
        };
        let slot = SlotPath::parse("value").expect("value slot");

        ctx.publish_runtime_slot(&state, slot.clone())
            .expect("publish");
        let production = ctx
            .resolve(QueryKey::ProducedSlot { node, slot })
            .expect("resolve published slot");

        assert_eq!(
            *production.value_leaf().expect("leaf").value(),
            LpValue::F32(3.5)
        );
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
        ) -> Result<Production, crate::dataflow::resolver::SessionResolveError> {
            match query {
                QueryKey::ConsumedSlot { node, slot }
                    if *node == self.node && *slot == self.out_path =>
                {
                    Ok(Production::value(
                        lpc_model::WithRevision::new(session.revision(), LpsValueF32::F32(11.0)),
                        crate::dataflow::resolver::ProductionSource::Default,
                    )?)
                }
                _ => Err(crate::dataflow::resolver::SessionResolveError::other(
                    "fixture produce mismatch",
                )),
            }
        }
    }

    /// Dummy node that uses [`TickContext::resolve`](TickContext::resolve) from [`super::super::NodeRuntime::produce`].
    struct QueryResolvingNode {
        query: QueryKey,
        resolved_value: Option<f32>,
    }

    impl super::super::NodeRuntime for QueryResolvingNode {
        fn produce(
            &mut self,
            _slot: &SlotPath,
            ctx: &mut TickContext<'_>,
        ) -> Result<super::super::ProduceResult, crate::node::NodeError> {
            let pv = ctx.resolve(self.query.clone()).map_err(|e| {
                crate::node::NodeError::msg(alloc::format!("resolve failed: {}", e.message))
            })?;
            if let LpsValueF32::F32(v) = pv.as_value().expect("value") {
                self.resolved_value = Some(v);
            }
            Ok(super::super::ProduceResult::Produced)
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
    }

    #[test]
    fn dummy_node_can_resolve_bus_query_from_produce() {
        let mut bindings = TestBindings::default();
        let frame = Revision::new(10);
        let channel = lpc_model::ChannelName(String::from("in"));
        bindings.add(
            BindingDraft {
                source: BindingSource::Literal(lpc_model::LpValue::F32(8.8)),
                target: BindingTarget::BusChannel(channel.clone()),
                priority: BindingPriority::new(0),
                kind: Kind::Amplitude,
                owner: NodeId::new(2),
            },
            frame,
        );

        let mut resolver = Resolver::new();
        let mut session = session_bundle(&mut resolver, frame);
        let mut host = PanicProduceHost { bindings };
        let slot_shapes = SlotShapeRegistry::default();

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
            &mut bridge as &mut dyn TickResolver,
            &slot_shapes,
        );

        node.produce(&SlotPath::root(), &mut ctx)
            .expect("produce should succeed");
        assert_eq!(node.resolved_value, Some(8.8));
    }

    #[test]
    fn dummy_node_can_resolve_consumed_slot_via_host_from_produce() {
        let frame = Revision::new(10);
        let node_id = NodeId::new(2);
        let input_path = SlotPath::parse("fixture_in").unwrap();

        let mut resolver = Resolver::new();
        let mut session = session_bundle(&mut resolver, frame);
        let mut host = FixtureProduceHost {
            node: node_id,
            out_path: input_path.clone(),
        };
        let slot_shapes = SlotShapeRegistry::default();

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
            &mut bridge as &mut dyn TickResolver,
            &slot_shapes,
        );

        node.produce(&SlotPath::root(), &mut ctx)
            .expect("produce should succeed");
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
