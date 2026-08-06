use alloc::format;

use lpc_model::{
    ClockDef, ClockState, NodeId, SlotAccess, SlotAccessor, SlotPath, SlotShapeRegistry,
    SlotShapeRegistryError, StaticSlotShape, TimeProduct,
};

use crate::node::{
    DestroyCtx, MemPressureCtx, NodeError, NodeRuntime, PressureLevel, ProduceResult,
    RuntimeStateShape, TickContext,
};

/// Runtime clock node producing project time as ordinary slot data.
pub struct ClockNode {
    node_id: NodeId,
    state: ClockState,
    accessors: Option<ClockAccessors>,
    accumulated_seconds: f32,
    last_engine_seconds: Option<f32>,
    last_effective_seconds: Option<f32>,
}

impl ClockNode {
    pub fn new(node_id: NodeId) -> Self {
        Self {
            node_id,
            state: ClockState::for_node(node_id),
            accessors: None,
            accumulated_seconds: 0.0,
            last_engine_seconds: None,
            last_effective_seconds: None,
        }
    }

    fn update_from_controls(&mut self, ctx: &mut TickContext<'_>) -> Result<(), NodeError> {
        let accessors = ClockAccessors::get_or_compile(&mut self.accessors, ctx.slot_shapes())
            .map_err(|e| NodeError::msg(format!("compile clock controls view: {e}")))?;
        let running: bool = ctx.resolve_consumed_slot_accessor_value(&accessors.running)?;
        let rate: f32 = ctx.resolve_consumed_slot_accessor_value(&accessors.rate)?;
        let scrub_offset_seconds: f32 =
            ctx.resolve_consumed_slot_accessor_value(&accessors.scrub_offset_seconds)?;

        let now = ctx.time_seconds();
        let engine_delta = self
            .last_engine_seconds
            .map_or(0.0, |previous| (now - previous).max(0.0));
        self.last_engine_seconds = Some(now);

        let clock_delta = if running { engine_delta * rate } else { 0.0 };
        if running {
            self.accumulated_seconds += clock_delta;
        }

        let effective_seconds = self.accumulated_seconds + scrub_offset_seconds;
        // What the timebase advanced by, which is not the same thing as what
        // the clock ran by: dragging `scrub_offset` moves effective time
        // without any wall time passing, and a drag *backwards* has to arrive
        // as a negative delta or a device (which keeps no breakpoint log)
        // would sit still while its clock reads earlier and earlier. With the
        // offset held constant the two are identical, which is why an
        // ordinary running clock is unaffected by this.
        let effective_delta = self
            .last_effective_seconds
            .map_or(0.0, |previous| effective_seconds - previous);
        self.last_effective_seconds = Some(effective_seconds);
        // The published handle is constant for the life of the node; only its
        // revision moves, so readers of `bus:time` see a stable value while
        // the timebase behind it advances every tick.
        self.state
            .product
            .set_with_version(ctx.revision(), TimeProduct::new(self.node_id, 0));
        self.state
            .seconds
            .set_with_version(ctx.revision(), effective_seconds);
        self.state
            .delta_seconds
            .set_with_version(ctx.revision(), effective_delta);

        // The same two numbers, published where TimeProduct queries can
        // reach them without dispatching back into this node. The clock
        // stays the transformer on engine wall time (`ctx.time_seconds()`
        // is raw, and stays raw); this is only where its output lands.
        ctx.publish_timebase(effective_seconds, effective_delta);
        Ok(())
    }
}

impl NodeRuntime for ClockNode {
    fn produce(
        &mut self,
        _slot: &SlotPath,
        ctx: &mut TickContext<'_>,
    ) -> Result<ProduceResult, NodeError> {
        self.update_from_controls(ctx)?;
        Ok(ProduceResult::Produced)
    }

    fn destroy(&mut self, _ctx: &mut DestroyCtx) -> Result<(), NodeError> {
        Ok(())
    }

    fn handle_memory_pressure(
        &mut self,
        _level: PressureLevel,
        _ctx: &mut MemPressureCtx,
    ) -> Result<(), NodeError> {
        Ok(())
    }

    fn runtime_state_slots(&self) -> Option<&dyn SlotAccess> {
        Some(&self.state)
    }

    fn register_runtime_state_shapes(
        &self,
        registry: &mut SlotShapeRegistry,
    ) -> Result<(), SlotShapeRegistryError> {
        ClockState::register_runtime_state_shape(registry).map(|_| ())
    }
}

struct ClockAccessors {
    registry_revision: lpc_model::Revision,
    running: SlotAccessor,
    rate: SlotAccessor,
    scrub_offset_seconds: SlotAccessor,
}

impl ClockAccessors {
    fn compile(registry: &SlotShapeRegistry) -> Result<Self, lpc_model::SlotAccessorError> {
        Ok(Self {
            registry_revision: registry.revision(),
            running: compile_clock_accessor("transport.running", registry)?,
            rate: compile_clock_accessor("transport.rate", registry)?,
            scrub_offset_seconds: compile_clock_accessor(
                "transport.scrub_offset_seconds",
                registry,
            )?,
        })
    }

