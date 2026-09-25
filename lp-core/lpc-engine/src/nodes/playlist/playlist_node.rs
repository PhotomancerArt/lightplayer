//! Runtime playlist node: plays one owned visual entry at a time and
//! switches between them without going black.
//!
//! The playlist holds every authored entry, but only one is ever loaded
//! (multi-pattern vision D8). A switch holds the last frame, asks the engine
//! to unload the old entry and load the new one, and fades from the held
//! frame once the new entry renders — see [`super::playlist_switch`] for the
//! sequence and [`super::playlist_held_frame`] for the buffer.
//!
//! **Where a switch is decided:** `produce`, in the consumed `time` slot's
//! domain — an activate command first, then an entry trigger, then a
//! next/prev trigger, then the cycle or (when the cycle is off) the timed
//! advance ([`PlaylistNode::switch_to`]). A failure moves on from the render
//! or the engine's hooks ([`PlaylistNode::fail_entry`]).
//!
//! **Cycling** (vision D13, plan A1–A3): with a running
//! [`lpc_model::PlaylistCycle::Cycle`] the playlist walks its enabled entries
//! in key order, one step each, as a pure function of the consumed `time`
//! and an anchor ([`super::playlist_cycle_position`]); the idle entry is an
//! ordinary stop. A pick, a trigger or next/prev re-anchors there. A held,
//! frozen or absent cycle is the playlist exactly as before: the idle entry,
//! triggers and per-entry durations (D17). The skip list marks entries
//! [`PlaylistEntryReason::Disabled`] either way.
//!
//! **The texture path** (`render_texture*`, plan PD5) has two kinds of
//! consumer:
//!
//! - **lamps**: a fixture authored `"sampling": "texture_area"` — the
//!   default when a fixture names no sampling, and the declared-strip idiom
//!   — renders the playlist into a texture and area-samples it. That HOLDS
//!   and fades like the sample path, with a held texture at the fixture's
//!   render size ([`super::playlist_held_texture`]). Every fixture shipped in
//!   `catalog/` and `projects/` is `"direct"` today, which samples through
//!   `sample_visual_into` (the device, the emulator and fw-browser's lamp
//!   preview all go through the fixture).
//! - **canvas previews** with no lamps behind them, through
//!   `Engine::render_texture_product`: Studio's visual-product probe
//!   (`project_read_probes.rs` — product previews and thumbnails, the GPU
//!   tier's GPU-resident read-back included) and fw-browser's canvas preview
//!   (`fw-browser/src/runtime.rs`, which a project with lamps skips for the
//!   output-frame read). These CUT: a target of another size than the held
//!   one renders the live or incoming product, black while it compiles, and
//!   a preview never makes the playlist allocate a held texture when the
//!   lamps already held on the sample path. Whether fw-browser should look
//!   exactly like the device here is plan P8's check (vision D14).

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use lp_collection::VecMap;

use lp_gfx::TextureHandle;
use lpc_model::{
    ControlMessage, FromLpValue, NodeId, NodeRuntimeStatus, PlaylistCycle, PlaylistState,
    SlotAccess, SlotData, SlotPath, SlotShapeRegistry, SlotShapeRegistryError, U32List,
};
use lps_shared::TextureStorageFormat;

use crate::dataflow::resolver::QueryKey;
use crate::node::{
    DestroyCtx, MemPressureCtx, NodeError, NodeRuntime, PressureLevel, ProduceResult,
    RenderContext, RenderNode, ResidencyRequest, RuntimeStateShape, TickContext,
    ensure_scratch_len, err_ctx,
};
use crate::products::visual::{
    RenderTextureRequest, TextureRenderProduct, VisualReadiness, VisualSampleStream,
};

use super::playlist_cycle_position::{PlaylistCycleAnchor, cycle_entry_at};
use super::playlist_held_frame::PlaylistHeldFrame;
use super::playlist_held_texture::PlaylistHeldTexture;
use super::playlist_runtime_entry::{
    next_playable_after, next_playable_in_order, prev_playable_before,
};
use super::playlist_switch::{PlaylistFramePlan, PlaylistSwitch, clamp01};
use super::{PlaylistEntryReason, PlaylistRuntimeEntry};

pub struct PlaylistNode {
    /// The entry played when nothing else chose one: the authored
    /// `idle_entry`, or the first entry when that names none
    /// ([`lpc_model::PlaylistDef::effective_idle_entry`]).
    idle_entry: u32,
    default_fade: f32,
    /// Every authored entry, loaded or not (plan PD4), sorted by key.
    entries: Vec<PlaylistRuntimeEntry>,
    /// Trigger message ids that step to the next / previous enabled entry.
    next_trigger_ids: Vec<u32>,
    prev_trigger_ids: Vec<u32>,
    /// The cycle this playlist last read (authored default or panel write).
    cycle: PlaylistCycle,
    /// Where the cycle counts from; set by a pick, a trigger or next/prev.
    anchor: Option<PlaylistCycleAnchor>,
    /// The skipped entry keys this playlist last read.
    skip: Vec<u32>,
    state: PlaylistState,
    /// The selected entry: the one playing, or the one a switch is bringing
    /// in.
    current_entry: u32,
    /// The published `active_entry`: the selected entry once its child is
    /// loaded, until then the one before.
    active_entry: u32,
    switch_time: f32,
    /// The playlist clock at this frame's `produce`.
    frame_time: f32,
    last_seen_triggers: VecMap<u32, u32>,
    /// Entry key queued by [`WireNodeCommand::PlaylistActivateEntry`],
    /// applied (and cleared) on the next `produce` in the consumed `time`
    /// slot's domain — command switches reset the entry clock exactly like
    /// trigger switches, even when the playlist clock is scrubbed or rated.
    pending_activate: Option<u32>,
    /// The load/unload the engine applies before the next tick.
    pending_request: Option<ResidencyRequest>,
    /// The switch in flight, if any.
    switch: Option<PlaylistSwitch>,
    /// The entry the last frame showed live (a `Live` or `Blend` plan), and
    /// the blend's alpha if it was one — what a switch decided now captures.
    shown_entry: Option<u32>,
    shown_alpha: Option<f32>,
    /// The next frame is a switch's first: capture what it shows.
    capture_pending: bool,
    /// The current entry has rendered for real since it loaded.
    current_ready: bool,
    /// What this frame's renders show, planned by `produce`.
    plan: PlaylistFramePlan,
    held: PlaylistHeldFrame,
    /// The same, on the texture path (a `texture_area` fixture).
    held_texture: PlaylistHeldTexture,
    /// One window of blended samples, alive for a fade (never per frame).
    blend_scratch: Vec<u16>,
    /// The runtime warning naming failed entries (Studio's view of the
    /// device-only reasons, without a wire change).
    failure_status: Option<String>,
    /// The four produced-slot paths this node publishes every `produce`,
    /// parsed once. Parsing them per frame was four `SlotPath`s built and
    /// dropped per tick for constants.
    published_paths: PublishedPaths,
}

/// The runtime state paths a [`PlaylistNode`] publishes each frame.
struct PublishedPaths {
    time: SlotPath,
    entry_time: SlotPath,
    entry_progress: SlotPath,
    active_entry: SlotPath,
    output: SlotPath,
}

