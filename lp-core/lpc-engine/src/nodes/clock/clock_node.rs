use alloc::format;

use lpc_model::{
    ClockDef, ClockState, NodeId, SlotAccess, SlotAccessor, SlotPath, SlotShapeRegistry,
    SlotShapeRegistryError, StaticSlotShape,
};

use crate::node::{
    DestroyCtx, MemPressureCtx, NodeError, NodeRuntime, PressureLevel, ProduceResult, TickContext,
};

/// Runtime clock node producing project time as ordinary slot data.
pub struct ClockNode {
    node_id: NodeId,
    state: ClockState,
    accessors: Option<ClockAccessors>,
    accumulated_seconds: f32,
    last_engine_seconds: Option<f32>,
}

impl ClockNode {
    pub fn new(node_id: NodeId) -> Self {
        Self {
            node_id,
            state: ClockState::default(),
            accessors: None,
            accumulated_seconds: 0.0,
            last_engine_seconds: None,
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

        self.state.seconds.set_with_version(
            ctx.revision(),
            self.accumulated_seconds + scrub_offset_seconds,
        );
        self.state
            .delta_seconds
            .set_with_version(ctx.revision(), clock_delta);
        Ok(())
    }
}

impl NodeRuntime for ClockNode {
    fn produce(
        &mut self,
        _slot: &SlotPath,
        ctx: &mut TickContext<'_>,
    ) -> Result<ProduceResult, NodeError> {
        let _ = self.node_id;
        self.update_from_controls(ctx)?;
        Ok(ProduceResult::Produced)
    }

    fn destroy(&mut self, _ctx: &mut DestroyCtx<'_>) -> Result<(), NodeError> {
        Ok(())
    }

    fn handle_memory_pressure(
        &mut self,
        _level: PressureLevel,
        _ctx: &mut MemPressureCtx<'_>,
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
        ClockState::ensure_registered(registry).map(|_| ())
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
            running: compile_clock_accessor("controls.running", registry)?,
            rate: compile_clock_accessor("controls.rate", registry)?,
            scrub_offset_seconds: compile_clock_accessor(
                "controls.scrub_offset_seconds",
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
