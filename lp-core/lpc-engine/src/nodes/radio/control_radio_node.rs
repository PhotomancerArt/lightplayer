//! Runtime control radio node: mirrors control events between a graph bus and radio channel.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::vec::Vec;

use lpc_hardware::{RadioChannelId, RadioConfig, RadioDevice, RadioMessage, RadioMessageKind};
use lpc_model::{
    ControlMessage, ControlRadioDefView, ControlRadioState, FromLpValue, HardwareEndpointSpec,
    MapSlot, SlotAccess, SlotData, SlotPath, SlotShapeRegistry, SlotShapeRegistryError,
};

use crate::dataflow::resolver::QueryKey;
use crate::node::{
    DestroyCtx, MemPressureCtx, NodeError, NodeRuntime, PressureLevel, ProduceResult,
    RuntimeStateShape, TickContext,
};

const CONTROL_MESSAGE_PAYLOAD_LEN: usize = 8;
const MAX_REPEAT_COUNT: u32 = 8;
const RECENT_MESSAGE_LIMIT: usize = 32;

/// Runtime node for `kind = "ControlRadio"` artifacts.
pub struct ControlRadioNode {
    state: ControlRadioState,
    def_view: Option<ControlRadioDefView>,
    device: Option<Box<dyn RadioDevice>>,
    opened: Option<OpenedRadio>,
    pending: Vec<PendingControlMessage>,
    recent_sent: Vec<ControlMessageKey>,
    recent_received: Vec<ControlMessageKey>,
    receive_buffer: Vec<RadioMessage>,
}

impl ControlRadioNode {
    pub fn new() -> Self {
        Self {
            state: ControlRadioState::default(),
            def_view: None,
            device: None,
            opened: None,
            pending: Vec::new(),
            recent_sent: Vec::new(),
            recent_received: Vec::new(),
            receive_buffer: Vec::new(),
        }
    }

    fn read_config(
        &mut self,
        ctx: &mut TickContext<'_>,
    ) -> Result<ControlRadioRuntimeConfig, NodeError> {
        let def = ControlRadioDefView::get_or_compile(&mut self.def_view, ctx.slot_shapes())
            .map_err(|e| NodeError::msg(format!("compile control radio def view: {e}")))?;
        let wifi_channel = def.wifi_channel().get::<_, u32>(ctx)?;
        if wifi_channel > u32::from(u8::MAX) {
            return Err(NodeError::msg(format!(
                "control radio wifi_channel {wifi_channel} is outside u8 range"
            )));
        }
        Ok(ControlRadioRuntimeConfig {
            endpoint: def.endpoint().get(ctx)?,
            channel: RadioChannelId::new(def.channel().get::<_, u32>(ctx)?),
            repeat_count: def
                .repeat_count()
                .get::<_, u32>(ctx)?
                .clamp(1, MAX_REPEAT_COUNT),
            wifi_channel: if wifi_channel == 0 {
                None
            } else {
                Some(wifi_channel as u8)
            },
        })
    }

    fn ensure_radio(
        &mut self,
        config: &ControlRadioRuntimeConfig,
        ctx: &TickContext<'_>,
    ) -> Result<(), NodeError> {
        let opened = OpenedRadio {
            endpoint: config.endpoint.clone(),
            channel: config.channel,
            wifi_channel: config.wifi_channel,
        };
        if self.opened.as_ref() == Some(&opened) && self.device.is_some() {
            return Ok(());
        }

        let service = ctx
            .radio_service()
            .ok_or_else(|| NodeError::msg("control radio node has no radio service"))?;
        let mut device = service
            .open_radio_by_spec(&config.endpoint, RadioConfig::new(config.wifi_channel))
            .map_err(|error| {
                NodeError::msg(format!("open control radio {}: {error}", config.endpoint))
            })?;
        device
            .subscribe_channel(config.channel)
            .map_err(|error| NodeError::msg(format!("subscribe control radio channel: {error}")))?;
        self.device = Some(device);
        self.opened = Some(opened);
        self.pending.clear();
        self.receive_buffer.clear();
        Ok(())
    }