impl PublishedPaths {
    fn new() -> Self {
        Self {
            time: SlotPath::parse("time").expect("playlist time path"),
            entry_time: SlotPath::parse("entry_time").expect("playlist entry_time path"),
            entry_progress: SlotPath::parse("entry_progress")
                .expect("playlist entry_progress path"),
            active_entry: SlotPath::parse("active_entry").expect("playlist active_entry path"),
            output: SlotPath::parse("output").expect("playlist output path"),
        }
    }
}

impl PlaylistNode {
    /// A playlist over every authored entry. `idle_entry` is the effective
    /// idle entry. If it is not loaded, the playlist asks for it.
    pub fn new(
        node_id: NodeId,
        idle_entry: u32,
        default_fade: f32,
        mut entries: Vec<PlaylistRuntimeEntry>,
    ) -> Self {
        entries.sort_by_key(|entry| entry.index);
        let pending_request = entries
            .iter()
            .find(|entry| entry.index == idle_entry && entry.child.is_none())
            .map(|entry| ResidencyRequest::load(entry.index));
        Self {
            idle_entry,
            default_fade,
            entries,
            next_trigger_ids: Vec::new(),
            prev_trigger_ids: Vec::new(),
            cycle: PlaylistCycle::Hold,
            anchor: None,
            skip: Vec::new(),
            state: PlaylistState::new(
                lpc_model::VisualProduct::new(node_id, 0),
                0.0,
                -1.0,
                idle_entry,
            ),
            current_entry: idle_entry,
            active_entry: idle_entry,
            switch_time: 0.0,
            frame_time: 0.0,
            last_seen_triggers: VecMap::new(),
            pending_activate: None,
            pending_request,
            switch: None,
            shown_entry: None,
            shown_alpha: None,
            capture_pending: false,
            current_ready: false,
            plan: PlaylistFramePlan::Clear,
            held: PlaylistHeldFrame::default(),
            held_texture: PlaylistHeldTexture::default(),
            blend_scratch: Vec::new(),
            failure_status: None,
            published_paths: PublishedPaths::new(),
        }
    }

    /// The authored next/prev trigger ids (`PlaylistDef::next_trigger_ids`
    /// and `prev_trigger_ids`).
    #[must_use]
    pub fn with_step_triggers(mut self, next: Vec<u32>, prev: Vec<u32>) -> Self {
        self.next_trigger_ids = next;
        self.prev_trigger_ids = prev;
        self
    }

    /// The per-entry reason of `index` (device-only, plan PD10).
    pub fn entry_reason(&self, index: u32) -> Option<&PlaylistEntryReason> {
        self.runtime_entry(index).map(|entry| &entry.reason)
    }

    /// The selected entry (published as `active_entry` once it is loaded).
    pub fn current_entry(&self) -> u32 {
        self.current_entry
    }

    fn runtime_entry(&self, index: u32) -> Option<&PlaylistRuntimeEntry> {
        self.entries.iter().find(|entry| entry.index == index)
    }

    fn runtime_entry_mut(&mut self, index: u32) -> Option<&mut PlaylistRuntimeEntry> {
        self.entries.iter_mut().find(|entry| entry.index == index)
    }

    fn fade_after(&self, index: u32) -> f32 {
        self.runtime_entry(index)
            .and_then(|entry| entry.fade_after)
            .unwrap_or(self.default_fade)
    }

    fn duration(&self, index: u32) -> Option<f32> {
        self.runtime_entry(index).and_then(|entry| entry.duration)
    }

    fn is_loaded(&self, index: u32) -> bool {
        self.runtime_entry(index)
            .is_some_and(|entry| entry.child.is_some())
    }

    /// Where the timed advance goes after the current entry: the next
    /// playable authored key, else back to idle — or, when idle itself
    /// failed, the next playable entry after it.
    fn timed_next(&self) -> u32 {
        if let Some(next) = next_playable_in_order(&self.entries, self.current_entry) {
            return next;
        }
        let idle_playable = self
            .runtime_entry(self.idle_entry)
            .is_some_and(|entry| entry.reason.is_playable());
        if idle_playable {
            return self.idle_entry;
        }
        next_playable_after(&self.entries, self.idle_entry).unwrap_or(self.current_entry)
    }

    /// The load/unload that makes `target` the loaded entry: unload the
    /// loaded one first (never two at once), then load `target`. Nothing
    /// when `target` is already loaded — an embedder that made several
    /// entries resident keeps them.
    fn request_for(&self, target: u32) -> Option<ResidencyRequest> {
        if self.is_loaded(target) {
            return None;
        }
        let unload = self
            .entries
            .iter()
            .find(|entry| entry.index != target && entry.child.is_some())
            .map(|entry| entry.index);
        Some(ResidencyRequest {
            load: Some(target),
            unload,
        })
    }

    /// Decide a switch to `target` (activate, trigger or timed advance).
    ///
    /// Switching to the entry already selected restarts its clock, as
    /// before. Otherwise the frame this runs on captures what the last frame
    /// showed, and the playlist asks for the new entry.
    fn switch_to(&mut self, target: u32, time: f32) {
        self.switch_to_with_fade(target, time, self.fade_after(self.current_entry));
    }

    /// [`Self::switch_to`] with an explicit fade: the cycle's steps fade by
    /// the cycle's `fade_seconds`.
    fn switch_to_with_fade(&mut self, target: u32, time: f32, fade: f32) {
        self.switch_time = time;
        if target == self.current_entry {
            return;
        }
        self.current_entry = target;
        self.current_ready = false;
        // Something was live last frame (the old entry, or a fade): that is
        // the frame to hold. Holding already, the held frame stays.
        self.capture_pending = self.shown_entry.is_some();
        self.switch = Some(PlaylistSwitch::holding(fade));
        self.pending_request = self.request_for(target);
    }

    /// An explicit choice of `target` — an activate command, a trigger, or
    /// next/prev: switch there and count the cycle from it (vision D13: the
    /// cycle carries on from the pick).
    fn pick(&mut self, target: u32, time: f32) {
        self.switch_to(target, time);
        self.anchor = Some(PlaylistCycleAnchor::new(target, time));
    }

    /// The entry `steps` enabled stops away from `from` (negative is
    /// previous), wrapping; `None` when nothing else can play.
    fn stepped_from(&self, from: u32, steps: i32) -> Option<u32> {
        let mut at = from;
        for _ in 0..steps.unsigned_abs() {
            at = if steps > 0 {
                next_playable_after(&self.entries, at)?
            } else {
                prev_playable_before(&self.entries, at)?
            };
        }
        (at != from).then_some(at)
    }

    /// The stops changed under a running cycle (a skip or a failure): count
    /// from the entry playing now, keeping the step phase, so the cycle does
    /// not jump and the entry playing keeps the rest of its step.
    fn rebase_cycle(&mut self) {
        if let (Some(step), Some(anchor)) = (self.cycle.running_step_seconds(), self.anchor) {
            self.anchor = Some(anchor.rebased_on(self.current_entry, step, self.frame_time));
        }
    }

