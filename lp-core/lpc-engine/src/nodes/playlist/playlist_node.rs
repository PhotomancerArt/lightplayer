//! Runtime playlist node: selects and blends owned visual child entries.

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::vec::Vec;

use lpc_model::{
    ControlMessage, FromLpValue, NodeId, PlaylistState, SlotAccess, SlotData, SlotPath,
    SlotShapeRegistry, SlotShapeRegistryError,
};
use lps_shared::{TextureBuffer, TextureStorageFormat};

use crate::dataflow::resolver::QueryKey;
use crate::node::{
    DestroyCtx, MemPressureCtx, NodeError, NodeRuntime, PressureLevel, ProduceResult,
    RenderContext, RenderNode, RuntimeStateShape, TickContext,
};
use crate::products::visual::{
    RenderTextureRequest, TextureRenderProduct, VisualSampleBufferRequest, VisualSampleTarget,
};

#[derive(Clone, Debug, PartialEq)]
pub struct PlaylistRuntimeEntry {
    pub index: u32,
    pub child: NodeId,
    pub output_slot: SlotPath,
    pub duration: Option<f32>,
    pub fade_after: Option<f32>,
}

pub struct PlaylistNode {
    idle_entry: u32,
    default_fade: f32,
    entries: Vec<PlaylistRuntimeEntry>,
    state: PlaylistState,
    current_entry: u32,
    previous_entry: Option<u32>,
    previous_product: Option<lpc_model::VisualProduct>,
    active_product: Option<lpc_model::VisualProduct>,
    switch_time: f32,
    transition_start_time: f32,
    transition_duration: f32,
    last_seen_triggers: BTreeMap<(u32, u32), u32>,
}

impl PlaylistNode {
    pub fn new(
        node_id: NodeId,
        idle_entry: u32,
        default_fade: f32,
        entries: Vec<PlaylistRuntimeEntry>,
    ) -> Self {
        Self {
            idle_entry,
            default_fade,
            entries,
            state: PlaylistState::new(
                lpc_model::VisualProduct::new(node_id, 0),
                0.0,
                -1.0,
                idle_entry,
            ),
            current_entry: idle_entry,
            previous_entry: None,
            previous_product: None,
            active_product: None,
            switch_time: 0.0,
            transition_start_time: 0.0,
            transition_duration: 0.0,
            last_seen_triggers: BTreeMap::new(),
        }
    }

    fn runtime_entry(&self, index: u32) -> Option<&PlaylistRuntimeEntry> {
        self.entries.iter().find(|entry| entry.index == index)
    }

    fn fade_after(&self, index: u32) -> f32 {
        self.runtime_entry(index)
            .and_then(|entry| entry.fade_after)
            .unwrap_or(self.default_fade)
    }

    fn duration(&self, index: u32) -> Option<f32> {
        self.runtime_entry(index).and_then(|entry| entry.duration)
    }

    fn next_entry_after(&self, index: u32) -> Option<u32> {
        self.entries
            .iter()
            .map(|entry| entry.index)
            .filter(|candidate| *candidate > index)
            .min()
    }

    fn switch_to(&mut self, entry: u32, time: f32) {
        let leaving = self.current_entry;
        let fade = if leaving == entry {
            0.0
        } else {
            self.fade_after(leaving)
        };
        self.previous_entry = (fade > 0.0).then_some(leaving);
        self.previous_product = (fade > 0.0).then_some(self.active_product).flatten();
        self.transition_start_time = time;
        self.transition_duration = fade;
        self.current_entry = entry;
        self.switch_time = time;
    }

    fn transition_alpha(&self, time: f32) -> Option<f32> {
        let previous = self.previous_entry?;
        let _ = previous;
        if self.transition_duration <= 0.0 {
            return None;
        }
        let alpha = clamp01((time - self.transition_start_time) / self.transition_duration);
        (alpha < 1.0).then_some(alpha)
    }
}