    fn get_or_compile<'a>(
        cache: &'a mut Option<Self>,
        registry: &SlotShapeRegistry,
    ) -> Result<&'a Self, lpc_model::SlotAccessorError> {
        let needs_compile = cache
            .as_ref()
            .is_none_or(|view| view.registry_revision != registry.revision());
        if needs_compile {
            *cache = Some(Self::compile(registry)?);
        }
        Ok(cache.as_ref().expect("clock accessors were just compiled"))
    }
}

fn compile_clock_accessor(
    path: &str,
    registry: &SlotShapeRegistry,
) -> Result<SlotAccessor, lpc_model::SlotAccessorError> {
    SlotAccessor::compile_value(
        ClockDef::SHAPE_ID,
        SlotPath::parse(path).expect("clock accessor path is valid"),
        registry,
    )
}

pub fn clock_seconds_path() -> SlotPath {
    SlotPath::parse("seconds").expect("clock seconds path")
}

/// The clock's time-product output — the slot `bus:time` is fed from.
pub fn clock_product_path() -> SlotPath {
    SlotPath::parse("product").expect("clock product path")
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::{String, ToString};
    use alloc::vec::Vec;
    use lpc_model::{LpValue, Revision, SlotShapeRegistry, WithRevision};

    use crate::dataflow::resolver::{
        Production, ProductionSource, QueryKey, ResolveError, TickResolver,
    };

    /// Serves the clock's three controls and records what it publishes.
    ///
    /// Clock controls are `Debug`-role slot data (transient — the project
    /// codec deliberately round-trips them to defaults), so authoring them
    /// in a project file cannot drive this seam. Feeding them straight to
    /// the node is how the rate/pause contract gets pinned.
    struct ControlsResolver {
        running: bool,
        rate: f32,
        scrub_offset_seconds: f32,
        published: Vec<(f32, f32)>,
    }

    impl ControlsResolver {
        fn new(running: bool, rate: f32) -> Self {
            Self {
                running,
                rate,
                scrub_offset_seconds: 0.0,
                published: Vec::new(),
            }
        }
    }

    impl TickResolver for ControlsResolver {
        fn resolve(&mut self, query: &QueryKey) -> Result<Production, ResolveError> {
            let path = query
                .consumed_slot_path()
                .ok_or_else(|| ResolveError::new(String::from("unexpected query kind")))?;
            let value = match path.to_string().as_str() {
                "transport.running" => LpValue::Bool(self.running),
                "transport.rate" => LpValue::F32(self.rate),
                "transport.scrub_offset_seconds" => LpValue::F32(self.scrub_offset_seconds),
                other => {
                    return Err(ResolveError::new(alloc::format!("no control {other}")));
                }
            };
            Ok(Production::leaf(
                WithRevision::new(Revision::new(1), value),
                ProductionSource::Literal,
            ))
        }

        fn resolve_static_consumed(
            &mut self,
            node: NodeId,
            path: &'static str,
        ) -> Result<Production, ResolveError> {
            let slot = SlotPath::parse(path)
                .map_err(|e| ResolveError::new(alloc::format!("bad static path: {e}")))?;
            self.resolve(&QueryKey::ConsumedSlot { node, slot })
        }

        fn publish_produced_slot(
            &mut self,
            _node: NodeId,
            _slot: SlotPath,
            _production: Production,
        ) -> Result<(), ResolveError> {
            Ok(())
        }

        fn render_texture(
            &mut self,
            _product: crate::products::visual::VisualProduct,
            _request: &crate::products::visual::RenderTextureRequest,
        ) -> Result<crate::products::visual::TextureRenderProduct, ResolveError> {
            Err(ResolveError::new(String::from("no render")))
        }

        fn render_control(
            &mut self,
            _product: crate::products::control::ControlProduct,
            _request: &crate::products::control::ControlRenderRequest,
            _target: crate::products::control::ControlRenderTarget<'_>,
        ) -> Result<crate::products::control::ControlLayout, ResolveError> {
            Err(ResolveError::new(String::from("no control render")))
        }

        fn runtime_buffer_mut(
            &mut self,
            _id: crate::resource::RuntimeBufferId,
            _frame: Revision,
        ) -> Result<&mut crate::resource::RuntimeBuffer, ResolveError> {
            Err(ResolveError::new(String::from("no buffers")))
        }

        fn publish_timebase(
            &mut self,
            _clock: NodeId,
            effective_seconds: f32,
            delta_seconds: f32,
            _at: Revision,
        ) {
            self.published.push((effective_seconds, delta_seconds));
        }
    }

    /// Tick the clock `frames` times at `step` seconds of engine wall time.
    fn run(resolver: &mut ControlsResolver, frames: usize, step: f32) -> Vec<(f32, f32)> {
        let shapes = SlotShapeRegistry::default();
        let mut node = ClockNode::new(NodeId::new(1));
        for frame in 0..frames {
            let mut ctx = TickContext::with_render_services(
                NodeId::new(1),
                Revision::new(frame as i64 + 1),
                resolver,
                &shapes,
                None,
                None,
                step * (frame as f32 + 1.0),
            );
            node.produce(&SlotPath::root(), &mut ctx).expect("produce");
        }
        core::mem::take(&mut resolver.published)
    }

    #[test]
    fn a_running_clock_publishes_seconds_and_delta() {
        let mut resolver = ControlsResolver::new(true, 1.0);

        let published = run(&mut resolver, 3, 0.5);

        // The first frame has no previous engine timestamp to subtract.
        assert_eq!(published.len(), 3);
        assert_eq!(published[0], (0.0, 0.0));
        assert_eq!(published[1], (0.5, 0.5));
        assert_eq!(published[2], (1.0, 0.5));
    }

    #[test]
    fn a_paused_clock_publishes_a_zero_delta_and_a_still_clock() {
        let mut resolver = ControlsResolver::new(false, 1.0);

        let published = run(&mut resolver, 4, 0.5);

        assert!(
            published
                .iter()
                .all(|&(seconds, delta)| seconds == 0.0 && delta == 0.0),
            "a paused clock must publish a frozen timebase: {published:?}"
        );
    }

    #[test]
    fn clock_rate_scales_the_published_delta() {
        // Global speed is sacred: the rate multiplies the delta every phasor
        // rides on, so a 2× clock advances every phase 2× — no per-phasor
        // rate anywhere.
        let mut single = ControlsResolver::new(true, 1.0);
        let mut double = ControlsResolver::new(true, 2.0);

        let at_one = run(&mut single, 3, 0.5);
        let at_two = run(&mut double, 3, 0.5);

        for (one, two) in at_one.iter().zip(at_two.iter()) {
            assert_eq!(two.1, 2.0 * one.1, "delta: {one:?} vs {two:?}");
            assert_eq!(two.0, 2.0 * one.0, "seconds: {one:?} vs {two:?}");
        }
    }

    #[test]
    fn the_scrub_offset_shifts_seconds_without_touching_delta() {
        let mut resolver = ControlsResolver::new(true, 1.0);
        resolver.scrub_offset_seconds = 10.0;

        let published = run(&mut resolver, 3, 0.5);

        assert_eq!(published[0], (10.0, 0.0));
        assert_eq!(published[2], (11.0, 0.5));
    }

    /// Dragging the scrub offset backwards publishes a NEGATIVE delta.
    ///
    /// This is the device half of parent D6: firmware keeps no breakpoint log
    /// (`scrub-log` is off there), so the only way a backward scrub can reach
    /// a phasor at all is as a negative advance it integrates through. A
    /// clock that published only forward wall time would leave a scrubbed
    /// device showing an earlier clock and an unchanged animation.
    #[test]
    fn a_backward_scrub_publishes_a_negative_delta() {
        let shapes = SlotShapeRegistry::default();
        let mut resolver = ControlsResolver::new(true, 1.0);
        let mut node = ClockNode::new(NodeId::new(1));
        let run_frame = |node: &mut ClockNode, resolver: &mut ControlsResolver, frame: i64| {
            let mut ctx = TickContext::with_render_services(
                NodeId::new(1),
                Revision::new(frame),
                resolver,
                &shapes,
                None,
                None,
                0.5 * frame as f32,
            );
            node.produce(&SlotPath::root(), &mut ctx).expect("produce");
        };

        for frame in 1..=3 {
            run_frame(&mut node, &mut resolver, frame);
        }
        let (before, _) = *resolver.published.last().expect("published");

        resolver.scrub_offset_seconds = -0.75;
        run_frame(&mut node, &mut resolver, 4);

        let (seconds, delta) = *resolver.published.last().expect("published");
        assert!(
            seconds < before,
            "the clock reads earlier: {before} -> {seconds}"
        );
        assert!(
            (delta - (seconds - before)).abs() < 1e-6 && delta < 0.0,
            "the published delta is the change in effective time: {delta}"
        );
    }

    #[test]
    fn the_published_timebase_matches_the_produced_slots() {
        let mut resolver = ControlsResolver::new(true, 1.5);
        let shapes = SlotShapeRegistry::default();
        let mut node = ClockNode::new(NodeId::new(1));

        for frame in 0..3 {
            let mut ctx = TickContext::with_render_services(
                NodeId::new(1),
                Revision::new(frame + 1),
                &mut resolver,
                &shapes,
                None,
                None,
                0.25 * (frame as f32 + 1.0),
            );
            node.produce(&SlotPath::root(), &mut ctx).expect("produce");
        }

        let published = resolver.published.last().copied().expect("published");
        assert_eq!(published.0, *node.state.seconds.value());
        assert_eq!(published.1, *node.state.delta_seconds.value());
    }
}