    fn accept_local_inputs(
        &mut self,
        ctx: &mut TickContext<'_>,
        repeat_count: u32,
        accepted: &mut BTreeMap<u32, ControlMessage>,
    ) -> Result<(), NodeError> {
        for message in resolve_input_messages(ctx)? {
            let key = ControlMessageKey::from(message);
            if self.has_seen(&key) || self.pending.iter().any(|pending| pending.key == key) {
                continue;
            }
            remember_key(&mut self.recent_sent, key);
            self.pending.push(PendingControlMessage {
                message,
                key,
                remaining: repeat_count,
            });
            accepted.insert(message.id(), message);
        }
        Ok(())
    }

    fn transmit_pending(&mut self, channel: RadioChannelId) -> Result<(), NodeError> {
        let device = self
            .device
            .as_mut()
            .ok_or_else(|| NodeError::msg("control radio missing after open"))?;
        let mut retained = Vec::new();
        for mut pending in self.pending.drain(..) {
            if pending.remaining > 0 {
                let payload = encode_control_message(pending.message);
                device
                    .send_channel(channel, RadioMessageKind::ControlMessage, &payload)
                    .map_err(|error| {
                        NodeError::msg(format!("send control radio message: {error}"))
                    })?;
                pending.remaining -= 1;
            }
            if pending.remaining > 0 {
                retained.push(pending);
            }
        }
        self.pending = retained;
        Ok(())
    }

    fn receive_remote(
        &mut self,
        channel: RadioChannelId,
        accepted: &mut BTreeMap<u32, ControlMessage>,
    ) -> Result<(), NodeError> {
        self.receive_buffer.clear();
        self.device
            .as_mut()
            .ok_or_else(|| NodeError::msg("control radio missing after open"))?
            .drain_channel(channel, &mut self.receive_buffer)
            .map_err(|error| NodeError::msg(format!("drain control radio channel: {error}")))?;

        for message in &self.receive_buffer {
            if message.kind() != RadioMessageKind::ControlMessage {
                continue;
            }
            let Some(control) = decode_control_message(message.payload())? else {
                continue;
            };
            let key = ControlMessageKey::from(control);
            if self.has_seen(&key) {
                continue;
            }
            remember_key(&mut self.recent_received, key);
            accepted.insert(control.id(), control);
        }
        Ok(())
    }

    fn publish_output(
        &mut self,
        ctx: &mut TickContext<'_>,
        accepted: BTreeMap<u32, ControlMessage>,
    ) -> Result<(), NodeError> {
        self.state.output = MapSlot::with_version(ctx.revision(), accepted);
        ctx.publish_runtime_slot(&self.state, control_radio_output_path())
    }

    fn current_frame_output(&self, revision: lpc_model::Revision) -> BTreeMap<u32, ControlMessage> {
        if self.state.output.keys_revision == revision {
            self.state.output.entries.clone()
        } else {
            BTreeMap::new()
        }
    }