impl NodeRuntime for PlaylistNode {
    fn produce(
        &mut self,
        _slot: &SlotPath,
        ctx: &mut TickContext<'_>,
    ) -> Result<ProduceResult, NodeError> {
        let time = ctx.resolve_consumed_slot_value::<f32>(&SlotPath::parse("time").unwrap())?;
        let triggered_entry =
            detect_triggered_entry(ctx, &self.entries, &mut self.last_seen_triggers)?;
        if let Some(entry) = triggered_entry {
            self.switch_to(entry, time);
        } else if self.current_entry != self.idle_entry {
            let Some(duration) = self.duration(self.current_entry) else {
                return Err(NodeError::msg(format!(
                    "playlist entry {} has no duration",
                    self.current_entry
                )));
            };
            if time - self.switch_time >= duration {
                let next = self
                    .next_entry_after(self.current_entry)
                    .unwrap_or(self.idle_entry);
                self.switch_to(next, time);
            }
        }

        let entry_time = max_zero(time - self.switch_time);
        let entry_progress = self
            .duration(self.current_entry)
            .map(|duration| clamp01(entry_time / duration))
            .unwrap_or(-1.0);
        self.state.output.set_with_version(
            ctx.revision(),
            lpc_model::VisualProduct::new(ctx.node_id(), 0),
        );
        self.state
            .entry_time
            .set_with_version(ctx.revision(), entry_time);
        self.state
            .entry_progress
            .set_with_version(ctx.revision(), entry_progress);
        self.state
            .active_entry
            .set_with_version(ctx.revision(), self.current_entry);
        ctx.publish_runtime_slot(&self.state, SlotPath::parse("entry_time").unwrap())?;
        ctx.publish_runtime_slot(&self.state, SlotPath::parse("entry_progress").unwrap())?;
        ctx.publish_runtime_slot(&self.state, SlotPath::parse("active_entry").unwrap())?;
        ctx.publish_runtime_slot(&self.state, SlotPath::parse("output").unwrap())?;

        self.active_product = Some(resolve_entry_product(
            ctx,
            self.runtime_entry(self.current_entry).ok_or_missing()?,
        )?);
        if self.transition_alpha(time).is_none() {
            self.previous_entry = None;
            self.previous_product = None;
        } else if let Some(previous) = self.previous_entry {
            if self.previous_product.is_none() {
                self.previous_product = Some(resolve_entry_product(
                    ctx,
                    self.runtime_entry(previous).ok_or_missing()?,
                )?);
            }
        }
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
        PlaylistState::register_runtime_state_shape(registry).map(|_| ())
    }

    fn render_node(&mut self) -> Option<&mut dyn RenderNode> {
        Some(self)
    }
}

impl RenderNode for PlaylistNode {
    fn render_texture(
        &mut self,
        product: lpc_model::VisualProduct,
        request: &RenderTextureRequest,
        ctx: &mut RenderContext<'_>,
    ) -> Result<TextureRenderProduct, NodeError> {
        if request.format != TextureStorageFormat::Rgba16Unorm {
            return Err(NodeError::msg(
                "playlist texture render only supports RGBA16 unorm",
            ));
        }
        let mut texture = {
            let graphics = ctx
                .graphics()
                .ok_or_else(|| NodeError::msg("missing graphics backend"))?;
            graphics
                .alloc_output_buffer(request.width, request.height)
                .map_err(|e| NodeError::msg(format!("playlist scratch texture: {e}")))?
        };
        self.render_texture_into(product, request, &mut texture, ctx)?;
        let bytes = texture.data().to_vec();
        ctx.graphics()
            .expect("graphics checked above")
            .free_output_buffer(texture);
        TextureRenderProduct::rgba16_unorm(request.width, request.height, bytes)
            .map_err(|e| NodeError::msg(format!("playlist texture product: {e}")))
    }

