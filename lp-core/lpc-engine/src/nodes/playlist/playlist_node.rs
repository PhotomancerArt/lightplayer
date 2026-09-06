//! Runtime playlist node: selects and blends owned visual child entries.

use alloc::format;
use alloc::vec::Vec;
use lp_collection::VecMap;

use lp_gfx::{SampleOutHandle, TextureHandle};
use lpc_model::{
    ControlMessage, FromLpValue, NodeId, PlaylistState, SlotAccess, SlotData, SlotPath,
    SlotShapeRegistry, SlotShapeRegistryError,
};
use lps_shared::TextureStorageFormat;

use crate::dataflow::resolver::QueryKey;
use crate::node::{
    DestroyCtx, MemPressureCtx, NodeError, NodeRuntime, PressureLevel, ProduceResult,
    RenderContext, RenderNode, RuntimeStateShape, TickContext, ensure_scratch_len, err_ctx,
};
use crate::products::visual::{RenderTextureRequest, TextureRenderProduct, VisualSampleStream};

#[derive(Clone, Debug, PartialEq)]
pub struct PlaylistRuntimeEntry {
    pub index: u32,
    pub child: NodeId,
    pub output_slot: SlotPath,
    pub duration: Option<f32>,
    pub fade_after: Option<f32>,
    /// Trigger message ids that start or restart this entry; `None` means the
    /// entry is never triggered.
    pub trigger_ids: Option<Vec<u32>>,
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
    last_seen_triggers: VecMap<u32, u32>,
    /// Entry key queued by [`WireNodeCommand::PlaylistActivateEntry`],
    /// applied (and cleared) on the next `produce` in the consumed `time`
    /// slot's domain — command switches reset the entry clock exactly like
    /// trigger switches, even when the playlist clock is scrubbed or rated.
    pending_activate: Option<u32>,
    /// The crossfade sample path's buffers, alive for one transition, so a
    /// running transition adds no per-tick allocation on the host heap OR
    /// in graphics memory (the tick-alloc rule from defect
    /// 2026-08-29-flash-write-wedges-under-zook-playback).
    crossfade_scratch: CrossfadeScratch,
    /// The four produced-slot paths this node publishes every `produce`,
    /// parsed once. Parsing them per frame was four `SlotPath`s built and
    /// dropped per tick for constants.
    published_paths: PublishedPaths,
}

/// The runtime state paths a [`PlaylistNode`] publishes each frame.
struct PublishedPaths {
    entry_time: SlotPath,
    entry_progress: SlotPath,
    active_entry: SlotPath,
    output: SlotPath,
}

impl PublishedPaths {
    fn new() -> Self {
        Self {
            entry_time: SlotPath::parse("entry_time").expect("playlist entry_time path"),
            entry_progress: SlotPath::parse("entry_progress")
                .expect("playlist entry_progress path"),
            active_entry: SlotPath::parse("active_entry").expect("playlist active_entry path"),
            output: SlotPath::parse("output").expect("playlist output path"),
        }
    }
}

/// The crossfade sample path's buffers, alive for one transition.
///
/// One window-sized sample-out (graphics memory, read in place through
/// `LpGraphics::sample_out_data`) and the blended host scratch (`4 × window`
/// `u16`s). Allocated on a transition's first frame, keyed on the stream's
/// window like the fixture's `SampleBatch`, and dropped when the transition
/// ends — never per frame. Before 2026-09-06 two count-sized handles were
/// created and freed every frame: 16 B/lamp of churn through the classic's
/// infallible allocator, and on the host a leak outright, since the wasmtime
/// backend's bump allocator never frees; then they lived for the transition;
/// now the window bounds them (`docs/adr/2026-09-06-direct-sampling-bounded-batches.md`).
#[derive(Default)]
struct CrossfadeScratch {
    /// One window of an entry's samples — both entries answer into it in
    /// turn, and the blend accumulates in `blended`.
    samples: Option<SampleOutHandle>,
    blended: Vec<u16>,
}

impl CrossfadeScratch {
    /// Free everything. Handles release their graphics memory on drop.
    fn clear(&mut self) {
        self.samples = None;
        self.blended = Vec::new();
    }