    /// A cycle read this frame: a change of value re-anchors at the entry
    /// playing now, so turning the cycle on (or re-timing it) starts a fresh
    /// step instead of jumping.
    fn apply_cycle(&mut self, cycle: PlaylistCycle, time: f32) {
        if cycle == self.cycle {
            return;
        }
        self.cycle = cycle;
        self.anchor = Some(PlaylistCycleAnchor::new(self.current_entry, time));
    }

    /// A skip list read this frame: mark skipped entries `Disabled` and
    /// clear the mark from the rest. A failure outranks a skip. The entry
    /// playing is marked too but keeps playing until the cycle's next step.
    fn apply_skip(&mut self, skip: Vec<u32>) {
        if skip == self.skip {
            return;
        }
        for entry in &mut self.entries {
            let skipped = skip.contains(&entry.index);
            entry.reason = match (&entry.reason, skipped) {
                (PlaylistEntryReason::Failed(_), _) => continue,
                (_, true) => PlaylistEntryReason::Disabled,
                (PlaylistEntryReason::Disabled, false) if entry.child.is_some() => {
                    PlaylistEntryReason::Loaded
                }
                (PlaylistEntryReason::Disabled, false) => PlaylistEntryReason::NotPlaying,
                (_, false) => continue,
            };
        }
        self.skip = skip;
        self.rebase_cycle();
    }

    /// The consumed `cycle` and `skip`: the authored defaults, or what Play
    /// mode wrote on their channels. Absent reads as a hold and no skips.
    fn read_cycle_and_skip(
        &self,
        ctx: &mut TickContext<'_>,
    ) -> Result<(PlaylistCycle, Vec<u32>), NodeError> {
        let cycle = read_absent_as_none::<PlaylistCycle>(ctx, "cycle.some")?.unwrap_or_default();
        let skip = read_absent_as_none::<U32List>(ctx, "skip.some")?
            .map(|list| list.0)
            .unwrap_or_default();
        Ok((cycle, skip))
    }

    /// `index` failed to load, compile or produce: mark it, and if it was
    /// the one being brought in, move to the next candidate — still holding
    /// (plan PD9). With nothing left to play, hold and ask for nothing.
    fn fail_entry(&mut self, index: u32, reason: String) {
        log::warn!("playlist: entry {index} failed: {reason}");
        if let Some(entry) = self.runtime_entry_mut(index) {
            entry.reason = PlaylistEntryReason::Failed(reason);
        }
        self.failure_status = failure_status(&self.entries);
        if index != self.current_entry {
            self.rebase_cycle();
            return;
        }
        let Some(next) = next_playable_after(&self.entries, index) else {
            log::warn!("playlist: every entry has failed; holding what was last shown");
            self.pending_request = None;
            return;
        };
        let fade = self
            .switch
            .map_or_else(|| self.fade_after(index), |switch| switch.fade);
        self.current_entry = next;
        self.switch_time = self.frame_time;
        self.current_ready = false;
        self.switch = Some(PlaylistSwitch::holding(fade));
        self.pending_request = self.request_for(next);
        self.rebase_cycle();
    }

    /// A readiness answer for the current entry, asked after rendering it.
    fn observe_readiness(&mut self, readiness: VisualReadiness) {
        match readiness {
            VisualReadiness::Ready => {
                self.current_ready = true;
                if let Some(switch) = &mut self.switch
                    && switch.fade_start.is_none()
                {
                    switch.fade_start = Some(self.frame_time);
                }
            }
            VisualReadiness::Pending => {}
            VisualReadiness::Failed(reason) => {
                self.fail_entry(self.current_entry, format!("compile: {reason}"));
            }
        }
    }

    fn probe_current(&mut self, ctx: &mut RenderContext<'_>, product: lpc_model::VisualProduct) {
        let readiness = ctx
            .visual_product_readiness(product)
            .unwrap_or_else(|e| VisualReadiness::Failed(format!("{e}")));
        self.observe_readiness(readiness);
    }

    /// The switch is over: free what it held.
    fn end_switch(&mut self) {
        self.switch = None;
        self.held.release();
        self.held_texture.release();
        self.blend_scratch = Vec::new();
    }

