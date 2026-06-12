//! Runtime hardware button node: polls a debounced input and produces control maps.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::format;

use lpc_hardware::{ButtonConfig, ButtonEventKind, ButtonInput};
use lpc_model::{
    ButtonDefView, ButtonState, ControlMessage, HwEndpointSpec, MapSlot, Revision, SlotAccess,
    SlotPath, SlotShapeRegistry, SlotShapeRegistryError,
};

use crate::node::{
    DestroyCtx, MemPressureCtx, NodeError, NodeRuntime, PressureLevel, ProduceResult,
    RuntimeStateShape, TickContext,
};

/// Runtime node for `kind = "Button"` artifacts.
pub struct ButtonNode {
    state: ButtonState,
    def_view: Option<ButtonDefView>,
    input: Option<Box<dyn ButtonInput>>,
    opened: Option<OpenedButton>,
    held_id_seq: Option<(u32, u32)>,
    fallback_now_ms: u64,
}

impl ButtonNode {
    pub fn new() -> Self {
        Self {
            state: ButtonState::default(),
            def_view: None,
            input: None,
            opened: None,
            held_id_seq: None,
            fallback_now_ms: 0,
        }
    }

    fn read_config(&mut self, ctx: &mut TickContext<'_>) -> Result<ButtonRuntimeConfig, NodeError> {
        let def = ButtonDefView::get_or_compile(&mut self.def_view, ctx.slot_shapes())
            .map_err(|e| NodeError::msg(format!("compile button def view: {e}")))?;
        Ok(ButtonRuntimeConfig {
            endpoint: def.endpoint().get(ctx)?,
            id: def.id().get::<_, u32>(ctx)?,
            stable_ms: u64::from(def.stable_ms().get::<_, u32>(ctx)?),
        })
    }

    fn ensure_input(
        &mut self,
        config: &ButtonRuntimeConfig,
        ctx: &TickContext<'_>,
    ) -> Result<(), NodeError> {
        let opened = OpenedButton {
            endpoint: config.endpoint.clone(),
            stable_ms: config.stable_ms,
        };
        if self.opened.as_ref() == Some(&opened) && self.input.is_some() {
            return Ok(());
        }

        let service = ctx
            .button_service()
            .ok_or_else(|| NodeError::msg("button node has no button service"))?;
        let input = service
            .open_button_by_spec(&config.endpoint, ButtonConfig::new(config.stable_ms))
            .map_err(|error| NodeError::msg(format!("open button {}: {error}", config.endpoint)))?;
        self.input = Some(input);
        self.opened = Some(opened);
        self.held_id_seq = None;
        Ok(())
    }

    fn next_now_ms(&mut self, ctx: &TickContext<'_>) -> u64 {
        if let Some(now_ms) = ctx.now_ms() {
            self.fallback_now_ms = now_ms;
            return now_ms;
        }
        self.fallback_now_ms = self.fallback_now_ms.saturating_add(1);
        self.fallback_now_ms
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ButtonRuntimeConfig {
    endpoint: HwEndpointSpec,
    id: u32,
    stable_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct OpenedButton {
    endpoint: HwEndpointSpec,
    stable_ms: u64,
}

impl NodeRuntime for ButtonNode {
    fn produce(
        &mut self,
        _slot: &SlotPath,
        ctx: &mut TickContext<'_>,
    ) -> Result<ProduceResult, NodeError> {
        let config = self.read_config(ctx)?;
        self.ensure_input(&config, ctx)?;
        let now_ms = self.next_now_ms(ctx);

        let mut down = MapSlot::default();
        let mut up = MapSlot::default();
        let mut held = self
            .held_id_seq
            .map(|(id, seq)| one_message_map(ctx.revision(), id, seq))
            .unwrap_or_default();

        if let Some(event) = self
            .input
            .as_mut()
            .ok_or_else(|| NodeError::msg("button input missing after open"))?
            .poll(now_ms)
        {
            let seq = event.sequence();
            match event.kind() {
                ButtonEventKind::Pressed => {
                    self.held_id_seq = Some((config.id, seq));
                    down = one_message_map(ctx.revision(), config.id, seq);
                    held = one_message_map(ctx.revision(), config.id, seq);
                }
                ButtonEventKind::Released => {
                    self.held_id_seq = None;
                    held = MapSlot::default();
                    up = one_message_map(ctx.revision(), config.id, seq);
                }
            }
        }

        self.state.down = down;
        self.state.held = held;
        self.state.up = up;
        Ok(ProduceResult::Produced)
    }

    fn destroy(&mut self, _ctx: &mut DestroyCtx<'_>) -> Result<(), NodeError> {
        self.input = None;
        self.opened = None;
        self.held_id_seq = None;
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
        ButtonState::register_runtime_state_shape(registry).map(|_| ())
    }
}

fn one_message_map(revision: Revision, id: u32, seq: u32) -> MapSlot<u32, ControlMessage> {
    let mut entries = BTreeMap::new();
    entries.insert(id, ControlMessage::new(id, seq));
    MapSlot::with_version(revision, entries)
}

pub fn button_down_path() -> SlotPath {
    SlotPath::parse("down").expect("button down path")
}

pub fn button_held_path() -> SlotPath {
    SlotPath::parse("held").expect("button held path")
}

pub fn button_up_path() -> SlotPath {
    SlotPath::parse("up").expect("button up path")
}