    #[cfg(test)]
    fn holds_buffers(&self) -> bool {
        self.samples.is_some() || !self.blended.is_empty()
    }
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
            last_seen_triggers: VecMap::new(),
            pending_activate: None,
            crossfade_scratch: CrossfadeScratch::default(),
            published_paths: PublishedPaths::new(),
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

    /// The transition is over (or there never was one): forget the outgoing
    /// entry and free the crossfade buffers, which exist only while one
    /// runs. The next transition's first frame allocates them again — once
    /// per transition, not once per frame.
    fn end_transition(&mut self) {
        self.previous_entry = None;
        self.previous_product = None;
        self.crossfade_scratch.clear();
    }
}

impl NodeRuntime for PlaylistNode {
    fn produce(
        &mut self,
        _slot: &SlotPath,
        ctx: &mut TickContext<'_>,
    ) -> Result<ProduceResult, NodeError> {
        // `bus:time` carries the product handle; the schedule below works in
        // effective seconds, so query it once per tick.
        let product = ctx.resolve_consumed_slot_value::<lpc_model::TimeProduct>(
            &SlotPath::parse("time").unwrap(),
        )?;
        let time = ctx.time_product_seconds(product)?;
        // Trigger detection always runs (it also advances the per-message
        // dedup state), but an explicit activate command wins a same-frame
        // race against a trigger message.
        let triggered_entry =
            detect_triggered_entry(ctx, &self.entries, &mut self.last_seen_triggers)?;
        if let Some(entry) = self.pending_activate.take() {
            self.switch_to(entry, time);
        } else if let Some(entry) = triggered_entry {
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
        ctx.publish_runtime_slot(&self.state, &self.published_paths.entry_time)?;
        ctx.publish_runtime_slot(&self.state, &self.published_paths.entry_progress)?;
        ctx.publish_runtime_slot(&self.state, &self.published_paths.active_entry)?;
        ctx.publish_runtime_slot(&self.state, &self.published_paths.output)?;

        self.active_product = Some(resolve_entry_product(
            ctx,
            self.runtime_entry(self.current_entry).ok_or_missing()?,
        )?);
        if self.transition_alpha(time).is_none() {
            self.end_transition();
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

    /// Activate-entry command (the wire runtime command channel): validate
    /// the entry key against the loaded runtime entries and queue it; the
    /// switch itself happens on the next `produce`, in the consumed `time`
    /// slot's domain, so the entry clock resets exactly as a trigger switch
    /// does. Unknown keys (including authored entries whose child never
    /// mounted) reject with a reason — a normal response, not a status
    /// poisoning.
    fn handle_command(
        &mut self,
        command: &lpc_wire::WireNodeCommand,
        _time_s: f32,
    ) -> Result<(), NodeError> {
        match command {
            lpc_wire::WireNodeCommand::PlaylistActivateEntry { entry } => {
                if self.runtime_entry(*entry).is_none() {
                    return Err(NodeError::msg(format!(
                        "playlist has no loaded entry {entry}"
                    )));
                }
                self.pending_activate = Some(*entry);
                Ok(())
            }
        }
    }

    fn destroy(&mut self, _ctx: &mut DestroyCtx) -> Result<(), NodeError> {
        Ok(())
    }

    fn handle_memory_pressure(
        &mut self,
        level: PressureLevel,
        _ctx: &mut MemPressureCtx,
    ) -> Result<(), NodeError> {
        // The crossfade buffers are droppable — the next transition frame
        // rebuilds them to bit-identical output — but NOT at `High`: that
        // broadcast is the top-of-tick compile window, and this node
        // rebuilds them at render time BEFORE the child entry's compile
        // runs inside `sample_visual_into`, so a drop there is
        // re-allocation, not reclaim (the ordering rule in
        // `engine/memory_pressure.rs`; ADR 2026-08-03, 2026-08-04
        // amendment). `Critical` is the embedder's between-ticks survival
        // broadcast, where nothing of ours is rebuilt before the allocation
        // that failed retries.
        if level >= PressureLevel::Critical {
            self.crossfade_scratch.clear();
        }
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
    /// A playlist has no space of its own: it answers with the active
    /// item's, so a 1D effect stays 1D behind a playlist. During a
    /// crossfade the ACTIVE item is the answer — the outgoing one is on
    /// its way out, and a mid-fade space flip would re-key the consumer's
    /// sample points twice.
    fn visual_space(
        &mut self,
        _product: lpc_model::VisualProduct,
        ctx: &mut RenderContext<'_>,
    ) -> Result<crate::products::visual::ProductSpaceInfo, NodeError> {
        let Some(active) = self.active_product else {
            return Ok(crate::products::visual::ProductSpaceInfo::two_d());
        };
        ctx.visual_product_space(active)
    }

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
                .create_render_target(request.width, request.height)
                .map_err(err_ctx("playlist scratch texture"))?
        };
        self.render_texture_into(product, request, &mut texture, ctx)?;
        let graphics = ctx.graphics().expect("graphics checked above");
        if !graphics.supports_read_back() {
            // GPU-resident tier: keep the rendered target on the GPU
            // (fidelity-tiers ADR; see the shader node's render_texture).
            return TextureRenderProduct::gpu_resident(texture)
                .map_err(err_ctx("playlist gpu texture product"));
        }
        let bytes = graphics
            .read_back(&texture)
            .map_err(err_ctx("playlist scratch read back"))?
            .into_bytes();
        TextureRenderProduct::rgba16_unorm(request.width, request.height, bytes)
            .map_err(err_ctx("playlist texture product"))
    }

    fn render_texture_into(
        &mut self,
        _product: lpc_model::VisualProduct,
        request: &RenderTextureRequest,
        target: &mut TextureHandle,
        ctx: &mut RenderContext<'_>,
    ) -> Result<(), NodeError> {
        let Some(active) = self.active_product else {
            ctx.graphics()
                .ok_or_else(|| NodeError::msg("missing graphics backend"))?
                .clear_texture(target)
                .map_err(err_ctx("playlist clear target"))?;
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
                .create_render_target(request.width, request.height)
                .map_err(err_ctx("playlist previous texture"))?
        };
        let mut active_texture = {
            let graphics = ctx
                .graphics()
                .ok_or_else(|| NodeError::msg("missing graphics backend"))?;
            graphics
                .create_render_target(request.width, request.height)
                .map_err(err_ctx("playlist active texture"))?
        };
        ctx.render_texture_into(previous, request, &mut previous_texture)?;
        ctx.render_texture_into(active, request, &mut active_texture)?;
        // GPU-resident op: the blend happens behind the graphics trait so
        // render products never leave the GPU on accelerated backends.
        ctx.graphics()
            .ok_or_else(|| NodeError::msg("missing graphics backend"))?
            .blend_textures(&previous_texture, &active_texture, alpha, target)
            .map_err(err_ctx("playlist crossfade blend"))
    }

    fn sample_visual_into(
        &mut self,
        _product: lpc_model::VisualProduct,
        mut stream: VisualSampleStream<'_>,
        ctx: &mut RenderContext<'_>,
    ) -> Result<(), NodeError> {
        let Some(active) = self.active_product else {
            let graphics = ctx
                .graphics()
                .ok_or_else(|| NodeError::msg("missing graphics backend"))?;
            return stream
                .drive_cleared(graphics)
                .map_err(err_ctx("playlist clear samples"));
        };
        let Some(alpha) = self.transition_alpha(ctx.time_seconds()) else {
            return ctx.sample_visual_into(active, stream);
        };
        let Some(previous) = self.previous_product else {
            return ctx.sample_visual_into(active, stream);
        };
        let capacity = stream.validate()?;

        // Resident for the transition: allocated on its first frame (or when
        // the window's capacity moves), reused every frame after, freed by
        // `end_transition`. Sized to the window, never to the product.
        ensure_crossfade_sample_out(
            &mut self.crossfade_scratch.samples,
            capacity,
            ctx,
            "playlist crossfade samples",
        )?;
        ensure_scratch_len(
            &mut self.crossfade_scratch.blended,
            capacity as usize * 4,
            "playlist blended samples",
        )?;
        let CrossfadeScratch { samples, blended } = &mut self.crossfade_scratch;
        let samples = samples
            .as_mut()
            .ok_or_else(|| NodeError::msg("playlist crossfade samples missing after allocation"))?;

        // This node drives the outer loop: each batch of the consumer's
        // coordinates is handed to BOTH entries through a one-batch inner
        // stream over the same point window (the inner `fill` yields the
        // batch's count once and leaves the words as the consumer filled
        // them), the two answers are blended in place, and the blend goes to
        // the consumer. The entries
        // bind their uniforms once per frame — their bound-uniforms key
        // survives across these inner streams.
        let mut continuation = false;
        loop {
            let n = {
                let graphics = ctx
                    .graphics()
                    .ok_or_else(|| NodeError::msg("missing graphics backend"))?;
                let words = graphics
                    .sample_points_data_mut(stream.points)
                    .map_err(err_ctx("playlist crossfade sample points"))?;
                (stream.fill)(words)
            };
            if n == 0 {
                return Ok(());
            }
            let words = n as usize * 4;
            {
                let mut once = Some(n);
                let mut fill = |_: &mut [i32]| once.take().unwrap_or(0);
                let mut consume = |data: &[u16]| -> Result<(), NodeError> {
                    blended[..words].copy_from_slice(data);
                    Ok(())
                };
                ctx.sample_visual_into(
                    previous,
                    VisualSampleStream {
                        points: &mut *stream.points,
                        samples: &mut *samples,
                        fill: &mut fill,
                        consume: &mut consume,
                        output_width: stream.output_width,
                        output_height: stream.output_height,
                        time_seconds: stream.time_seconds,
                        space: stream.space,
                        policy: stream.policy,
                        continuation,
                    },
                )?;
            }
            {
                let mut once = Some(n);
                let mut fill = |_: &mut [i32]| once.take().unwrap_or(0);
                let mut consume = |data: &[u16]| -> Result<(), NodeError> {
                    blend_rgba16_samples_in_place(&mut blended[..words], data, alpha)
                };
                ctx.sample_visual_into(
                    active,
                    VisualSampleStream {
                        points: &mut *stream.points,
                        samples: &mut *samples,
                        fill: &mut fill,
                        consume: &mut consume,
                        output_width: stream.output_width,
                        output_height: stream.output_height,
                        time_seconds: stream.time_seconds,
                        space: stream.space,
                        policy: stream.policy,
                        continuation,
                    },
                )?;
            }
            (stream.consume)(&blended[..words])?;
            continuation = true;
        }
    }
}
/// Size the crossfade's sample-out to `count` points (the stream's window,
/// never the product), allocating only when it is missing or the count
/// moved — the fixture's `ensure_sample_batch` rule. Allocation is fallible
/// through the backend; a failure degrades this frame and the next one
/// retries.
fn ensure_crossfade_sample_out(
    current: &mut Option<SampleOutHandle>,
    count: u32,
    ctx: &RenderContext<'_>,
    what: &'static str,
) -> Result<(), NodeError> {
    let stale = current
        .as_ref()
        .is_none_or(|samples| samples.count() != count);
    if !stale {
        return Ok(());
    }
    let graphics = ctx
        .graphics()
        .ok_or_else(|| NodeError::msg("missing graphics backend"))?;
    drop(current.take());
    let samples = graphics.create_sample_out(count).map_err(err_ctx(what))?;
    *current = Some(samples);
    Ok(())
}

fn detect_triggered_entry(
    ctx: &mut TickContext<'_>,
    entries: &[PlaylistRuntimeEntry],
    last_seen: &mut VecMap<u32, u32>,
) -> Result<Option<u32>, NodeError> {
    let production = ctx
        .resolve(&QueryKey::ConsumedSlot {
            node: ctx.node_id(),
            slot: SlotPath::parse("trigger").expect("playlist trigger slot"),
        })
        .map_err(|e| NodeError::msg(format!("resolve playlist trigger: {e:?}")))?;
    let SlotData::Map(map) = production.data() else {
        return Ok(None);
    };
    let mut triggered: Option<u32> = None;
    for data in map.entries.values() {
        let Some(message) = control_message_from_slot_data(data)? else {
            continue;
        };
        let previous = last_seen.insert(message.id(), message.seq());
        if previous == Some(message.seq()) {
            continue;
        }
        let entry = entries
            .iter()
            .filter(|entry| {
                entry
                    .trigger_ids
                    .as_ref()
                    .is_some_and(|ids| ids.contains(&message.id()))
            })
            .map(|entry| entry.index)
            .min();
        triggered = match (triggered, entry) {
            (Some(current), Some(candidate)) => Some(current.min(candidate)),
            (current, candidate) => current.or(candidate),
        };
    }
    Ok(triggered)
}

fn control_message_from_slot_data(data: &SlotData) -> Result<Option<ControlMessage>, NodeError> {
    let SlotData::Value(value) = data else {
        return Ok(None);
    };
    ControlMessage::from_lp_value(value.value())
        .map(Some)
        .map_err(err_ctx("control message value"))
}

fn resolve_entry_product(
    ctx: &mut TickContext<'_>,
    entry: &PlaylistRuntimeEntry,
) -> Result<lpc_model::VisualProduct, NodeError> {
    let production = ctx
        .resolve(&QueryKey::ProducedSlot {
            node: entry.child,
            slot: entry.output_slot.clone(),
        })
        .map_err(|e| NodeError::msg(format!("resolve playlist child output: {e:?}")))?;
    let value = production
        .value_leaf()
        .ok_or_else(|| NodeError::msg("playlist child output is not a value"))?;
    lpc_model::VisualProduct::from_lp_value(value.value()).map_err(err_ctx("playlist child output"))
}

// Texture crossfade blending moved behind `LpGraphics::blend_textures`
// (GPU-resident op family); the sample-channel blend below stays CPU-side
// for now — sample buffers are the GPU-sample-points milestone's domain.
/// Blend the incoming entry's samples over the outgoing entry's, in place:
/// the batched crossfade holds one window, samples the outgoing entry into
/// it, then mixes the incoming entry's answer in per channel.
fn blend_rgba16_samples_in_place(
    previous_then_out: &mut [u16],
    active: &[u16],
    alpha: f32,
) -> Result<(), NodeError> {
    if previous_then_out.len() != active.len() {
        return Err(NodeError::msg("playlist crossfade sample length mismatch"));
    }
    let alpha = clamp01(alpha);
    for (out, next) in previous_then_out.iter_mut().zip(active) {
        *out = mix_u16(*out as f32, *next as f32, alpha);
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

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use lpc_wire::WireNodeCommand;

    fn playlist_with_entries(keys: &[u32]) -> PlaylistNode {
        let entries = keys
            .iter()
            .map(|&index| PlaylistRuntimeEntry {
                index,
                child: NodeId::new(100 + index),
                output_slot: SlotPath::parse("output").unwrap(),
                duration: Some(4.0),
                fade_after: None,
                trigger_ids: None,
            })
            .collect();
        PlaylistNode::new(NodeId::new(1), keys[0], 0.35, entries)
    }

    #[test]
    fn activate_command_queues_a_known_entry() {
        let mut node = playlist_with_entries(&[1, 2]);

        node.handle_command(&WireNodeCommand::PlaylistActivateEntry { entry: 2 }, 0.5)
            .expect("known entry accepted");

        assert_eq!(node.pending_activate, Some(2));
        // The switch is deferred to produce (consumed time domain): the
        // command itself must not move the active entry or its clock.
        assert_eq!(node.current_entry, 1);
        assert_eq!(node.switch_time, 0.0);
    }

    #[test]
    fn activate_command_rejects_an_unknown_entry() {
        let mut node = playlist_with_entries(&[1, 2]);

        let err = node
            .handle_command(&WireNodeCommand::PlaylistActivateEntry { entry: 9 }, 0.5)
            .expect_err("unknown entry rejected");

        assert!(err.to_string().contains("no loaded entry 9"), "{err}");
        assert_eq!(node.pending_activate, None);
    }

    #[test]
    fn latest_activate_command_wins_within_a_frame() {
        let mut node = playlist_with_entries(&[1, 2, 3]);

        node.handle_command(&WireNodeCommand::PlaylistActivateEntry { entry: 2 }, 0.5)
            .expect("first accepted");
        node.handle_command(&WireNodeCommand::PlaylistActivateEntry { entry: 3 }, 0.6)
            .expect("second accepted");

        assert_eq!(node.pending_activate, Some(3));
    }

    #[test]
    fn switch_to_resets_the_entry_clock() {
        let mut node = playlist_with_entries(&[1, 2]);

        node.switch_to(2, 7.25);

        assert_eq!(node.current_entry, 2);
        assert_eq!(node.switch_time, 7.25);
    }

    /// The crossfade buffers live exactly as long as the transition: the
    /// end-of-transition seam `produce` calls frees all three.
    #[test]
    fn end_transition_drops_the_crossfade_buffers() {
        let mut node = playlist_with_entries(&[1, 2]);
        node.previous_entry = Some(1);
        seed_crossfade_buffers(&mut node, 4);
        assert!(node.crossfade_scratch.holds_buffers());

        node.end_transition();

        assert!(
            !node.crossfade_scratch.holds_buffers(),
            "end_transition must free the crossfade sample-outs and scratch"
        );
        assert_eq!(node.previous_entry, None);
        assert_eq!(node.previous_product, None);
    }

    /// `Critical` is the survival broadcast between ticks: drop. `High` is
    /// the top-of-tick compile window, and the render path rebuilds these
    /// before the child compile runs, so dropping there is re-allocation —
    /// the handler must leave them alone (ADR 2026-08-03 amendment).
    #[test]
    fn critical_pressure_drops_the_crossfade_buffers_and_high_does_not() {
        let mut node = playlist_with_entries(&[1, 2]);
        seed_crossfade_buffers(&mut node, 4);

        for level in [
            PressureLevel::Low,
            PressureLevel::Medium,
            PressureLevel::High,
        ] {
            let mut ctx = MemPressureCtx::new(NodeId::new(1), lpc_model::Revision::new(8));
            node.handle_memory_pressure(level, &mut ctx)
                .expect("handle pressure");
            assert!(
                node.crossfade_scratch.holds_buffers(),
                "{level:?} must not drop the crossfade buffers"
            );
        }

        let mut ctx = MemPressureCtx::new(NodeId::new(1), lpc_model::Revision::new(9));
        node.handle_memory_pressure(PressureLevel::Critical, &mut ctx)
            .expect("handle pressure");
        assert!(
            !node.crossfade_scratch.holds_buffers(),
            "Critical must drop the crossfade buffers"
        );
    }

    /// A keyed re-ensure is a no-op at the same count and a fresh handle
    /// at a different one — one allocation per transition, not per frame.
    #[test]
    fn ensure_crossfade_sample_out_is_keyed_on_the_point_count() {
        let graphics: alloc::sync::Arc<dyn lp_gfx::LpGraphics> =
            alloc::sync::Arc::new(test_graphics());
        let ctx = RenderContext::new(
            NodeId::new(1),
            lpc_model::Revision::new(1),
            Some(graphics.clone()),
            None,
            0.0,
        );
        let mut slot: Option<SampleOutHandle> = None;

        ensure_crossfade_sample_out(&mut slot, 4, &ctx, "test").expect("allocate");
        assert_eq!(slot.as_ref().map(SampleOutHandle::count), Some(4));

        // Same count: no allocation. The backend call count is what proves
        // it, and the crossfade probe (`tests/playlist_crossfade_memory.rs`)
        // pins that across a whole transition; here the handle must at
        // least still be there and still the right size.
        ensure_crossfade_sample_out(&mut slot, 4, &ctx, "test").expect("same count");
        assert_eq!(slot.as_ref().map(SampleOutHandle::count), Some(4));

        ensure_crossfade_sample_out(&mut slot, 8, &ctx, "test").expect("new count");
        assert_eq!(slot.as_ref().map(SampleOutHandle::count), Some(8));
    }

    fn test_graphics() -> lp_gfx_lpvm::TargetLpvmGraphics {
        lp_gfx_lpvm::TargetLpvmGraphics::new(lp_shader::ShaderFrontend::LpsGlsl)
    }

    /// Put real backend handles on the node, as a transition frame would.
    fn seed_crossfade_buffers(node: &mut PlaylistNode, points: u32) {
        use lp_gfx::LpGraphics;
        let graphics = test_graphics();
        node.crossfade_scratch.samples = Some(graphics.create_sample_out(points).expect("samples"));
        node.crossfade_scratch.blended = alloc::vec![0u16; points as usize * 4];
    }
}