    fn render_texture_into(
        &mut self,
        _product: lpc_model::VisualProduct,
        request: &RenderTextureRequest,
        target: &mut lp_shader::LpsTextureBuf,
        ctx: &mut RenderContext<'_>,
    ) -> Result<(), NodeError> {
        let Some(active) = self.active_product else {
            target.data_mut().fill(0);
            return Ok(());
        };
        let Some(alpha) = self.transition_alpha(ctx.time_seconds()) else {
            return ctx.render_texture_into(active, request, target);
        };
        let Some(previous) = self.previous_product else {
            return ctx.render_texture_into(active, request, target);
        };
        if request.format != TextureStorageFormat::Rgba16Unorm
            || target.format() != TextureStorageFormat::Rgba16Unorm
        {
            return Err(NodeError::msg(
                "playlist crossfade only supports RGBA16 unorm",
            ));
        }
        let mut previous_texture = {
            let graphics = ctx
                .graphics()
                .ok_or_else(|| NodeError::msg("missing graphics backend"))?;
            graphics
                .alloc_output_buffer(request.width, request.height)
                .map_err(|e| NodeError::msg(format!("playlist previous texture: {e}")))?
        };
        let mut active_texture = {
            let graphics = ctx
                .graphics()
                .ok_or_else(|| NodeError::msg("missing graphics backend"))?;
            graphics
                .alloc_output_buffer(request.width, request.height)
                .map_err(|e| NodeError::msg(format!("playlist active texture: {e}")))?
        };
        ctx.render_texture_into(previous, request, &mut previous_texture)?;
        ctx.render_texture_into(active, request, &mut active_texture)?;
        blend_rgba16(
            previous_texture.data(),
            active_texture.data(),
            alpha,
            target.data_mut(),
        )?;
        let graphics = ctx.graphics().expect("graphics checked above");
        graphics.free_output_buffer(previous_texture);
        graphics.free_output_buffer(active_texture);
        Ok(())
    }

    fn sample_visual_into(
        &mut self,
        _product: lpc_model::VisualProduct,
        request: VisualSampleBufferRequest<'_>,
        target: VisualSampleTarget<'_>,
        ctx: &mut RenderContext<'_>,
    ) -> Result<(), NodeError> {
        let Some(active) = self.active_product else {
            target.samples.data_mut().fill(0);
            return Ok(());
        };
        let Some(alpha) = self.transition_alpha(ctx.time_seconds()) else {
            return ctx.sample_visual_into(active, request, target);
        };
        let Some(previous) = self.previous_product else {
            return ctx.sample_visual_into(active, request, target);
        };
        let point_count = request.points.count();
        if target.samples.count() != point_count {
            return Err(NodeError::msg("playlist sample target count mismatch"));
        }

        let mut previous_samples = {
            let graphics = ctx
                .graphics()
                .ok_or_else(|| NodeError::msg("missing graphics backend"))?;
            graphics
                .alloc_sample_rgba16(point_count)
                .map_err(|e| NodeError::msg(format!("playlist previous samples: {e}")))?
        };
        let mut active_samples = {
            let graphics = ctx
                .graphics()
                .ok_or_else(|| NodeError::msg("missing graphics backend"))?;
            match graphics.alloc_sample_rgba16(point_count) {
                Ok(samples) => samples,
                Err(e) => {
                    graphics.free_sample_rgba16(previous_samples);
                    return Err(NodeError::msg(format!("playlist active samples: {e}")));
                }
            }
        };

        let points = request.points;
        let result = (|| {
            ctx.sample_visual_into(
                previous,
                VisualSampleBufferRequest {
                    points: &mut *points,
                    output_width: request.output_width,
                    output_height: request.output_height,
                    time_seconds: request.time_seconds,
                },
                VisualSampleTarget {
                    samples: &mut previous_samples,
                },
            )?;
            ctx.sample_visual_into(
                active,
                VisualSampleBufferRequest {
                    points: &mut *points,
                    output_width: request.output_width,
                    output_height: request.output_height,
                    time_seconds: request.time_seconds,
                },
                VisualSampleTarget {
                    samples: &mut active_samples,
                },
            )?;
            blend_rgba16_samples(
                previous_samples.data(),
                active_samples.data(),
                alpha,
                target.samples.data_mut(),
            )
        })();

        let graphics = ctx.graphics().expect("graphics checked above");
        graphics.free_sample_rgba16(previous_samples);
        graphics.free_sample_rgba16(active_samples);
        result
    }
}

pub fn playlist_output_path() -> SlotPath {
    SlotPath::parse("output").expect("playlist output path")
}