    /// Plan this frame's output (see [`PlaylistFramePlan`]).
    fn plan_frame(
        &mut self,
        ctx: &mut TickContext<'_>,
        time: f32,
    ) -> Result<PlaylistFramePlan, NodeError> {
        // A switch's first frame shows exactly what the last frame showed,
        // and keeps a copy. The old entry is still loaded: the unload is
        // applied before the next tick.
        if core::mem::take(&mut self.capture_pending)
            && let Some(shown) = self.shown_entry
            && let Some(entry) = self.runtime_entry(shown)
            && entry.child.is_some()
            && let Ok(product) = resolve_entry_product(ctx, entry)
        {
            return Ok(match self.shown_alpha {
                Some(alpha) => PlaylistFramePlan::Blend {
                    product,
                    alpha,
                    capture: true,
                },
                None => PlaylistFramePlan::Live {
                    product,
                    capture: true,
                    probe: false,
                },
            });
        }

        let current = self.current_entry;
        let mut product = None;
        if let Some(entry) = self.runtime_entry(current)
            && entry.child.is_some()
        {
            match resolve_entry_product(ctx, entry) {
                Ok(resolved) => product = Some(resolved),
                // An entry that has not rendered for real yet (the incoming
                // one, or idle at boot) failing never fails the playlist.
                Err(error) if self.switch.is_some() || !self.current_ready => {
                    self.fail_entry(current, format!("produce: {error}"));
                }
                Err(error) => return Err(error),
            }
        }

        let held = self.held.is_held() || self.held_texture.is_held();
        Ok(match (product, held) {
            (None, false) => PlaylistFramePlan::Clear,
            (None, true) => PlaylistFramePlan::Held { probe: None },
            (Some(product), false) => {
                // Nothing held (boot, or nothing captured): a switch has
                // nothing to fade from, so it cuts.
                if self.switch.is_some() {
                    self.end_switch();
                }
                PlaylistFramePlan::Live {
                    product,
                    capture: false,
                    probe: !self.current_ready,
                }
            }
            (Some(product), true) => match self.switch.and_then(|switch| switch.alpha(time)) {
                None if self.switch.is_some() => PlaylistFramePlan::Held {
                    probe: Some(product),
                },
                Some(alpha) if alpha < 1.0 => PlaylistFramePlan::Blend {
                    product,
                    alpha,
                    capture: false,
                },
                _ => {
                    self.end_switch();
                    PlaylistFramePlan::Live {
                        product,
                        capture: false,
                        probe: !self.current_ready,
                    }
                }
            },
        })
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
        let product =
            ctx.resolve_consumed_slot_value::<lpc_model::TimeProduct>(&self.published_paths.time)?;
        let time = ctx.time_product_seconds(product)?;
        self.frame_time = time;
        let (cycle, skip) = self.read_cycle_and_skip(ctx)?;
        self.apply_skip(skip);
        self.apply_cycle(cycle, time);
        // Trigger detection always runs (it also advances the per-message
        // dedup state), but an explicit activate command wins a same-frame
        // race against a trigger message.
        let triggered = detect_triggers(
            ctx,
            &self.entries,
            TriggerIds {
                next: &self.next_trigger_ids,
                prev: &self.prev_trigger_ids,
            },
            &mut self.last_seen_triggers,
        )?;
        if let Some(entry) = self.pending_activate.take() {
            self.pick(entry, time);
        } else if let Some(entry) = triggered.entry {
            self.pick(entry, time);
        } else if triggered.steps != 0 {
            if let Some(target) = self.stepped_from(self.current_entry, triggered.steps) {
                self.pick(target, time);
            }
        } else if let Some(step) = self.cycle.running_step_seconds() {
            // Cycling: where the cycle is, is a pure function of the clock
            // and the anchor. Idle has no special role here (plan A2).
            if let Some(anchor) = self.anchor
                && let Some(target) = cycle_entry_at(&self.entries, anchor, step, time)
                && target != self.current_entry
            {
                self.switch_to_with_fade(target, time, self.cycle.fade_seconds());
            }
        } else if self.current_entry != self.idle_entry
            && let Some(duration) = self.duration(self.current_entry)
            && time - self.switch_time >= duration
        {
            // An entry with no duration (other than idle) stays until
            // something else switches: a playlist that moved there after a
            // failure must not fail with it.
            let next = self.timed_next();
            self.switch_to(next, time);
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
        // `active_entry` names the entry whose child is live: it moves to a
        // new entry once that entry is loaded (the frame after the switch is
        // decided), so it never names a dormant entry or one whose load
        // failed. Studio's "one live surface" keys on it.
        if self.is_loaded(self.current_entry) {
            self.active_entry = self.current_entry;
        }
        self.state
            .active_entry
            .set_with_version(ctx.revision(), self.active_entry);
        ctx.publish_runtime_slot(&self.state, &self.published_paths.entry_time)?;
        ctx.publish_runtime_slot(&self.state, &self.published_paths.entry_progress)?;
        ctx.publish_runtime_slot(&self.state, &self.published_paths.active_entry)?;
        ctx.publish_runtime_slot(&self.state, &self.published_paths.output)?;

        self.plan = self.plan_frame(ctx, time)?;
        (self.shown_entry, self.shown_alpha) = match self.plan {
            PlaylistFramePlan::Live { capture: true, .. }
            | PlaylistFramePlan::Blend { capture: true, .. } => {
                // The capture frame shows the leaving entry; after it the
                // held frame is shown.
                (None, None)
            }
            PlaylistFramePlan::Live { .. } => (Some(self.current_entry), None),
            PlaylistFramePlan::Blend { alpha, .. } => (Some(self.current_entry), Some(alpha)),
            PlaylistFramePlan::Held { .. } | PlaylistFramePlan::Clear => (None, None),
        };
        Ok(ProduceResult::Produced)
    }

    /// Activate-entry command (the wire runtime command channel): any
    /// AUTHORED key is accepted, dormant or not — a dormant key is the
    /// switch sequence's load request, applied on the next `produce` in the
    /// consumed `time` slot's domain, so the entry clock resets exactly as a
    /// trigger switch does. Unknown keys reject with a reason (a normal
    /// response, not a status poisoning). A failed entry is tried again:
    /// activating it is an explicit ask, unlike the timed advance and
    /// triggers, which skip it.
    fn handle_command(
        &mut self,
        command: &lpc_wire::WireNodeCommand,
        _time_s: f32,
    ) -> Result<(), NodeError> {
        match command {
            lpc_wire::WireNodeCommand::PlaylistActivateEntry { entry } => {
                let Some(runtime_entry) = self.runtime_entry_mut(*entry) else {
                    return Err(NodeError::msg(format!("playlist has no entry {entry}")));
                };
                if matches!(runtime_entry.reason, PlaylistEntryReason::Failed(_)) {
                    runtime_entry.reason = PlaylistEntryReason::NotPlaying;
                    self.failure_status = failure_status(&self.entries);
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
        // The blend scratch is droppable — the next fade frame rebuilds it
        // to bit-identical output — but NOT at `High`: that broadcast is the
        // top-of-tick compile window, and this node rebuilds it at render
        // time BEFORE the incoming entry's compile runs inside
        // `sample_visual_into`, so a drop there is re-allocation, not
        // reclaim (the ordering rule in `engine/memory_pressure.rs`; ADR
        // 2026-08-03, 2026-08-04 amendment). `Critical` is the embedder's
        // between-ticks survival broadcast. The held frame is never
        // dropped: it is what the lamps show until the new entry renders,
        // and dropping it is exactly the black frame the switch exists to
        // prevent.
        if level >= PressureLevel::Critical {
            self.blend_scratch = Vec::new();
        }
        Ok(())
    }

    fn residency_request(&mut self) -> Option<ResidencyRequest> {
        self.pending_request.take()
    }

    fn entry_loaded(&mut self, entry: u32, child: NodeId, output_slot: &SlotPath) {
        let Some(runtime_entry) = self.runtime_entry_mut(entry) else {
            return;
        };
        runtime_entry.child = Some(child);
        runtime_entry.output_slot = output_slot.clone();
        // A skipped entry can still load (picked explicitly, or playing out
        // its step): it stays marked.
        if runtime_entry.reason != PlaylistEntryReason::Disabled {
            runtime_entry.reason = PlaylistEntryReason::Loaded;
        }
        if entry == self.current_entry {
            self.current_ready = false;
        }
    }

    fn entry_unloaded(&mut self, entry: u32) {
        if let Some(runtime_entry) = self.runtime_entry_mut(entry) {
            runtime_entry.child = None;
            if runtime_entry.reason == PlaylistEntryReason::Loaded {
                runtime_entry.reason = PlaylistEntryReason::NotPlaying;
            }
        }
        if self.shown_entry == Some(entry) {
            self.shown_entry = None;
            self.shown_alpha = None;
        }
    }

    fn entry_load_failed(&mut self, entry: u32, reason: &str) {
        self.fail_entry(entry, format!("load: {reason}"));
    }

    /// The unload was refused (it would strand edits a commit writes): keep
    /// playing the entry that is still loaded, and do not ask again.
    fn residency_refused(&mut self, request: ResidencyRequest, reason: &str) {
        log::warn!("playlist: switch refused ({reason}); keeping the playing entry");
        if let Some(kept) = request.unload {
            self.current_entry = kept;
        }
        // A running cycle counts on from the kept entry: asking again every
        // frame would meet the same refusal every frame.
        self.anchor = Some(PlaylistCycleAnchor::new(
            self.current_entry,
            self.frame_time,
        ));
        self.current_ready = false;
        self.end_switch();
    }

    fn runtime_status(&self) -> Option<NodeRuntimeStatus> {
        self.failure_status.clone().map(NodeRuntimeStatus::Warn)
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
    /// A playlist has no space of its own: it answers with the product it
    /// shows or brings in, so a 1D effect stays 1D behind a playlist.
    fn visual_space(
        &mut self,
        _product: lpc_model::VisualProduct,
        ctx: &mut RenderContext<'_>,
    ) -> Result<crate::products::visual::ProductSpaceInfo, NodeError> {
        let Some(product) = self.plan.product() else {
            return Ok(crate::products::visual::ProductSpaceInfo::two_d());
        };
        ctx.visual_product_space(product)
    }

    /// A held or blended frame is real output; a live entry is as ready as
    /// it is; nothing to show is a wait.
    fn visual_readiness(
        &mut self,
        _product: lpc_model::VisualProduct,
        ctx: &mut RenderContext<'_>,
    ) -> Result<VisualReadiness, NodeError> {
        match self.plan {
            PlaylistFramePlan::Clear => Ok(VisualReadiness::Pending),
            PlaylistFramePlan::Held { .. } | PlaylistFramePlan::Blend { .. } => {
                Ok(VisualReadiness::Ready)
            }
            PlaylistFramePlan::Live { product, .. } => ctx.visual_product_readiness(product),
        }
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

    /// The texture path holds and fades like the sample path, at the size of
    /// the target the switch frame was rendered into (see
    /// [`super::playlist_held_texture`]); a target of another size — a
    /// canvas preview — cuts to the live product.
    fn render_texture_into(
        &mut self,
        _product: lpc_model::VisualProduct,
        request: &RenderTextureRequest,
        target: &mut TextureHandle,
        ctx: &mut RenderContext<'_>,
    ) -> Result<(), NodeError> {
        let revision = ctx.revision();
        match self.plan {
            PlaylistFramePlan::Clear => clear_target(ctx, target),
            PlaylistFramePlan::Live {
                product,
                capture,
                probe,
            } => {
                if !self.render_incoming_texture(product, probe, request, target, ctx)? {
                    return clear_target(ctx, target);
                }
                // Only when the lamps did not already hold on the sample path:
                // a canvas preview rendered after the tick must not cost a
                // held texture when the fixture sampled directly.
                if capture
                    && !self.held.is_held()
                    && let Err(error) = self.held_texture.capture(graphics(ctx)?, target)
                {
                    log::warn!("playlist: held texture refused ({error}); the switch cuts");
                }
                Ok(())
            }
            PlaylistFramePlan::Held { probe } => {
                if let Some(incoming) = probe {
                    // Rendered for its compile decision; overwritten below
                    // when a held frame of this size exists.
                    self.render_incoming_texture(incoming, true, request, target, ctx)?;
                }
                if !self.held_texture.show(graphics(ctx)?, target)? && probe.is_none() {
                    clear_target(ctx, target)?;
                }
                Ok(())
            }
            PlaylistFramePlan::Blend {
                product,
                alpha,
                capture,
            } => {
                if self.held_texture.held_for(target).is_none() {
                    return ctx.render_texture_into(product, request, target);
                }
                let fade = self.held_texture.fade_target(graphics(ctx)?, target)?;
                ctx.render_texture_into(product, request, fade)?;
                let (held, fade) = self
                    .held_texture
                    .held_and_fade()
                    .expect("held and fade targets exist");
                graphics(ctx)?
                    .blend_textures(held, fade, alpha, target)
                    .map_err(err_ctx("playlist fade blend"))?;
                if capture {
                    self.held_texture
                        .recapture(graphics(ctx)?, target, revision)?;
                }
                Ok(())
            }
        }
    }

    fn sample_visual_into(
        &mut self,
        _product: lpc_model::VisualProduct,
        stream: VisualSampleStream<'_>,
        ctx: &mut RenderContext<'_>,
    ) -> Result<(), NodeError> {
        match self.plan {
            PlaylistFramePlan::Clear => {
                let mut stream = stream;
                let graphics = ctx
                    .graphics()
                    .ok_or_else(|| NodeError::msg("missing graphics backend"))?;
                stream
                    .drive_cleared(graphics)
                    .map_err(err_ctx("playlist clear samples"))
            }
            PlaylistFramePlan::Live {
                product,
                capture,
                probe,
            } => {
                self.sample_live(product, capture, stream, ctx)?;
                if probe {
                    self.probe_current(ctx, product);
                }
                Ok(())
            }
            PlaylistFramePlan::Held { probe } => self.sample_held(probe, stream, ctx),
            PlaylistFramePlan::Blend {
                product,
                alpha,
                capture,
            } => self.sample_blend(product, alpha, capture, stream, ctx),
        }
    }
}

impl PlaylistNode {
    /// Render `product` into `target`. When `probe` is set (the current
    /// entry, not yet confirmed real), ask its readiness after; a render
    /// error then fails the entry instead of the playlist, and the answer is
    /// `false` (nothing was rendered).
    fn render_incoming_texture(
        &mut self,
        product: lpc_model::VisualProduct,
        probe: bool,
        request: &RenderTextureRequest,
        target: &mut TextureHandle,
        ctx: &mut RenderContext<'_>,
    ) -> Result<bool, NodeError> {
        match ctx.render_texture_into(product, request, target) {
            Ok(()) => {
                if probe {
                    self.probe_current(ctx, product);
                }
                Ok(true)
            }
            Err(error) if probe => {
                self.fail_entry(self.current_entry, format!("render: {error}"));
                Ok(false)
            }
            Err(error) => Err(error),
        }
    }

    /// One entry, live: pass the stream through, counting what it streams
    /// (the next capture's size) and, on a switch's first frame, copying it
    /// into the held frame.
    fn sample_live(
        &mut self,
        product: lpc_model::VisualProduct,
        capture: bool,
        stream: VisualSampleStream<'_>,
        ctx: &mut RenderContext<'_>,
    ) -> Result<(), NodeError> {
        let mut capturing = capture;
        if capturing && let Err(error) = self.held.begin_capture() {
            log::warn!("{error}");
            capturing = false;
        }
        let VisualSampleStream {
            points,
            samples,
            fill,
            consume,
            output_width,
            output_height,
            time_seconds,
            space,
            policy,
            continuation,
            scope,
        } = stream;
        let held = &mut self.held;
        let mut words = 0usize;
        let mut capture_failed = false;
        let mut counted = |data: &[u16]| -> Result<(), NodeError> {
            words += data.len();
            if capturing && !capture_failed && held.capture(data).is_err() {
                capture_failed = true;
            }
            consume(data)
        };
        ctx.sample_visual_into(
            product,
            VisualSampleStream {
                points,
                samples,
                fill,
                consume: &mut counted,
                output_width,
                output_height,
                time_seconds,
                space,
                policy,
                continuation,
                scope,
            },
        )?;
        self.held.note_frame_words(words);
        if capturing {
            if capture_failed {
                log::warn!("playlist: held frame capture refused; the switch cuts");
                self.held.release();
            } else {
                self.held.finish_capture();
            }
        }
        Ok(())
    }

    /// The held frame. When the incoming entry is loaded, it is sampled for
    /// the first batch only (its answer is discarded) so it can make its
    /// compile decision, then asked whether that render was real.
    fn sample_held(
        &mut self,
        probe: Option<lpc_model::VisualProduct>,
        stream: VisualSampleStream<'_>,
        ctx: &mut RenderContext<'_>,
    ) -> Result<(), NodeError> {
        let VisualSampleStream {
            points,
            samples,
            fill,
            consume,
            output_width,
            output_height,
            time_seconds,
            space,
            policy,
            continuation: _,
            scope,
        } = stream;
        let capacity = points.count();
        let mut probe = probe;
        let mut offset = 0usize;
        loop {
            let n = {
                let graphics = ctx
                    .graphics()
                    .ok_or_else(|| NodeError::msg("missing graphics backend"))?;
                let words = graphics
                    .sample_points_data_mut(points)
                    .map_err(err_ctx("playlist held sample points"))?;
                fill(words)
            };
            if n == 0 {
                return Ok(());
            }
            if n > capacity {
                return Err(NodeError::msg(format!(
                    "sample stream: fill wrote {n} points into a {capacity}-point window"
                )));
            }
            let words = n as usize * 4;
            if let Some(incoming) = probe.take() {
                let mut once = Some(n);
                let mut one_batch = |_: &mut [i32]| once.take().unwrap_or(0);
                let mut discard = |_: &[u16]| -> Result<(), NodeError> { Ok(()) };
                let sampled = ctx.sample_visual_into(
                    incoming,
                    VisualSampleStream {
                        points: &mut *points,
                        samples: &mut *samples,
                        fill: &mut one_batch,
                        consume: &mut discard,
                        output_width,
                        output_height,
                        time_seconds,
                        space,
                        policy,
                        continuation: false,
                        scope,
                    },
                );
                match sampled {
                    Ok(()) => self.probe_current(ctx, incoming),
                    Err(error) => self.fail_entry(self.current_entry, format!("render: {error}")),
                }
            }
            let held = self.held.words(offset, words);
            if held.len() == words {
                consume(held)?;
            } else {
                // The stream is longer than the captured frame: the lamps
                // past its end get black.
                ensure_scratch_len(&mut self.blend_scratch, words, "playlist held padding")?;
                let pad = &mut self.blend_scratch[..words];
                pad[..held.len()].copy_from_slice(held);
                pad[held.len()..].fill(0);
                consume(pad)?;
            }
            offset += words;
        }
    }

    /// The fade: each batch of the incoming entry is blended over the held
    /// frame at the same positions. On a mid-fade switch the blend is also
    /// written back as the new held frame.
    fn sample_blend(
        &mut self,
        product: lpc_model::VisualProduct,
        alpha: f32,
        capture: bool,
        stream: VisualSampleStream<'_>,
        ctx: &mut RenderContext<'_>,
    ) -> Result<(), NodeError> {
        let VisualSampleStream {
            points,
            samples,
            fill,
            consume,
            output_width,
            output_height,
            time_seconds,
            space,
            policy,
            continuation: _,
            scope,
        } = stream;
        let capacity = points.count();
        // Resident for the fade: sized on its first frame (or when the
        // window moves), reused every frame after, freed by `end_switch`.
        // Sized to the window, never to the product.
        ensure_scratch_len(
            &mut self.blend_scratch,
            capacity as usize * 4,
            "playlist blended samples",
        )?;
        let Self {
            held,
            blend_scratch,
            ..
        } = self;
        let mut offset = 0usize;
        let mut continuation = false;
        loop {
            let n = {
                let graphics = ctx
                    .graphics()
                    .ok_or_else(|| NodeError::msg("missing graphics backend"))?;
                let words = graphics
                    .sample_points_data_mut(points)
                    .map_err(err_ctx("playlist fade sample points"))?;
                fill(words)
            };
            if n == 0 {
                return Ok(());
            }
            if n > capacity {
                return Err(NodeError::msg(format!(
                    "sample stream: fill wrote {n} points into a {capacity}-point window"
                )));
            }
            let words = n as usize * 4;
            {
                let from = held.words(offset, words);
                let blended = &mut blend_scratch[..words];
                let mut once = Some(n);
                let mut one_batch = |_: &mut [i32]| once.take().unwrap_or(0);
                let mut blend = |data: &[u16]| -> Result<(), NodeError> {
                    blend_rgba16_samples(from, data, alpha, blended)
                };
                ctx.sample_visual_into(
                    product,
                    VisualSampleStream {
                        points: &mut *points,
                        samples: &mut *samples,
                        fill: &mut one_batch,
                        consume: &mut blend,
                        output_width,
                        output_height,
                        time_seconds,
                        space,
                        policy,
                        continuation,
                        scope,
                    },
                )?;
            }
            if capture {
                let into = held.words_mut(offset, words);
                let len = into.len();
                into.copy_from_slice(&blend_scratch[..len]);
            }
            consume(&blend_scratch[..words])?;
            offset += words;
            continuation = true;
        }
    }
}

fn graphics<'a>(ctx: &'a RenderContext<'_>) -> Result<&'a dyn lp_gfx::LpGraphics, NodeError> {
    ctx.graphics()
        .ok_or_else(|| NodeError::msg("missing graphics backend"))
}

fn clear_target(ctx: &RenderContext<'_>, target: &mut TextureHandle) -> Result<(), NodeError> {
    graphics(ctx)?
        .clear_texture(target)
        .map_err(err_ctx("playlist clear target"))
}

/// The authored step trigger ids, matched beside the entries' own.
struct TriggerIds<'a> {
    next: &'a [u32],
    prev: &'a [u32],
}

/// What this frame's fresh trigger messages ask for.
#[derive(Default)]
struct TriggeredAction {
    /// An entry trigger (the lowest entry claiming any fresh id).
    entry: Option<u32>,
    /// Net next (+) / prev (-) presses among ids no entry claims.
    steps: i32,
}

fn detect_triggers(
    ctx: &mut TickContext<'_>,
    entries: &[PlaylistRuntimeEntry],
    step_ids: TriggerIds<'_>,
    last_seen: &mut VecMap<u32, u32>,
) -> Result<TriggeredAction, NodeError> {
    let production = ctx
        .resolve(&QueryKey::ConsumedSlot {
            node: ctx.node_id(),
            slot: SlotPath::parse("trigger").expect("playlist trigger slot"),
        })
        .map_err(|e| NodeError::msg(format!("resolve playlist trigger: {e:?}")))?;
    let SlotData::Map(map) = production.data() else {
        return Ok(TriggeredAction::default());
    };
    let mut action = TriggeredAction::default();
    for data in map.entries.values() {
        let Some(message) = control_message_from_slot_data(data)? else {
            continue;
        };
        let previous = last_seen.insert(message.id(), message.seq());
        if previous == Some(message.seq()) {
            continue;
        }
        // Triggers route by AUTHORED ids, loaded or not; a failed or skipped
        // entry ignores its trigger.
        let entry = entries
            .iter()
            .filter(|entry| {
                entry.reason.is_playable()
                    && entry
                        .trigger_ids
                        .as_ref()
                        .is_some_and(|ids| ids.contains(&message.id()))
            })
            .map(|entry| entry.index)
            .min();
        action.entry = match (action.entry, entry) {
            (Some(current), Some(candidate)) => Some(current.min(candidate)),
            (current, candidate) => current.or(candidate),
        };
        if entry.is_none() {
            if step_ids.next.contains(&message.id()) {
                action.steps += 1;
            }
            if step_ids.prev.contains(&message.id()) {
                action.steps -= 1;
            }
        }
    }
    Ok(action)
}

/// Read an optional consumed slot's `some` value, `None` when it is absent.
///
/// An option nobody authored and nobody wrote on its channel comes back as
/// the typed [`crate::dataflow::resolver::ResolveError::is_absent_option`],
/// which allocates nothing: this runs every frame per playlist. Any other
/// error — an unresolved slot, a written value of the wrong shape — is still
/// an error.
///
/// A static path is interned by the resolver once per structural epoch; a
/// compiled `PlaylistDefView` for the same two reads measured 4,192 B more
/// of the C6 image.
fn read_absent_as_none<T: FromLpValue>(
    ctx: &mut TickContext<'_>,
    path: &'static str,
) -> Result<Option<T>, NodeError> {
    let production = match ctx.resolve_static_consumed(path) {
        Ok(production) => production,
        Err(error) if error.is_absent_option() => return Ok(None),
        Err(error) => {
            return Err(NodeError::msg(format!(
                "resolve playlist {path}: {}",
                error.message
            )));
        }
    };
    let value = production
        .value_leaf()
        .ok_or_else(|| NodeError::msg(format!("playlist {path} is not a value")))?;
    T::from_lp_value(value.value())
        .map(Some)
        .map_err(|error| NodeError::msg(format!("playlist {path}: {error}")))
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
    let child = entry
        .child
        .ok_or_else(|| NodeError::msg("playlist entry has no loaded child node"))?;
    let production = ctx
        .resolve(&QueryKey::ProducedSlot {
            node: child,
            slot: entry.output_slot.clone(),
        })
        .map_err(|e| NodeError::msg(format!("resolve playlist child output: {e:?}")))?;
    let value = production
        .value_leaf()
        .ok_or_else(|| NodeError::msg("playlist child output is not a value"))?;
    lpc_model::VisualProduct::from_lp_value(value.value()).map_err(err_ctx("playlist child output"))
}

/// The playlist's runtime warning: every failed entry and why. The text is
/// `lpc_model`'s ([`lpc_model::format_playlist_failure_status`]), because
/// Studio reads the failed keys back out of it with the paired parser.
fn failure_status(entries: &[PlaylistRuntimeEntry]) -> Option<String> {
    lpc_model::format_playlist_failure_status(entries.iter().filter_map(
        |entry| match &entry.reason {
            PlaylistEntryReason::Failed(reason) => Some((entry.index, reason.as_str())),
            _ => None,
        },
    ))
}

// Texture crossfade blending lives behind `LpGraphics::blend_textures`
// (GPU-resident op family); the playlist's texture path cuts and no longer
// uses it. The sample-channel blend below stays CPU-side.
/// Blend the incoming entry's samples over the held frame's into `out`.
/// Held words past the end of `held` (a stream longer than the captured
/// frame) blend from black.
fn blend_rgba16_samples(
    held: &[u16],
    incoming: &[u16],
    alpha: f32,
    out: &mut [u16],
) -> Result<(), NodeError> {
    if incoming.len() != out.len() {
        return Err(NodeError::msg("playlist fade sample length mismatch"));
    }
    let alpha = clamp01(alpha);
    for (index, (out, next)) in out.iter_mut().zip(incoming).enumerate() {
        let from = held.get(index).copied().unwrap_or(0);
        *out = mix_u16(from as f32, *next as f32, alpha);
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

fn max_zero(value: f32) -> f32 {
    if value <= 0.0 { 0.0 } else { value }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use lpc_wire::WireNodeCommand;

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
    fn activate_command_accepts_a_dormant_entry() {
        let mut node = playlist_with_entries(&[1, 2]);
        assert!(!node.is_loaded(2));

        node.handle_command(&WireNodeCommand::PlaylistActivateEntry { entry: 2 }, 0.5)
            .expect("an authored dormant entry is accepted");

        assert_eq!(node.pending_activate, Some(2));
    }

    #[test]
    fn activate_command_rejects_an_unknown_entry() {
        let mut node = playlist_with_entries(&[1, 2]);

        let err = node
            .handle_command(&WireNodeCommand::PlaylistActivateEntry { entry: 9 }, 0.5)
            .expect_err("unknown entry rejected");

        assert!(err.to_string().contains("no entry 9"), "{err}");
        assert_eq!(node.pending_activate, None);
    }

    #[test]
    fn activate_command_retries_a_failed_entry() {
        let mut node = playlist_with_entries(&[1, 2]);
        node.fail_entry(2, String::from("bad glsl"));
        assert!(node.failure_status.is_some());

        node.handle_command(&WireNodeCommand::PlaylistActivateEntry { entry: 2 }, 0.5)
            .expect("an explicit ask tries a failed entry again");

        assert_eq!(node.entry_reason(2), Some(&PlaylistEntryReason::NotPlaying));
        assert_eq!(node.failure_status, None);
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
    fn switch_to_a_dormant_entry_unloads_then_loads_and_captures() {
        let mut node = playlist_with_entries(&[1, 2]);
        node.shown_entry = Some(1);

        node.switch_to(2, 7.25);

        assert_eq!(node.current_entry, 2);
        assert_eq!(node.switch_time, 7.25);
        assert!(
            node.capture_pending,
            "the switch frame captures what was shown"
        );
        assert_eq!(
            node.residency_request(),
            Some(ResidencyRequest::switch(1, 2)),
            "unload the old entry before loading the new one"
        );
        assert_eq!(
            node.residency_request(),
            None,
            "taking the request clears it"
        );
    }

    #[test]
    fn switch_to_the_current_entry_only_restarts_its_clock() {
        let mut node = playlist_with_entries(&[1, 2]);

        node.switch_to(1, 3.0);

        assert_eq!(node.switch_time, 3.0);
        assert_eq!(node.switch, None);
        assert_eq!(node.residency_request(), None);
    }

    #[test]
    fn a_new_playlist_asks_for_its_idle_entry_when_it_is_not_loaded() {
        let entries = alloc::vec![
            PlaylistRuntimeEntry::dormant(1),
            PlaylistRuntimeEntry::dormant(2),
        ];
        let mut node = PlaylistNode::new(NodeId::new(1), 1, 0.35, entries);

        assert_eq!(node.residency_request(), Some(ResidencyRequest::load(1)));
    }

    #[test]
    fn a_load_failure_moves_on_to_the_next_entry_and_keeps_holding() {
        let mut node = playlist_with_entries(&[1, 2, 3]);
        node.shown_entry = Some(1);
        node.switch_to(2, 1.0);
        let _ = node.residency_request();
        // The engine unloads 1 and fails to load 2.
        node.entry_unloaded(1);
        node.entry_load_failed(2, "missing file");

        assert!(matches!(
            node.entry_reason(2),
            Some(PlaylistEntryReason::Failed(reason)) if reason.contains("missing file")
        ));
        assert_eq!(node.current_entry, 3);
        assert!(
            node.switch.is_some_and(|s| s.fade_start.is_none()),
            "still holding"
        );
        assert_eq!(node.residency_request(), Some(ResidencyRequest::load(3)));
        assert!(
            node.runtime_status()
                .is_some_and(|status| matches!(status, NodeRuntimeStatus::Warn(text) if text.contains("entry 2 failed"))),
            "Studio sees the failure as the playlist's warning"
        );
    }

    #[test]
    fn every_entry_failing_requests_nothing_further() {
        let mut node = playlist_with_entries(&[1, 2]);
        node.switch_to(2, 1.0);
        let _ = node.residency_request();
        node.entry_load_failed(2, "missing");
        // Moved back to 1, which then fails to compile.
        assert_eq!(node.current_entry, 1);
        node.fail_entry(1, String::from("bad glsl"));

        assert_eq!(node.residency_request(), None, "nothing left: no spin");
    }

    /// Studio's Pattern instrument reads the failed keys back out of the
    /// warning with the paired `lpc_model` parser; this is the producer's
    /// half of that contract.
    #[test]
    fn the_failure_warning_names_every_failed_key_to_the_shared_parser() {
        let mut node = playlist_with_entries(&[1, 2, 3]);
        node.fail_entry(3, String::from("compile: x = 1; y (line 2)"));
        node.fail_entry(2, String::from("load: missing file"));

        let Some(NodeRuntimeStatus::Warn(text)) = node.runtime_status() else {
            panic!("a failure is a warning");
        };
        assert_eq!(
            lpc_model::parse_playlist_failed_entries(&text),
            [2, 3],
            "key order, whatever order they failed in: {text}"
        );
    }

    #[test]
    fn a_refused_switch_keeps_the_playing_entry() {
        let mut node = playlist_with_entries(&[1, 2]);
        node.shown_entry = Some(1);
        node.switch_to(2, 1.0);
        let request = node.residency_request().expect("switch request");

        node.residency_refused(request, "uncommitted edits");

        assert_eq!(node.current_entry, 1);
        assert_eq!(node.switch, None);
        assert_eq!(node.residency_request(), None, "no retry every tick");
    }

    #[test]
    fn timed_advance_skips_failed_entries_and_returns_to_idle() {
        let mut node = playlist_with_entries(&[1, 2, 3, 4]);
        node.fail_entry(3, String::from("bad glsl"));
        node.current_entry = 2;

        assert_eq!(node.timed_next(), 4, "3 failed");
        node.current_entry = 4;
        assert_eq!(node.timed_next(), 1, "after the last entry: idle");
        node.fail_entry(1, String::from("bad glsl"));
        assert_eq!(
            node.timed_next(),
            2,
            "idle failed: the next playable after it"
        );
    }

    #[test]
    fn a_skip_list_marks_entries_disabled_and_a_failure_outranks_it() {
        let mut node = playlist_with_entries(&[1, 2, 3]);
        node.fail_entry(3, String::from("bad glsl"));

        node.apply_skip(alloc::vec![1, 2, 3]);
        assert_eq!(node.entry_reason(1), Some(&PlaylistEntryReason::Disabled));
        assert_eq!(node.entry_reason(2), Some(&PlaylistEntryReason::Disabled));
        assert!(matches!(
            node.entry_reason(3),
            Some(PlaylistEntryReason::Failed(_))
        ));

        node.apply_skip(Vec::new());
        assert_eq!(
            node.entry_reason(1),
            Some(&PlaylistEntryReason::Loaded),
            "entry 1 is loaded: unskipped, it is Loaded again"
        );
        assert_eq!(node.entry_reason(2), Some(&PlaylistEntryReason::NotPlaying));
    }

    #[test]
    fn a_skipped_entry_that_loads_stays_disabled() {
        let mut node = playlist_with_entries(&[1, 2]);
        node.apply_skip(alloc::vec![2]);

        node.entry_loaded(2, NodeId::new(102), &SlotPath::parse("output").unwrap());

        assert_eq!(node.entry_reason(2), Some(&PlaylistEntryReason::Disabled));
        assert!(node.is_loaded(2));
    }

    #[test]
    fn next_and_prev_step_over_disabled_entries_and_wrap() {
        let mut node = playlist_with_entries(&[1, 2, 3, 4]);
        node.apply_skip(alloc::vec![3]);

        assert_eq!(node.stepped_from(2, 1), Some(4));
        assert_eq!(node.stepped_from(4, 1), Some(1), "wraps");
        assert_eq!(node.stepped_from(1, -1), Some(4), "wraps back");
        assert_eq!(node.stepped_from(1, 2), Some(4), "two presses in a frame");
        node.apply_skip(alloc::vec![2, 3, 4]);
        assert_eq!(node.stepped_from(1, 1), None, "nothing else can play");
    }

    /// `Critical` is the survival broadcast between ticks: drop the blend
    /// scratch. `High` is the top-of-tick compile window, and the render
    /// path rebuilds it before the incoming entry's compile runs, so
    /// dropping there is re-allocation (ADR 2026-08-03 amendment). The held
    /// frame survives both: dropping it is the black frame.
    #[test]
    fn critical_pressure_drops_the_blend_scratch_and_keeps_the_held_frame() {
        let mut node = playlist_with_entries(&[1, 2]);
        node.blend_scratch = alloc::vec![0u16; 16];
        node.held.begin_capture().expect("reserve");
        node.held.capture(&[1, 2, 3, 4]).expect("capture");
        node.held.finish_capture();

        for level in [
            PressureLevel::Low,
            PressureLevel::Medium,
            PressureLevel::High,
        ] {
            let mut ctx = MemPressureCtx::new(NodeId::new(1), lpc_model::Revision::new(8));
            node.handle_memory_pressure(level, &mut ctx)
                .expect("handle pressure");
            assert!(
                !node.blend_scratch.is_empty(),
                "{level:?} must not drop the blend scratch"
            );
        }

        let mut ctx = MemPressureCtx::new(NodeId::new(1), lpc_model::Revision::new(9));
        node.handle_memory_pressure(PressureLevel::Critical, &mut ctx)
            .expect("handle pressure");
        assert!(node.blend_scratch.is_empty(), "Critical drops the scratch");
        assert!(node.held.is_held(), "the held frame survives");
    }

    /// The switch's end frees everything it held.
    #[test]
    fn end_switch_frees_the_held_frame_and_the_scratch() {
        let mut node = playlist_with_entries(&[1, 2]);
        node.switch = Some(PlaylistSwitch::holding(0.5));
        node.blend_scratch = alloc::vec![0u16; 16];
        node.held.begin_capture().expect("reserve");
        node.held.capture(&[1, 2, 3, 4]).expect("capture");
        node.held.finish_capture();

        node.end_switch();

        assert_eq!(node.switch, None);
        assert!(!node.held.holds_memory());
        assert_eq!(node.blend_scratch.capacity(), 0);
    }

    #[test]
    fn blending_from_the_held_frame_pads_with_black() {
        let mut out = [0u16; 4];
        blend_rgba16_samples(&[1000, 1000], &[3000, 3000, 3000, 3000], 0.5, &mut out)
            .expect("blend");
        assert_eq!(out, [2000, 2000, 1500, 1500]);
    }

    /// Entry 1 loaded (the idle entry), the rest dormant.
    fn playlist_with_entries(keys: &[u32]) -> PlaylistNode {
        let entries = keys
            .iter()
            .map(|&index| {
                let mut entry = PlaylistRuntimeEntry::dormant(index);
                entry.duration = Some(4.0);
                if index == keys[0] {
                    entry = entry.loaded(NodeId::new(100 + index));
                }
                entry
            })
            .collect();
        PlaylistNode::new(NodeId::new(1), keys[0], 0.35, entries)
    }
}