    fn has_seen(&self, key: &ControlMessageKey) -> bool {
        self.recent_sent.contains(key) || self.recent_received.contains(key)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ControlRadioRuntimeConfig {
    endpoint: HardwareEndpointSpec,
    channel: RadioChannelId,
    repeat_count: u32,
    wifi_channel: Option<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct OpenedRadio {
    endpoint: HardwareEndpointSpec,
    channel: RadioChannelId,
    wifi_channel: Option<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ControlMessageKey {
    id: u32,
    seq: u32,
}

impl From<ControlMessage> for ControlMessageKey {
    fn from(message: ControlMessage) -> Self {
        Self {
            id: message.id(),
            seq: message.seq(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingControlMessage {
    message: ControlMessage,
    key: ControlMessageKey,
    remaining: u32,
}

impl NodeRuntime for ControlRadioNode {
    fn produce(
        &mut self,
        slot: &SlotPath,
        ctx: &mut TickContext<'_>,
    ) -> Result<ProduceResult, NodeError> {
        if slot != &control_radio_output_path() {
            return Ok(ProduceResult::Unsupported);
        }
        let config = self.read_config(ctx)?;
        self.ensure_radio(&config, ctx)?;

        let mut accepted = self.current_frame_output(ctx.revision());
        self.receive_remote(config.channel, &mut accepted)?;
        self.publish_output(ctx, accepted)?;
        Ok(ProduceResult::Produced)
    }

    fn consume(&mut self, ctx: &mut TickContext<'_>) -> Result<(), NodeError> {
        let config = self.read_config(ctx)?;
        self.ensure_radio(&config, ctx)?;

        let mut accepted = self.current_frame_output(ctx.revision());
        self.receive_remote(config.channel, &mut accepted)?;
        self.publish_output(ctx, accepted.clone())?;

        self.accept_local_inputs(ctx, config.repeat_count, &mut accepted)?;
        self.transmit_pending(config.channel)?;
        self.receive_remote(config.channel, &mut accepted)?;

        self.publish_output(ctx, accepted)?;
        Ok(())
    }

    fn destroy(&mut self, _ctx: &mut DestroyCtx<'_>) -> Result<(), NodeError> {
        self.device = None;
        self.opened = None;
        self.pending.clear();
        self.receive_buffer.clear();
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
        ControlRadioState::register_runtime_state_shape(registry).map(|_| ())
    }
}

fn resolve_input_messages(ctx: &mut TickContext<'_>) -> Result<Vec<ControlMessage>, NodeError> {
    let production = ctx
        .resolve(QueryKey::ConsumedSlot {
            node: ctx.node_id(),
            slot: control_radio_input_path(),
        })
        .map_err(|e| NodeError::msg(format!("resolve control radio input: {e:?}")))?;
    let SlotData::Map(map) = production.data() else {
        return Ok(Vec::new());
    };
    let mut messages = Vec::new();
    for data in map.entries.values() {
        let SlotData::Value(value) = data else {
            continue;
        };
        messages.push(
            ControlMessage::from_lp_value(value.value())
                .map_err(|e| NodeError::msg(format!("control radio input value: {e}")))?,
        );
    }
    Ok(messages)
}

fn encode_control_message(message: ControlMessage) -> [u8; CONTROL_MESSAGE_PAYLOAD_LEN] {
    let mut payload = [0; CONTROL_MESSAGE_PAYLOAD_LEN];
    payload[0..4].copy_from_slice(&message.id().to_le_bytes());
    payload[4..8].copy_from_slice(&message.seq().to_le_bytes());
    payload
}

fn decode_control_message(payload: &[u8]) -> Result<Option<ControlMessage>, NodeError> {
    if payload.is_empty() {
        return Ok(None);
    }
    if payload.len() != CONTROL_MESSAGE_PAYLOAD_LEN {
        return Err(NodeError::msg(format!(
            "control radio payload length {} is invalid",
            payload.len()
        )));
    }
    Ok(Some(ControlMessage::new(
        u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]),
        u32::from_le_bytes([payload[4], payload[5], payload[6], payload[7]]),
    )))
}

fn remember_key(recent: &mut Vec<ControlMessageKey>, key: ControlMessageKey) {
    if recent.contains(&key) {
        return;
    }
    if recent.len() >= RECENT_MESSAGE_LIMIT {
        recent.remove(0);
    }
    recent.push(key);
}

pub fn control_radio_input_path() -> SlotPath {
    SlotPath::parse("input").expect("control radio input path")
}

pub fn control_radio_output_path() -> SlotPath {
    SlotPath::parse("output").expect("control radio output path")
}