fn detect_triggered_entry(
    ctx: &mut TickContext<'_>,
    entries: &[PlaylistRuntimeEntry],
    last_seen: &mut BTreeMap<(u32, u32), u32>,
) -> Result<Option<u32>, NodeError> {
    let mut triggered = None;
    for entry in entries {
        let slot = SlotPath::parse(&format!("entries[{}].trigger", entry.index)).unwrap();
        let production = ctx
            .resolve(QueryKey::ConsumedSlot {
                node: ctx.node_id(),
                slot,
            })
            .map_err(|e| NodeError::msg(format!("resolve entry trigger: {e:?}")))?;
        let SlotData::Map(map) = production.data() else {
            continue;
        };
        for data in map.entries.values() {
            let Some(message) = control_message_from_slot_data(data)? else {
                continue;
            };
            let key = (entry.index, message.id());
            let previous = last_seen.insert(key, message.seq());
            if previous != Some(message.seq()) && triggered.is_none() {
                triggered = Some(entry.index);
            }
        }
    }
    Ok(triggered)
}

fn control_message_from_slot_data(data: &SlotData) -> Result<Option<ControlMessage>, NodeError> {
    let SlotData::Value(value) = data else {
        return Ok(None);
    };
    ControlMessage::from_lp_value(value.value())
        .map(Some)
        .map_err(|e| NodeError::msg(format!("control message value: {e}")))
}

fn resolve_entry_product(
    ctx: &mut TickContext<'_>,
    entry: &PlaylistRuntimeEntry,
) -> Result<lpc_model::VisualProduct, NodeError> {
    let production = ctx
        .resolve(QueryKey::ProducedSlot {
            node: entry.child,
            slot: entry.output_slot.clone(),
        })
        .map_err(|e| NodeError::msg(format!("resolve playlist child output: {e:?}")))?;
    let value = production
        .value_leaf()
        .ok_or_else(|| NodeError::msg("playlist child output is not a value"))?;
    lpc_model::VisualProduct::from_lp_value(value.value())
        .map_err(|e| NodeError::msg(format!("playlist child output: {e}")))
}

fn blend_rgba16(
    previous: &[u8],
    active: &[u8],
    alpha: f32,
    target: &mut [u8],
) -> Result<(), NodeError> {
    if previous.len() != active.len() || previous.len() != target.len() {
        return Err(NodeError::msg("playlist crossfade texture length mismatch"));
    }
    let alpha = clamp01(alpha);
    for ((prev, next), out) in previous
        .chunks_exact(2)
        .zip(active.chunks_exact(2))
        .zip(target.chunks_exact_mut(2))
    {
        let a = u16::from_le_bytes([prev[0], prev[1]]) as f32;
        let b = u16::from_le_bytes([next[0], next[1]]) as f32;
        let mixed = mix_u16(a, b, alpha);
        out.copy_from_slice(&mixed.to_le_bytes());
    }
    Ok(())
}

fn blend_rgba16_samples(
    previous: &[u16],
    active: &[u16],
    alpha: f32,
    target: &mut [u16],
) -> Result<(), NodeError> {
    if previous.len() != active.len() || previous.len() != target.len() {
        return Err(NodeError::msg("playlist crossfade sample length mismatch"));
    }
    let alpha = clamp01(alpha);
    for ((prev, next), out) in previous.iter().zip(active).zip(target.iter_mut()) {
        *out = mix_u16(*prev as f32, *next as f32, alpha);
    }
    Ok(())
}

fn mix_u16(a: f32, b: f32, alpha: f32) -> u16 {
    let mixed = a * (1.0 - alpha) + b * alpha + 0.5;
    if mixed <= 0.0 {
        0
    } else if mixed >= u16::MAX as f32 {
        u16::MAX
    } else {
        mixed as u16
    }
}

fn clamp01(value: f32) -> f32 {
    if value <= 0.0 {
        0.0
    } else if value >= 1.0 {
        1.0
    } else {
        value
    }
}

fn max_zero(value: f32) -> f32 {
    if value <= 0.0 { 0.0 } else { value }
}

trait OptionEntryExt<'a> {
    fn ok_or_missing(self) -> Result<&'a PlaylistRuntimeEntry, NodeError>;
}

impl<'a> OptionEntryExt<'a> for Option<&'a PlaylistRuntimeEntry> {
    fn ok_or_missing(self) -> Result<&'a PlaylistRuntimeEntry, NodeError> {
        self.ok_or_else(|| NodeError::msg("playlist entry has no loaded child node"))
    }
}
