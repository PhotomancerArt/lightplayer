//! Timebase store: engine-owned phasor/seconds state keyed by clock node.
//!
//! A `TimeProduct` on the bus is a pure handle. Everything it can answer —
//! effective seconds, this tick's delta, a wrapped `[0,1)` phasor — is
//! serviced from this side store instead of by dispatching back into the
//! producing node. That is deliberate: a shader may ask for a dozen phasors
//! per tick, and routing each through the resolver's node-call machinery
//! (take-the-node-out-of-the-tree, `Executing` guard, panic recovery) would
//! put the frame-cost lead on the hottest new path. A `VecMap` lookup plus a
//! multiply-add is what a timebase read should cost.
//!
//! Store, not bindings (the `panel_writers` precedent): this is Engine state,
//! so it survives `apply_project_changes`, which rebuilds bindings from defs
//! and would silently destroy anything registered as one. Nothing here is
//! ever persisted — phasors materialize on demand and despawn on silence.
//!
//! Two contracts worth stating out loud:
//!
//! - **The store keeps the raw ramp.** `Waveform` and `phase_offset` are
//!   applied by evaluators on the way out, never here (parent D8). Two
//!   consumers reading the same shared phasor with different waveforms must
//!   see the same underlying cycle.
//! - **State carries no config.** Period arrives with every query, so
//!   re-authoring it changes the rate from that instant forward without
//!   resetting the phase. The exceptions are *witnesses*:
//!   [`PhasorState::period_seconds`] records the period the integrator last
//!   ran at, and [`PhasorState::readings`] records who queried it and with
//!   what output shaping — both purely so the studio's probe rows can say
//!   what is riding a clock. Nothing reads either back into the
//!   integration, so the contract above is intact.
//!
//! Firmware-safe by construction: `VecMap`, `no_std` + alloc, and wrapping
//! done with plain arithmetic (no `std::f32`, no `libm`).
//!
//! # Two ways to answer "what phase is it?"
//!
//! With `feature = "scrub-log"` on (every host and sim tier), each phasor
//! also keeps a **breakpoint log**: one [`Breakpoint`] per moment its
//! effective rate changed, and nothing in between. Phase is then evaluated in
//! closed form from the segment covering the clock's effective time —
//! `fract(phase + rate·(t − t_eff))` — which is what makes scrubbing exact.
//! Dragging a clock back to a time already seen re-runs the *same*
//! expression from the *same* breakpoint and lands on the same bits, where
//! re-integrating a different sequence of deltas would not.
//!
//! With the feature off (firmware), the store keeps only the forward
//! integrator P2 shipped — `phase += rate · delta`, no allocation at all —
//! and a backward `scrub_offset` write arrives as a negative delta that
//! wraps the phase downward (parent D6's monotone-consistent device
//! behavior).

#[cfg(feature = "scrub-log")]
use alloc::vec;
use alloc::vec::Vec;

use lp_collection::VecMap;
use lpc_model::{ChannelName, NodeId, PhasorConfig, Revision, SlotPath, Waveform};

use crate::node::ScopeRef;

/// Store ticks a phasor may go unqueried before it despawns.
///
/// ~2 s at 60 fps. Long enough that a consumer skipped for a frame or two
/// (a playlist entry off-screen, a paused preview) keeps its phase; short
/// enough that a deleted shader's phasors do not accumulate on a device.
pub const PHASOR_IDLE_TICKS: u32 = 120;

/// Cap on recorded readings per phasor.
///
/// A working bound, not a wall a real project should hit: eight consumers on
/// ONE integrator is already a crowded clock face. A ninth reader is dropped
/// (with a debug line) rather than evicting an earlier one — the alternative
/// would make the face flicker as readers fight over the last slot. Recorded
/// on every tier: a connected board serves the studio's timebase probe too.
pub const PHASOR_READINGS_CAP: usize = 8;

/// How far behind the live edge a phasor stays reconstructable, in seconds of
/// effective clock time.
///
/// Breakpoints older than this are dropped on the next append. 30 s is a
/// working window, not a history: it is long enough to cover the scrub a
/// studio user actually performs (drag back a few seconds, watch, come
/// forward) and short enough that a session left running for an hour with a
/// busy config does not accumulate. The oldest breakpoint at or before the
/// cutoff is always kept — it is what anchors the segment covering the
/// window's own start.
#[cfg(feature = "scrub-log")]
pub const SCRUB_WINDOW_SECONDS: f32 = 30.0;

/// Hard cap on breakpoints kept per phasor.
///
/// A safety, not a working limit: breakpoints are event-sparse by
/// construction (one per *rate change*, never per frame), so a phasor that
/// reaches 256 inside one 30 s window is being re-authored ~9×/s — a dragged
/// period knob, or a defect. Either way the oldest entries go and a debug
/// line says so.
#[cfg(feature = "scrub-log")]
pub const SCRUB_LOG_CAP: usize = 256;

/// One point where a phasor's rate changed, and the ramp position it changed
/// at.
///
/// The log is a list of these; the segment between two of them is a straight
/// line in phase, so any time inside it is answerable in closed form. Copy +
/// 16 bytes: the whole point is that this is cheap enough to keep a few
/// dozen of per phasor on a host.
#[cfg(feature = "scrub-log")]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Breakpoint {
    /// Effective clock time this segment starts at.
    pub t_eff: f32,
    /// Wrapped `[0,1)` ramp position at `t_eff`.
    pub phase: f32,
    /// Completed cycles at `t_eff`.
    pub cycle: u32,
    /// Cycles per second from `t_eff` until the next breakpoint.
    pub rate: f32,
}

/// Provenance identity of one phasor integrator.
///
/// Never caller-chosen (parent D3): the key falls out of where the resolved
/// config came from. A slot-local config gets a `Private` key, so two nodes
/// reading "a 4-second ramp" each get their own phase; a channel-driven
/// config gets a `Shared` key, so every consumer of that channel rides one
/// integrator. The private↔shared transition therefore changes the key,
/// which resets the phase — the intended "grabbing the reins" behavior.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PhasorKey {
    /// Config authored on (or defaulted for) one node's slot.
    Private { node: NodeId, slot: SlotPath },
    /// Config driven by a bus channel — one integrator for every reader.
    Shared {
        scope: ScopeRef,
        channel: ChannelName,
    },
}

/// One downstream reading of a phasor: who queried it, and how they shape
/// the raw ramp on the way out.
///
/// A **witness** like [`PhasorState::period_seconds`]: recorded so the
/// studio's trace cards can draw each consumer's actual waveform, never read
/// back into integration. Shaping stays applied by evaluators (the store's
/// raw-ramp contract is untouched).
#[derive(Clone, Debug, PartialEq)]
pub struct PhasorReading {
    /// The consuming node.
    pub node: NodeId,
    /// The consumed slot on that node — the uniform's own path, the same
    /// representation [`PhasorKey::Private`] keys by.
    pub slot: SlotPath,
    /// The waveform this reader shapes the ramp with.
    pub waveform: Waveform,
    /// The phase offset this reader adds before shaping.
    pub phase_offset: f32,
}

/// One phasor's integrator. Config-free by design — the two witness fields
/// (`period_seconds`, `readings`) are probe fodder, never inputs.
#[derive(Clone, Debug, PartialEq)]
pub struct PhasorState {
    /// Wrapped cycle position, always in `[0,1)`.
    pub phase: f32,
    /// The period the most recent query integrated at, in seconds.
    ///
    /// A **witness**, not config: the store never reads it back (every
    /// advance takes the period from the query's own `PhasorConfig`). It
    /// exists so the studio's read-only phasor rows can say what rate a
    /// live integrator is running at — the studio cannot re-derive it for a
    /// `Shared` key without redoing the resolver's work, and for a shared
    /// integrator the winning period is precisely what a reader wants to
    /// see. `0.0` on a phasor materialized but never advanced.
    pub period_seconds: f32,
    /// Completed cycles since materialization (saturating both ways).
    pub cycle: u32,
    /// Store tick this phasor last advanced in — the advance-once-per-tick
    /// key.
    advanced_at: u32,
    /// Store tick this phasor was last queried in — the despawn clock.
    last_queried_at: u32,
    /// Who is riding this integrator, capped at [`PHASOR_READINGS_CAP`].
    ///
    /// Refreshed by every tick-side query (read-only render queries never
    /// record); dropped with the phasor when it despawns — a reader that
    /// goes silent ages out with the integrator itself, no separate GC.
    readings: Vec<PhasorReading>,
}

impl PhasorState {
    fn materialized(tick: u32) -> Self {
        Self {
            phase: 0.0,
            period_seconds: 0.0,
            cycle: 0,
            // Deliberately not `tick`: a phasor that materializes mid-tick
            // still owes this tick's advance to the query that created it.
            // Whether that first query advances is decided by the caller
            // (see `phasor_tick`), not by pretending the tick already ran.
            advanced_at: tick.wrapping_sub(1),
            last_queried_at: tick,
            readings: Vec::new(),
        }
    }

    /// Everyone currently riding this integrator, in first-seen order.
    #[must_use]
    pub fn readings(&self) -> &[PhasorReading] {
        &self.readings
    }

    /// Record (or refresh) `reader`'s shaping. An existing `(node, slot)`
    /// updates in place; a newcomer past the cap is dropped with a debug
    /// line rather than evicting an earlier reader.
    fn record_reading(&mut self, reader: (NodeId, &SlotPath), config: &PhasorConfig) {
        let (node, slot) = reader;
        if let Some(existing) = self
            .readings
            .iter_mut()
            .find(|reading| reading.node == node && &reading.slot == slot)
        {
            existing.waveform = config.waveform;
            existing.phase_offset = config.phase_offset;
        } else if self.readings.len() < PHASOR_READINGS_CAP {
            self.readings.push(PhasorReading {
                node,
                slot: slot.clone(),
                waveform: config.waveform,
                phase_offset: config.phase_offset,
            });
        } else {
            log::debug!(
                "phasor readings cap ({PHASOR_READINGS_CAP}) reached; dropping reading \
                 {node:?}:{slot}"
            );
        }
    }
}

/// One clock's published timebase plus the phasors riding on it.
#[derive(Clone, Debug, Default)]
pub struct TimebaseEntry {
    /// The clock's effective project time (accumulated + scrub offset).
    pub effective_seconds: f32,
    /// Seconds this timebase advanced during the most recent tick. May be
    /// negative when a device scrubs backwards.
    pub delta_seconds: f32,
    /// Engine revision of the most recent clock update.
    pub updated_at: Revision,
    phasors: VecMap<PhasorKey, PhasorState>,
    /// Per-phasor breakpoint log, in ascending `t_eff` order and never empty
    /// once its phasor has materialized.
    #[cfg(feature = "scrub-log")]
    logs: VecMap<PhasorKey, Vec<Breakpoint>>,
    /// The furthest effective time this timebase has ever run to.
    ///
    /// `None` until the first phasor query. Everything at or past it is live
    /// (integrate, log, advance the edge); anything behind it is a scrub, and
    /// answers out of the log without disturbing a thing.
    #[cfg(feature = "scrub-log")]
    live_edge: Option<f32>,
}

impl TimebaseEntry {
    /// How many phasors currently ride this timebase.
    #[must_use]
    pub fn phasor_count(&self) -> usize {
        self.phasors.len()
    }

    /// The furthest effective time this timebase has run to, or `None` before
    /// its first phasor query. A query behind this is a scrub.
    #[cfg(feature = "scrub-log")]
    #[must_use]
    pub fn live_edge(&self) -> Option<f32> {
        self.live_edge
    }

    /// One phasor's breakpoint log, oldest first. Empty for a phasor that has
    /// never materialized.
    #[cfg(feature = "scrub-log")]
    #[must_use]
    pub fn breakpoints(&self, key: &PhasorKey) -> &[Breakpoint] {
        self.logs.get(key).map_or(&[], |log| log.as_slice())
    }

    /// Every live phasor on this timebase, in store order.
    ///
    /// Read-only debug surface (the studio's clock-face phasor listing —
    /// parent D10). Iterating never materializes, advances, or refreshes the
    /// despawn clock, so watching the rows cannot keep a dead phasor alive.
    pub fn phasors(&self) -> impl Iterator<Item = (&PhasorKey, &PhasorState)> {
        self.phasors.iter()
    }

    /// Materialize `key` if this is its first query, and hand back a mutable
    /// integrator. Shared by both tick paths.
    fn state_mut(&mut self, key: &PhasorKey, tick: u32) -> &mut PhasorState {
        if self.phasors.get(key).is_none() {
            self.phasors
                .insert(key.clone(), PhasorState::materialized(tick));
        }
        self.phasors
            .get_mut(key)
            .expect("phasor was just materialized")
    }

    /// Forward-only tick (firmware): advance once per store tick by
    /// `rate · delta`, wrapping in either direction.
    #[cfg(not(feature = "scrub-log"))]
    fn phasor_tick(
        &mut self,
        key: &PhasorKey,
        config: &PhasorConfig,
        tick: u32,
        reader: (NodeId, &SlotPath),
    ) -> (f32, u32) {
        let delta_seconds = self.delta_seconds;
        let state = self.state_mut(key, tick);
        state.last_queried_at = tick;
        state.record_reading(reader, config);
        if state.advanced_at != tick {
            state.advanced_at = tick;
            state.period_seconds = config.period_seconds;
            advance(state, config.rate_hz() * delta_seconds);
        }
        (state.phase, state.cycle)
    }

    /// Logged tick (hosts): answer from the segment covering the clock's
    /// effective time, and record a breakpoint when the rate changes.
    ///
    /// Three cases, and the difference between them is the whole feature:
    ///
    /// - **Live, same rate** — evaluate the last segment at `t` and carry the
    ///   live edge forward. This is the ordinary frame, and it costs one
    ///   multiply-add; no allocation, no append.
    /// - **Rate changed** — evaluate the *old* segment at `t` (that is the
    ///   position the change happens at, so phase is continuous across it),
    ///   append a breakpoint there, and run on the new rate from `t`. If the
    ///   clock was scrubbed back when this happened it is a **punch-in**
    ///   (parent D6): the future being overwritten was provisional, so every
    ///   phasor on this timebase drops its breakpoints past `t` and the live
    ///   edge resets to here.
    /// - **Scrubbed, same rate** — a pure read. Nothing is appended, the live
    ///   edge does not move, and the integrator is not disturbed, which is
    ///   exactly why coming back to the live edge continues from the
    ///   pre-scrub state.
    #[cfg(feature = "scrub-log")]
    fn phasor_tick(
        &mut self,
        key: &PhasorKey,
        config: &PhasorConfig,
        tick: u32,
        reader: (NodeId, &SlotPath),
    ) -> (f32, u32) {
        let t = self.effective_seconds;
        let rate = config.rate_hz();

        if self.phasors.get(key).is_none() {
            // The materializing query still owes this tick's delta (P2's
            // contract), so the first segment starts where the tick started —
            // not at `t`, which would answer 0 for a frame.
            let opening = Breakpoint {
                t_eff: t - self.delta_seconds,
                phase: 0.0,
                cycle: 0,
                rate,
            };
            self.logs.insert(key.clone(), vec![opening]);
        }
        {
            let state = self.state_mut(key, tick);
            state.last_queried_at = tick;
            // Before the advance-once early return: a second consumer of a
            // `Shared` key in the same tick is a distinct reading, and this
            // is its only chance to say so.
            state.record_reading(reader, config);
            if state.advanced_at == tick {
                // Advance-once-per-tick: later consumers in the same tick see
                // what the first one saw.
                return (state.phase, state.cycle);
            }
            state.advanced_at = tick;
            state.period_seconds = config.period_seconds;
        }

        let scrubbed = self.live_edge.is_some_and(|edge| t < edge);
        let (active, latest) = {
            let log = self.logs.get(key).expect("log materialized with phasor");
            (
                log[active_segment(log, t)],
                *log.last().expect("a log is never empty"),
            )
        };
        // Evaluate on the segment covering `t`, but decide whether the *config*
        // changed against the newest breakpoint — the rate this phasor is
        // currently authored at. Comparing against the covering segment
        // instead would read every scrub into history as a config write:
        // scrub back past an old period edit and the query, still carrying
        // today's period, would punch in and delete the very history it was
        // asking for.
        let value = eval_segment(&active, t);

        if rate == latest.rate {
            if !scrubbed {
                self.live_edge = Some(t);
            }
        } else {
            if scrubbed {
                punch_in(&mut self.logs, t);
            }
            let log = self
                .logs
                .get_mut(key)
                .expect("log materialized with phasor");
            log.push(Breakpoint {
                t_eff: t,
                phase: value.0,
                cycle: value.1,
                rate,
            });
            trim(log, t);
            self.live_edge = Some(t);
        }

        let state = self.state_mut(key, tick);
        state.phase = value.0;
        state.cycle = value.1;
        value
    }
}

/// The segment covering `t`: the last breakpoint at or before it.
///
/// Falls back to the oldest breakpoint when `t` is older than the whole log
/// (trimmed away, or a phasor that materialized after `t`). Evaluating that
/// segment backwards is an extrapolation, not history — but it is continuous
/// with the window's edge and keeps a scrub past the window from reading as a
/// frozen phasor.
#[cfg(feature = "scrub-log")]
fn active_segment(log: &[Breakpoint], t: f32) -> usize {
    log.iter().rposition(|bp| bp.t_eff <= t).unwrap_or(0)
}

/// Where `segment` has got to at effective time `t`.
///
/// The one expression the whole feature rests on: the live path and a
/// scrubbed reconstruction both come through here with the same breakpoint
/// and the same `t`, so they cannot disagree in the last bit.
#[cfg(feature = "scrub-log")]
fn eval_segment(segment: &Breakpoint, t: f32) -> (f32, u32) {
    let (phase, whole) = wrap_unit(segment.phase + segment.rate * (t - segment.t_eff));
    (phase, saturating_offset(segment.cycle, whole))
}

/// Drop the overwritten future: every breakpoint past `t`, on every phasor of
/// one timebase.
///
/// Timebase-wide rather than per-phasor because the live edge is a property
/// of the clock. Once a config write lands at `t`, no phasor's recorded
/// history past `t` is what will be played again — and a stale breakpoint out
/// there would be picked up by a later scrub as if it had been.
///
/// A log never goes empty: a phasor that materialized inside the discarded
/// stretch keeps its opening breakpoint, and [`active_segment`] extrapolates
/// from it.
#[cfg(feature = "scrub-log")]
fn punch_in(logs: &mut VecMap<PhasorKey, Vec<Breakpoint>>, t: f32) {
    for (_, log) in logs.iter_mut() {
        let keep = log.iter().take_while(|bp| bp.t_eff <= t).count().max(1);
        log.truncate(keep);
    }
}

/// Drop breakpoints that have fallen out of the scrub window, keeping the one
/// that anchors its start, then enforce [`SCRUB_LOG_CAP`].
#[cfg(feature = "scrub-log")]
fn trim(log: &mut Vec<Breakpoint>, t: f32) {
    let cutoff = t - SCRUB_WINDOW_SECONDS;
    let anchor = log.iter().rposition(|bp| bp.t_eff <= cutoff).unwrap_or(0);
    if anchor > 0 {
        log.drain(..anchor);
    }
    if log.len() > SCRUB_LOG_CAP {
        let excess = log.len() - SCRUB_LOG_CAP;
        log.drain(..excess);
        log::debug!(
            "phasor breakpoint log hit its {SCRUB_LOG_CAP} cap at t={t}; dropped {excess} \
             oldest entries (a rate changing this often is a dragged knob or a defect)"
        );
    }
}

/// Engine-owned map of clock `NodeId` → timebase.
#[derive(Debug, Default)]
pub struct TimebaseStore {
    entries: VecMap<NodeId, TimebaseEntry>,
    /// Monotonic store tick, bumped once per [`TimebaseStore::sweep`].
    ///
    /// Deliberately NOT [`Revision`], despite revisions being the engine's
    /// usual clock: `advance_revision` is a process-global counter, so a
    /// second engine (or a parallel test) sharing the process inflates the
    /// gap between two of *this* engine's ticks. Advance-once only compares
    /// for equality and would survive that; the despawn horizon would not —
    /// phasors would evaporate early and non-deterministically. A counter
    /// this store owns cannot drift.
    tick: u32,
}

impl TimebaseStore {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Publish a clock's timebase for this tick. Called from the clock node's
    /// `produce`; creates the entry on first write.
    pub fn set_timebase(
        &mut self,
        clock: NodeId,
        effective_seconds: f32,
        delta_seconds: f32,
        at: Revision,
    ) {
        match self.entries.get_mut(&clock) {
            Some(entry) => {
                entry.effective_seconds = effective_seconds;
                entry.delta_seconds = delta_seconds;
                entry.updated_at = at;
            }
            None => {
                self.entries.insert(
                    clock,
                    TimebaseEntry {
                        effective_seconds,
                        delta_seconds,
                        updated_at: at,
                        ..TimebaseEntry::default()
                    },
                );
            }
        }
    }

    /// The clock's effective seconds, or `None` when it has never produced.
    #[must_use]
    pub fn seconds(&self, clock: NodeId) -> Option<f32> {
        self.entries
            .get(&clock)
            .map(|entry| entry.effective_seconds)
    }

    /// The clock's most recent per-tick delta.
    #[must_use]
    pub fn delta(&self, clock: NodeId) -> Option<f32> {
        self.entries.get(&clock).map(|entry| entry.delta_seconds)
    }

    /// Tick-side phasor query: materialize on first ask, advance once per
    /// store tick, return the raw wrapped ramp.
    ///
    /// `reader` names who is asking — the consuming node and the consumed
    /// slot the config was resolved for. Recorded as a witness (see
    /// [`PhasorState::readings`]); it never affects the answer.
    ///
    /// `None` only when the clock has no timebase yet (nothing has produced
    /// it) — an unmaterialized phasor is not an error, it is a birth.
    pub fn phasor_tick(
        &mut self,
        clock: NodeId,
        key: &PhasorKey,
        config: &PhasorConfig,
        reader: (NodeId, &SlotPath),
    ) -> Option<(f32, u32)> {
        let tick = self.tick;
        let entry = self.entries.get_mut(&clock)?;
        Some(entry.phasor_tick(key, config, tick, reader))
    }

    /// Render-side phasor query: read whatever the tick left behind.
    ///
    /// Render can run more than once per tick (and outside a tick entirely,
    /// for probes), so it must never materialize or advance — that would
    /// make a phasor's rate depend on how many previews happen to be open.
    /// An unmaterialized phasor reads as the start of its first cycle.
    #[must_use]
    pub fn phasor_read(&self, clock: NodeId, key: &PhasorKey) -> Option<(f32, u32)> {
        let entry = self.entries.get(&clock)?;
        Some(
            entry
                .phasors
                .get(key)
                .map_or((0.0, 0), |state| (state.phase, state.cycle)),
        )
    }

    /// End-of-tick maintenance: drop timebases whose clock left the tree,
    /// despawn phasors nothing has asked for in [`PHASOR_IDLE_TICKS`], then
    /// bump the store tick.
    ///
    /// Returns the number of phasors and entries dropped.
    pub fn sweep(&mut self, clock_is_live: impl Fn(NodeId) -> bool) -> usize {
        let tick = self.tick;
        let mut dropped = 0;
        self.entries.retain(|clock, entry| {
            if !clock_is_live(*clock) {
                dropped += 1 + entry.phasors.len();
                return false;
            }
            let before = entry.phasors.len();
            entry
                .phasors
                .retain(|_, state| tick.wrapping_sub(state.last_queried_at) < PHASOR_IDLE_TICKS);
            dropped += before - entry.phasors.len();
            // A despawned phasor's history goes with it: re-materializing
            // starts a fresh cycle at zero, so the old segments describe a
            // ramp nothing will ever replay.
            #[cfg(feature = "scrub-log")]
            {
                let phasors = &entry.phasors;
                entry.logs.retain(|key, _| phasors.get(key).is_some());
            }
            true
        });
        self.tick = self.tick.wrapping_add(1);
        dropped
    }

    /// The store's own monotonic tick (advance-once and despawn clock).
    #[must_use]
    pub fn tick(&self) -> u32 {
        self.tick
    }

    /// The timebase entry for `clock`, if one exists.
    #[must_use]
    pub fn entry(&self, clock: NodeId) -> Option<&TimebaseEntry> {
        self.entries.get(&clock)
    }

    /// Number of clocks with a published timebase.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Advance a phasor by `advance` cycles and re-wrap into `[0,1)`.
///
/// The forward-only (firmware) integrator. With `scrub-log` on, phase comes
/// from [`eval_segment`] instead — the log's closed form is what a scrubbed
/// read has to agree with, bit for bit.
///
/// Handles a negative advance (a device scrubbing backwards) by wrapping
/// downward and stepping the cycle counter back, and refuses to act on a
/// non-finite advance rather than poisoning the integrator with a NaN it can
/// never recover from.
#[cfg(not(feature = "scrub-log"))]
fn advance(state: &mut PhasorState, advance: f32) {
    if !advance.is_finite() || advance == 0.0 {
        return;
    }
    let (phase, whole) = wrap_unit(state.phase + advance);
    state.phase = phase;
    state.cycle = saturating_offset(state.cycle, whole);
}

/// Split `value` into its `[0,1)` fraction and the number of whole cycles it
/// crossed. Manual arithmetic on purpose — `f32::floor` is `std`, and this
/// path must compile for every firmware tier.
fn wrap_unit(value: f32) -> (f32, i64) {
    if !value.is_finite() {
        return (0.0, 0);
    }
    // `as i64` truncates toward zero and saturates at the bounds, so a wild
    // advance clamps instead of wrapping into nonsense.
    let mut whole = value as i64;
    let mut frac = value - whole as f32;
    if frac < 0.0 {
        frac += 1.0;
        whole -= 1;
    }
    // A tiny negative fraction can round to exactly 1.0 when 1.0 is added.
    // `[0,1)` is a promise, so close the interval by hand.
    if frac >= 1.0 {
        frac = 0.0;
        whole += 1;
    }
    (frac, whole)
}

fn saturating_offset(cycle: u32, delta: i64) -> u32 {
    let next = i64::from(cycle).saturating_add(delta);
    next.clamp(0, i64::from(u32::MAX)) as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_model::Waveform;

    const CLOCK: NodeId = NodeId(1);
    /// The reader identity the tests query as, unless a test is about
    /// readings specifically.
    const READER_NODE: NodeId = NodeId(9);

    fn reader() -> SlotPath {
        SlotPath::parse("reader").expect("slot path")
    }

    fn key(slot: &str) -> PhasorKey {
        PhasorKey::Private {
            node: NodeId(9),
            slot: SlotPath::parse(slot).expect("slot path"),
        }
    }

    fn store_with_delta(delta: f32) -> TimebaseStore {
        let mut store = TimebaseStore::new();
        store.set_timebase(CLOCK, 0.0, delta, Revision::new(1));
        store
    }

    /// End the tick and publish the next one, moving effective time by
    /// exactly the delta it reports.
    ///
    /// A real clock always publishes those two numbers in agreement (`P8`
    /// made the second one the difference of the first), and the breakpoint
    /// log keys on effective time — so a fixture that advanced the delta
    /// while holding the clock still would be describing a timebase nothing
    /// can produce.
    fn tick(store: &mut TimebaseStore, delta: f32) {
        store.sweep(|_| true);
        let now = store.seconds(CLOCK).unwrap_or(0.0);
        store.set_timebase(CLOCK, now + delta, delta, Revision::new(1));
    }

    /// Publish a scrub: effective time jumps to `to`, and the delta reports
    /// the jump — which is what the studio's `scrub_offset_seconds` slider
    /// produces through the clock.
    fn scrub_to(store: &mut TimebaseStore, to: f32) {
        store.sweep(|_| true);
        let now = store.seconds(CLOCK).unwrap_or(0.0);
        store.set_timebase(CLOCK, to, to - now, Revision::new(1));
    }

    /// Five `u32`-sized fields plus one `Vec` header (the readings witness,
    /// its entries heap-allocated and capped at [`PHASOR_READINGS_CAP`]).
    /// The cap stays deliberately tight: a device carries one of these per
    /// integrator, so anything that grows the inline size should have to
    /// argue for itself. 48 covers a 64-bit host; 32-bit device targets sit
    /// at 32.
    #[test]
    fn phasor_state_stays_small() {
        assert!(
            core::mem::size_of::<PhasorState>() <= 48,
            "PhasorState grew to {} bytes",
            core::mem::size_of::<PhasorState>()
        );
    }

    /// The period witness records what the integrator last RAN at — it
    /// follows a config change, and it never feeds back into integration
    /// (the continuity test above already pins that).
    #[test]
    fn the_period_witness_follows_the_config_the_query_supplied() {
        let mut store = store_with_delta(0.1);

        store.phasor_tick(
            CLOCK,
            &key("a"),
            &PhasorConfig::with_period(4.0),
            (READER_NODE, &reader()),
        );
        let entry = store.entry(CLOCK).expect("timebase");
        let (_, state) = entry.phasors().next().expect("one phasor");
        assert_eq!(state.period_seconds, 4.0);

        tick(&mut store, 0.1);
        store.phasor_tick(
            CLOCK,
            &key("a"),
            &PhasorConfig::with_period(0.5),
            (READER_NODE, &reader()),
        );
        let rows: alloc::vec::Vec<_> = store
            .entry(CLOCK)
            .expect("timebase")
            .phasors()
            .map(|(key, state)| (key.clone(), state.period_seconds))
            .collect();
        assert_eq!(rows.len(), 1, "listing does not duplicate the integrator");
        assert_eq!(rows[0].0, key("a"));
        assert_eq!(rows[0].1, 0.5);
    }

    // --- Readings witness (clock-face-v2 P1) -------------------------------

    /// One phasor state's readings, cloned out for assertion.
    fn readings_of(store: &TimebaseStore, key: &PhasorKey) -> alloc::vec::Vec<PhasorReading> {
        store
            .entry(CLOCK)
            .expect("timebase")
            .phasors()
            .find(|(k, _)| *k == key)
            .map(|(_, state)| state.readings().to_vec())
            .unwrap_or_default()
    }

    #[test]
    fn a_tick_query_records_its_reader_and_shaping() {
        let mut store = store_with_delta(0.1);
        let config = PhasorConfig {
            period_seconds: 2.0,
            waveform: Waveform::Sine,
            phase_offset: 0.25,
        };

        store.phasor_tick(CLOCK, &key("a"), &config, (READER_NODE, &reader()));

        assert_eq!(
            readings_of(&store, &key("a")),
            alloc::vec![PhasorReading {
                node: READER_NODE,
                slot: reader(),
                waveform: Waveform::Sine,
                phase_offset: 0.25,
            }]
        );
    }

    /// A reader re-querying — same tick or a later one — refreshes its
    /// shaping in place instead of stacking duplicate rows.
    #[test]
    fn the_same_reader_updates_its_shaping_in_place() {
        let mut store = store_with_delta(0.1);
        let sine = PhasorConfig {
            period_seconds: 2.0,
            waveform: Waveform::Sine,
            phase_offset: 0.0,
        };
        let square = PhasorConfig {
            period_seconds: 2.0,
            waveform: Waveform::Square,
            phase_offset: 0.5,
        };

        store.phasor_tick(CLOCK, &key("a"), &sine, (READER_NODE, &reader()));
        tick(&mut store, 0.1);
        store.phasor_tick(CLOCK, &key("a"), &square, (READER_NODE, &reader()));

        let readings = readings_of(&store, &key("a"));
        assert_eq!(readings.len(), 1, "one reader, one reading: {readings:?}");
        assert_eq!(readings[0].waveform, Waveform::Square);
        assert_eq!(readings[0].phase_offset, 0.5);
    }

    /// Two consumers riding one `Shared` integrator in the SAME tick are two
    /// readings — the second query takes the advance-once early return, and
    /// that return must not swallow its reading.
    #[test]
    fn two_readers_on_one_shared_key_are_two_readings() {
        let mut store = store_with_delta(0.1);
        let shared = PhasorKey::Shared {
            scope: ScopeRef::Module { owner: NodeId(1) },
            channel: ChannelName("phase".into()),
        };
        let ramp = PhasorConfig::with_period(1.0);
        let sine = PhasorConfig {
            period_seconds: 1.0,
            waveform: Waveform::Sine,
            phase_offset: 0.0,
        };
        let slot_a = SlotPath::parse("a").expect("slot path");
        let slot_b = SlotPath::parse("b").expect("slot path");

        store.phasor_tick(CLOCK, &shared, &ramp, (NodeId(7), &slot_a));
        store.phasor_tick(CLOCK, &shared, &sine, (NodeId(8), &slot_b));

        let readings = readings_of(&store, &shared);
        assert_eq!(readings.len(), 2, "{readings:?}");
        assert_eq!(
            (readings[0].node, readings[0].waveform),
            (NodeId(7), Waveform::Ramp)
        );
        assert_eq!(
            (readings[1].node, readings[1].waveform),
            (NodeId(8), Waveform::Sine)
        );
        assert_eq!(
            store.entry(CLOCK).expect("timebase").phasor_count(),
            1,
            "two readings, still one integrator"
        );
    }

    /// The cap drops the NEWCOMER (with a debug line), never an established
    /// reader — a face flickering as readers fight over the last slot would
    /// be worse than a truncated listing.
    #[test]
    fn the_reading_past_the_cap_is_dropped() {
        let mut store = store_with_delta(0.1);
        let shared = PhasorKey::Shared {
            scope: ScopeRef::Module { owner: NodeId(1) },
            channel: ChannelName("phase".into()),
        };
        let config = PhasorConfig::with_period(1.0);

        for n in 0..=PHASOR_READINGS_CAP as u32 {
            let slot = SlotPath::parse("phase").expect("slot path");
            store.phasor_tick(CLOCK, &shared, &config, (NodeId(100 + n), &slot));
        }

        let readings = readings_of(&store, &shared);
        assert_eq!(readings.len(), PHASOR_READINGS_CAP);
        assert!(
            readings.iter().all(|r| r.node != NodeId(108)),
            "the ninth reader is the one dropped: {readings:?}"
        );

        // …and an established reader still refreshes in place at the cap.
        let slot = SlotPath::parse("phase").expect("slot path");
        let square = PhasorConfig {
            period_seconds: 1.0,
            waveform: Waveform::Square,
            phase_offset: 0.0,
        };
        tick(&mut store, 0.1);
        store.phasor_tick(CLOCK, &shared, &square, (NodeId(100), &slot));
        let readings = readings_of(&store, &shared);
        assert_eq!(readings.len(), PHASOR_READINGS_CAP);
        assert_eq!(readings[0].waveform, Waveform::Square);
    }

    /// Render-phase reads are pure: no advance, no materialization — and no
    /// reading either, or every open preview would appear on the face.
    #[test]
    fn a_render_read_records_no_reading() {
        let mut store = store_with_delta(0.1);
        store.phasor_tick(
            CLOCK,
            &key("a"),
            &PhasorConfig::with_period(1.0),
            (READER_NODE, &reader()),
        );

        let _ = store.phasor_read(CLOCK, &key("a"));

        assert_eq!(readings_of(&store, &key("a")).len(), 1);
    }

    /// The listing is a pure read: walking it must not materialize a phasor
    /// nor hold a stale one past its despawn horizon.
    #[test]
    fn listing_phasors_neither_materializes_nor_keeps_alive() {
        let mut store = store_with_delta(0.25);
        assert_eq!(store.entry(CLOCK).expect("timebase").phasors().count(), 0);

        store.phasor_tick(
            CLOCK,
            &key("a"),
            &PhasorConfig::with_period(1.0),
            (READER_NODE, &reader()),
        );
        for _ in 0..=PHASOR_IDLE_TICKS {
            tick(&mut store, 0.25);
            let _ = store.entry(CLOCK).expect("timebase").phasors().count();
        }

        assert_eq!(store.entry(CLOCK).expect("timebase").phasors().count(), 0);
    }

    #[test]
    fn a_phasor_materializes_at_zero_and_advances_by_rate_times_delta() {
        let mut store = store_with_delta(0.5);
        let config = PhasorConfig::with_period(2.0);

        // The materializing query advances too: the tick's delta is owed.
        assert_eq!(
            store.phasor_tick(CLOCK, &key("a"), &config, (READER_NODE, &reader())),
            Some((0.25, 0))
        );
        tick(&mut store, 0.5);
        assert_eq!(
            store.phasor_tick(CLOCK, &key("a"), &config, (READER_NODE, &reader())),
            Some((0.5, 0))
        );
    }

    #[test]
    fn a_query_against_an_unpublished_clock_answers_none() {
        let mut store = TimebaseStore::new();

        assert_eq!(
            store.phasor_tick(
                NodeId(42),
                &key("a"),
                &PhasorConfig::default(),
                (READER_NODE, &reader())
            ),
            None
        );
        assert_eq!(store.seconds(NodeId(42)), None);
        assert_eq!(store.delta(NodeId(42)), None);
    }

    #[test]
    fn two_queries_in_one_tick_see_the_same_phase() {
        let mut store = store_with_delta(0.1);
        let config = PhasorConfig::with_period(1.0);

        let first = store
            .phasor_tick(CLOCK, &key("a"), &config, (READER_NODE, &reader()))
            .unwrap();
        let second = store
            .phasor_tick(CLOCK, &key("a"), &config, (READER_NODE, &reader()))
            .unwrap();
        let third = store
            .phasor_tick(CLOCK, &key("a"), &config, (READER_NODE, &reader()))
            .unwrap();

        assert_eq!(first, (0.1, 0));
        assert_eq!(second, first);
        assert_eq!(third, first);
    }

    #[test]
    fn phase_stays_in_unit_range_and_cycles_count_up() {
        let mut store = store_with_delta(0.25);
        let config = PhasorConfig::with_period(1.0);

        for _ in 0..40 {
            let (phase, _) = store
                .phasor_tick(CLOCK, &key("a"), &config, (READER_NODE, &reader()))
                .unwrap();
            assert!((0.0..1.0).contains(&phase), "phase escaped [0,1): {phase}");
            tick(&mut store, 0.25);
        }
        let (phase, cycle) = store.phasor_read(CLOCK, &key("a")).unwrap();
        assert_eq!(cycle, 10);
        assert!(phase.abs() < 1e-4, "phase after 10 whole cycles: {phase}");
    }

    #[test]
    fn a_period_change_does_not_disturb_the_phase() {
        let mut store = store_with_delta(0.1);
        let slow = PhasorConfig::with_period(4.0);
        let fast = PhasorConfig::with_period(1.0);

        for _ in 0..7 {
            store.phasor_tick(CLOCK, &key("a"), &slow, (READER_NODE, &reader()));
            tick(&mut store, 0.1);
        }
        let before = store
            .phasor_tick(CLOCK, &key("a"), &slow, (READER_NODE, &reader()))
            .unwrap();

        // Swapping config with no time elapsed must move nothing at all.
        tick(&mut store, 0.0);
        let at_event = store
            .phasor_tick(CLOCK, &key("a"), &fast, (READER_NODE, &reader()))
            .unwrap();
        assert_eq!(at_event, before);

        // …and the next advance simply uses the new rate from there.
        tick(&mut store, 0.1);
        let after = store
            .phasor_tick(CLOCK, &key("a"), &fast, (READER_NODE, &reader()))
            .unwrap();
        assert!(
            (after.0 - (before.0 + 0.1)).abs() <= f32::EPSILON,
            "expected continuity at the config change: {before:?} -> {after:?}"
        );
    }

    /// A period of 0 freezes the phase where it stood, and restoring the
    /// period resumes from there.
    ///
    /// Both edits land *at* the moment they arrive: the phase at the freeze
    /// is whatever the old rate had reached by then, and the resume returns
    /// that same value before the new rate starts moving it. A config change
    /// changes the slope from here; it never displaces the phase.
    #[test]
    fn a_zero_period_freezes_the_phase_and_resumes_continuously() {
        let mut store = store_with_delta(0.1);
        let running = PhasorConfig::with_period(1.0);
        let frozen = PhasorConfig::with_period(0.0);

        store.phasor_tick(CLOCK, &key("a"), &running, (READER_NODE, &reader()));
        tick(&mut store, 0.1);
        let held = store
            .phasor_tick(CLOCK, &key("a"), &frozen, (READER_NODE, &reader()))
            .unwrap();

        for _ in 0..5 {
            tick(&mut store, 0.1);
            assert_eq!(
                store.phasor_tick(CLOCK, &key("a"), &frozen, (READER_NODE, &reader())),
                Some(held)
            );
        }

        tick(&mut store, 0.1);
        let resumed = store
            .phasor_tick(CLOCK, &key("a"), &running, (READER_NODE, &reader()))
            .unwrap();
        assert!(
            resumed.0 >= held.0 && resumed.0 - held.0 <= 0.1 + f32::EPSILON,
            "restoring the period must not displace the phase: {held:?} -> {resumed:?}"
        );
        // With the log on, the edit lands at exactly the effective time it
        // arrived, so the restored rate has not moved anything yet. The
        // forward-only integrator instead spends this tick's delta at the new
        // rate — a one-frame difference in where a config change sits, and
        // the reason a scrubbed reconstruction has to come from the log.
        #[cfg(feature = "scrub-log")]
        assert_eq!(resumed, held);

        tick(&mut store, 0.1);
        let moving = store
            .phasor_tick(CLOCK, &key("a"), &running, (READER_NODE, &reader()))
            .unwrap();
        assert!(
            (moving.0 - resumed.0 - 0.1).abs() <= f32::EPSILON,
            "…and from there it advances at the restored rate: {resumed:?} -> {moving:?}"
        );
    }

    #[test]
    fn a_zero_delta_freezes_every_phasor_on_the_clock() {
        let mut store = store_with_delta(0.1);
        let config = PhasorConfig::with_period(1.0);
        store.phasor_tick(CLOCK, &key("a"), &config, (READER_NODE, &reader()));
        store.phasor_tick(CLOCK, &key("b"), &config, (READER_NODE, &reader()));

        for _ in 0..4 {
            tick(&mut store, 0.0);
            assert_eq!(
                store.phasor_tick(CLOCK, &key("a"), &config, (READER_NODE, &reader())),
                Some((0.1, 0))
            );
            assert_eq!(
                store.phasor_tick(CLOCK, &key("b"), &config, (READER_NODE, &reader())),
                Some((0.1, 0))
            );
        }
    }

    /// A backward scrub reaches a phasor as a negative delta, and it wraps
    /// downward through the cycle counter.
    ///
    /// This is the device contract (parent D6): firmware keeps no breakpoint
    /// log, so integrating the negative delta is *all* it can do. Both builds
    /// land on the same numbers here, which is what makes the logged path a
    /// refinement of this one rather than a different animal.
    #[test]
    fn a_negative_delta_wraps_downward() {
        let mut store = store_with_delta(0.25);
        let config = PhasorConfig::with_period(1.0);

        store.phasor_tick(CLOCK, &key("a"), &config, (READER_NODE, &reader()));
        for _ in 0..4 {
            tick(&mut store, 0.25);
            store.phasor_tick(CLOCK, &key("a"), &config, (READER_NODE, &reader()));
        }
        assert_eq!(store.phasor_read(CLOCK, &key("a")), Some((0.25, 1)));

        // Back half a cycle, from effective 1.0 to 0.5.
        scrub_to(&mut store, 0.5);
        let (phase, cycle) = store
            .phasor_tick(CLOCK, &key("a"), &config, (READER_NODE, &reader()))
            .unwrap();

        assert!((phase - 0.75).abs() < 1e-6, "phase: {phase}");
        assert_eq!(cycle, 0);
    }

    #[test]
    fn scrubbing_below_the_first_cycle_clamps_the_counter_not_the_phase() {
        let mut store = store_with_delta(-2.5);
        let config = PhasorConfig::with_period(1.0);

        let (phase, cycle) = store
            .phasor_tick(CLOCK, &key("a"), &config, (READER_NODE, &reader()))
            .unwrap();

        assert!((0.0..1.0).contains(&phase), "phase: {phase}");
        assert!((phase - 0.5).abs() < 1e-6, "phase: {phase}");
        assert_eq!(cycle, 0);
    }

    #[test]
    fn a_wild_advance_does_not_poison_the_integrator() {
        let mut store = store_with_delta(f32::INFINITY);
        let config = PhasorConfig::with_period(1.0);

        let (phase, _) = store
            .phasor_tick(CLOCK, &key("a"), &config, (READER_NODE, &reader()))
            .unwrap();
        assert!(phase.is_finite() && (0.0..1.0).contains(&phase));

        store.sweep(|_| true);
        store.set_timebase(CLOCK, 0.0, f32::NAN, Revision::new(2));
        let (phase, _) = store
            .phasor_tick(CLOCK, &key("a"), &config, (READER_NODE, &reader()))
            .unwrap();
        assert!(phase.is_finite() && (0.0..1.0).contains(&phase));
    }

    #[test]
    fn a_render_read_never_materializes_or_advances() {
        let mut store = store_with_delta(0.5);

        assert_eq!(store.phasor_read(CLOCK, &key("a")), Some((0.0, 0)));
        assert_eq!(store.entry(CLOCK).unwrap().phasor_count(), 0);

        store.phasor_tick(
            CLOCK,
            &key("a"),
            &PhasorConfig::with_period(1.0),
            (READER_NODE, &reader()),
        );
        let read = store.phasor_read(CLOCK, &key("a")).unwrap();
        assert_eq!(store.phasor_read(CLOCK, &key("a")).unwrap(), read);
        assert_eq!(read, (0.5, 0));
    }

    #[test]
    fn an_unqueried_phasor_despawns_and_re_materializes_at_zero() {
        let mut store = store_with_delta(0.25);
        let config = PhasorConfig::with_period(1.0);

        store.phasor_tick(CLOCK, &key("a"), &config, (READER_NODE, &reader()));
        assert_eq!(store.entry(CLOCK).unwrap().phasor_count(), 1);

        for _ in 0..PHASOR_IDLE_TICKS {
            tick(&mut store, 0.25);
            assert_eq!(
                store.entry(CLOCK).unwrap().phasor_count(),
                1,
                "despawned early at tick {}",
                store.tick()
            );
        }
        tick(&mut store, 0.25);
        assert_eq!(store.entry(CLOCK).unwrap().phasor_count(), 0);
        assert!(
            readings_of(&store, &key("a")).is_empty(),
            "a despawned phasor's readings go with it — no separate GC"
        );
        #[cfg(feature = "scrub-log")]
        assert!(
            store
                .entry(CLOCK)
                .unwrap()
                .breakpoints(&key("a"))
                .is_empty(),
            "a despawned phasor's history goes with it"
        );

        // The timebase itself survives; only the phasor went.
        assert_eq!(store.delta(CLOCK), Some(0.25));
        assert_eq!(
            store.phasor_tick(CLOCK, &key("a"), &config, (READER_NODE, &reader())),
            Some((0.25, 0))
        );
    }

    #[test]
    fn a_phasor_queried_every_tick_never_despawns() {
        let mut store = store_with_delta(0.0);
        let config = PhasorConfig::with_period(1.0);

        for _ in 0..(PHASOR_IDLE_TICKS * 3) {
            store.phasor_tick(CLOCK, &key("a"), &config, (READER_NODE, &reader()));
            tick(&mut store, 0.0);
        }

        assert_eq!(store.entry(CLOCK).unwrap().phasor_count(), 1);
    }

    #[test]
    fn a_dead_clock_loses_its_whole_timebase() {
        let mut store = store_with_delta(0.25);
        store.set_timebase(NodeId(2), 1.0, 0.25, Revision::new(1));
        store.phasor_tick(
            CLOCK,
            &key("a"),
            &PhasorConfig::default(),
            (READER_NODE, &reader()),
        );

        let dropped = store.sweep(|clock| clock != CLOCK);

        assert_eq!(dropped, 2, "the entry and its one phasor");
        assert_eq!(store.seconds(CLOCK), None);
        assert_eq!(store.seconds(NodeId(2)), Some(1.0));
    }

    #[test]
    fn two_clocks_keep_independent_timebases_and_phasors() {
        let mut store = TimebaseStore::new();
        let outer = NodeId(1);
        let inner = NodeId(2);
        store.set_timebase(outer, 10.0, 0.5, Revision::new(1));
        store.set_timebase(inner, 3.0, 0.1, Revision::new(1));
        let config = PhasorConfig::with_period(1.0);

        let outer_phase = store
            .phasor_tick(outer, &key("a"), &config, (READER_NODE, &reader()))
            .unwrap();
        let inner_phase = store
            .phasor_tick(inner, &key("a"), &config, (READER_NODE, &reader()))
            .unwrap();

        assert_eq!(store.seconds(outer), Some(10.0));
        assert_eq!(store.seconds(inner), Some(3.0));
        // Tolerances, not equality: a materializing query opens its segment
        // one delta behind the clock's *absolute* effective time, and
        // `3.0 - 0.1` is not exactly 2.9 in f32. Scrub-exactness is about a
        // reconstruction matching what was shown (same expression, same
        // inputs, same bits) — never about a phase matching decimal
        // arithmetic.
        assert!((outer_phase.0 - 0.5).abs() < 1e-6, "outer: {outer_phase:?}");
        assert!((inner_phase.0 - 0.1).abs() < 1e-6, "inner: {inner_phase:?}");
        assert_eq!((outer_phase.1, inner_phase.1), (0, 0));
    }

    #[test]
    fn private_and_shared_keys_are_separate_integrators() {
        let mut store = store_with_delta(0.25);
        let config = PhasorConfig::with_period(1.0);
        let private = key("a");
        let shared = PhasorKey::Shared {
            scope: ScopeRef::Module { owner: NodeId(1) },
            channel: ChannelName("phase".into()),
        };

        store.phasor_tick(CLOCK, &private, &config, (READER_NODE, &reader()));
        tick(&mut store, 0.25);
        store.phasor_tick(CLOCK, &private, &config, (READER_NODE, &reader()));
        store.phasor_tick(CLOCK, &shared, &config, (READER_NODE, &reader()));

        assert_eq!(store.phasor_read(CLOCK, &private), Some((0.5, 0)));
        assert_eq!(store.phasor_read(CLOCK, &shared), Some((0.25, 0)));
        assert_eq!(store.entry(CLOCK).unwrap().phasor_count(), 2);
    }

    #[test]
    fn the_same_channel_in_two_scopes_is_two_integrators() {
        let mut store = store_with_delta(0.25);
        let config = PhasorConfig::with_period(1.0);
        let outer = PhasorKey::Shared {
            scope: ScopeRef::Module { owner: NodeId(1) },
            channel: ChannelName("phase".into()),
        };
        let inner = PhasorKey::Shared {
            scope: ScopeRef::Module { owner: NodeId(2) },
            channel: ChannelName("phase".into()),
        };

        store.phasor_tick(CLOCK, &outer, &config, (READER_NODE, &reader()));
        tick(&mut store, 0.25);
        store.phasor_tick(CLOCK, &outer, &config, (READER_NODE, &reader()));
        store.phasor_tick(CLOCK, &inner, &config, (READER_NODE, &reader()));

        assert_eq!(store.phasor_read(CLOCK, &outer), Some((0.5, 0)));
        assert_eq!(store.phasor_read(CLOCK, &inner), Some((0.25, 0)));
    }

    #[test]
    fn the_store_ignores_waveform_and_offset() {
        let mut store = store_with_delta(0.25);
        let ramp = PhasorConfig {
            period_seconds: 1.0,
            waveform: Waveform::Ramp,
            phase_offset: 0.0,
        };
        let shaped = PhasorConfig {
            period_seconds: 1.0,
            waveform: Waveform::Square,
            phase_offset: 0.5,
        };

        let a = store
            .phasor_tick(CLOCK, &key("a"), &ramp, (READER_NODE, &reader()))
            .unwrap();
        let b = store
            .phasor_tick(CLOCK, &key("b"), &shaped, (READER_NODE, &reader()))
            .unwrap();

        assert_eq!(a, b, "the store's contract is the raw ramp");
    }

    // --- P8: scrub-exact reconstruction from the breakpoint log ------------

    /// Run `frames` frames at `dt`, applying `edits` (frame → new period) on
    /// the way, and record what the phasor read at each effective time.
    ///
    /// The recorded pairs are the ground truth every scrub test compares
    /// against: they are literally what a shader uniform was filled with.
    #[cfg(feature = "scrub-log")]
    fn run_forward(
        store: &mut TimebaseStore,
        frames: usize,
        dt: f32,
        edits: &[(usize, f32)],
    ) -> (alloc::vec::Vec<(f32, (f32, u32))>, f32) {
        let mut period = 1.0_f32;
        let mut samples = alloc::vec::Vec::new();
        for frame in 0..frames {
            if let Some((_, next)) = edits.iter().find(|(at, _)| *at == frame) {
                period = *next;
            }
            if frame > 0 {
                tick(store, dt);
            }
            let t = store.seconds(CLOCK).expect("timebase");
            let value = store
                .phasor_tick(
                    CLOCK,
                    &key("a"),
                    &PhasorConfig::with_period(period),
                    (READER_NODE, &reader()),
                )
                .expect("phasor");
            samples.push((t, value));
        }
        (samples, period)
    }

    /// The headline obligation: every instant the phasor was ever shown at
    /// comes back **bit for bit** when the clock is scrubbed to it.
    ///
    /// Exact `f32` equality, not a tolerance — the closed form is evaluated
    /// from the same breakpoint with the same `t`, so anything less than
    /// equality would mean the reconstruction is a different computation
    /// wearing the same numbers.
    #[cfg(feature = "scrub-log")]
    #[test]
    fn every_sample_of_a_scattered_run_reconstructs_bit_exactly() {
        let mut store = store_with_delta(0.1);
        let edits = [(4, 2.0), (9, 0.75), (13, 5.0), (20, 1.25), (26, 0.5)];
        let (samples, period) = run_forward(&mut store, 32, 0.1, &edits);
        let live = PhasorConfig::with_period(period);

        // Scrub back through history in the order a dragged slider would.
        for (t, recorded) in samples.iter().rev() {
            scrub_to(&mut store, *t);
            let replayed = store
                .phasor_tick(CLOCK, &key("a"), &live, (READER_NODE, &reader()))
                .expect("phasor");
            assert_eq!(
                replayed, *recorded,
                "reconstruction at t={t} drifted from what was shown"
            );
        }
    }

    /// Scrubbing is a read. The integrator is untouched by it, so releasing
    /// the slider picks up exactly where the pre-scrub frame left off.
    #[cfg(feature = "scrub-log")]
    #[test]
    fn returning_to_the_live_edge_continues_from_the_pre_scrub_state() {
        let mut store = store_with_delta(0.1);
        let (samples, period) = run_forward(&mut store, 20, 0.1, &[(6, 2.0), (12, 0.5)]);
        let live = PhasorConfig::with_period(period);
        let (edge, at_edge) = *samples.last().expect("samples");

        for back in [edge - 0.4, edge - 1.1, edge - 0.2] {
            scrub_to(&mut store, back);
            store.phasor_tick(CLOCK, &key("a"), &live, (READER_NODE, &reader()));
        }

        // Back to the live edge: the same effective time reads the same value
        // it did before the scrub…
        scrub_to(&mut store, edge);
        assert_eq!(
            store.phasor_tick(CLOCK, &key("a"), &live, (READER_NODE, &reader())),
            Some(at_edge),
            "the live edge itself must be reproduced"
        );
        // …and the next frame carries on from there.
        tick(&mut store, 0.1);
        let next = store
            .phasor_tick(CLOCK, &key("a"), &live, (READER_NODE, &reader()))
            .expect("phasor");
        assert_eq!(
            next,
            eval_segment(
                &Breakpoint {
                    t_eff: edge,
                    phase: at_edge.0,
                    cycle: at_edge.1,
                    rate: live.rate_hz(),
                },
                edge + 0.1
            )
        );
    }

    /// The log is event-sparse *by construction*: a frame is not an event.
    ///
    /// This is the pin that keeps the feature honest — a per-frame append
    /// would make the log a recording, with a recording's memory profile,
    /// and would be a design violation rather than a slow implementation.
    #[cfg(feature = "scrub-log")]
    #[test]
    fn the_log_grows_only_where_the_rate_changed() {
        let mut store = store_with_delta(0.1);
        let edits = [(5, 2.0), (11, 0.25), (19, 4.0)];
        run_forward(&mut store, 40, 0.1, &edits);

        let log = store.entry(CLOCK).expect("timebase").breakpoints(&key("a"));
        assert_eq!(
            log.len(),
            1 + edits.len(),
            "40 frames, 3 edits: the opening segment plus one breakpoint per \
             edit — nothing per frame ({log:?})"
        );
        assert!(
            log.windows(2).all(|pair| pair[0].t_eff <= pair[1].t_eff),
            "breakpoints must stay ordered in effective time: {log:?}"
        );
    }

    /// Punch-in (parent D6): editing the period while scrubbed back rewrites
    /// the timeline from there. The overwritten future was provisional.
    #[cfg(feature = "scrub-log")]
    #[test]
    fn a_config_write_while_scrubbed_truncates_the_provisional_future() {
        let mut store = store_with_delta(0.1);
        let (samples, _) = run_forward(&mut store, 24, 0.1, &[(8, 2.0), (16, 0.5)]);
        let (punch_t, at_punch) = samples[10];
        let edge = samples.last().expect("samples").0;

        scrub_to(&mut store, punch_t);
        let punched = store
            .phasor_tick(
                CLOCK,
                &key("a"),
                &PhasorConfig::with_period(3.0),
                (READER_NODE, &reader()),
            )
            .expect("phasor");

        // The write lands *at* the scrub position: it changes the slope from
        // here, it does not displace the phase.
        assert_eq!(punched, at_punch);
        let log = store.entry(CLOCK).expect("timebase").breakpoints(&key("a"));
        assert!(
            log.iter().all(|bp| bp.t_eff <= punch_t),
            "breakpoints past the punch-in survived: {log:?}"
        );
        assert_eq!(log.last().expect("log").rate, 1.0 / 3.0);
        assert_eq!(
            store.entry(CLOCK).expect("timebase").live_edge(),
            Some(punch_t),
            "the live edge resets to the punch-in position"
        );
        assert!(
            punch_t < edge,
            "the test only means something scrubbed back"
        );
    }

    /// And the rewritten timeline is itself scrub-exact: the history the
    /// punch-in created replays like any other.
    #[cfg(feature = "scrub-log")]
    #[test]
    fn the_punched_in_history_reproduces_exactly_on_a_second_scrub() {
        let mut store = store_with_delta(0.1);
        let (samples, _) = run_forward(&mut store, 20, 0.1, &[(7, 2.0)]);
        let punch_t = samples[9].0;
        let punched_config = PhasorConfig::with_period(3.0);

        scrub_to(&mut store, punch_t);
        store.phasor_tick(CLOCK, &key("a"), &punched_config, (READER_NODE, &reader()));

        // Run the new future forward, recording it.
        let mut replayed_samples = alloc::vec::Vec::new();
        for _ in 0..12 {
            tick(&mut store, 0.1);
            let t = store.seconds(CLOCK).expect("timebase");
            let value = store
                .phasor_tick(CLOCK, &key("a"), &punched_config, (READER_NODE, &reader()))
                .expect("phasor");
            replayed_samples.push((t, value));
        }

        for (t, recorded) in replayed_samples.iter().rev() {
            scrub_to(&mut store, *t);
            assert_eq!(
                store.phasor_tick(CLOCK, &key("a"), &punched_config, (READER_NODE, &reader())),
                Some(*recorded),
                "the punched-in timeline drifted at t={t}"
            );
        }
    }

    /// The window is a working set, not a history: breakpoints more than
    /// [`SCRUB_WINDOW_SECONDS`] behind get dropped on the next append, and
    /// everything inside it still reconstructs.
    #[cfg(feature = "scrub-log")]
    #[test]
    fn breakpoints_older_than_the_window_are_dropped() {
        let mut store = store_with_delta(1.0);
        // One edit per second for 70 s — the first half falls out of a 30 s
        // window by the end.
        let edits: alloc::vec::Vec<(usize, f32)> = (1..70)
            .map(|frame| (frame, 1.0 + (frame % 5) as f32))
            .collect();
        let (samples, period) = run_forward(&mut store, 70, 1.0, &edits);
        let live = PhasorConfig::with_period(period);
        let edge = samples.last().expect("samples").0;

        let log = store.entry(CLOCK).expect("timebase").breakpoints(&key("a"));
        assert!(
            log.len() < 40,
            "a 30 s window over a 70 s run should have dropped most of it: {}",
            log.len()
        );
        assert!(
            log[0].t_eff <= edge - SCRUB_WINDOW_SECONDS,
            "the segment anchoring the window's own start must be kept: {:?}",
            log[0]
        );

        // Inside the window, reconstruction is untouched by the trimming.
        for (t, recorded) in samples.iter().rev() {
            if *t < edge - SCRUB_WINDOW_SECONDS {
                break;
            }
            scrub_to(&mut store, *t);
            assert_eq!(
                store.phasor_tick(CLOCK, &key("a"), &live, (READER_NODE, &reader())),
                Some(*recorded),
                "in-window reconstruction at t={t}"
            );
        }
    }

    /// The cap is a safety net under a pathological rate: never more than
    /// [`SCRUB_LOG_CAP`] entries, however hard the period is dragged.
    #[cfg(feature = "scrub-log")]
    #[test]
    fn the_cap_bounds_a_pathological_log() {
        let mut store = store_with_delta(0.001);
        // 400 edits inside a hundredth of the window: nothing trims by time.
        let edits: alloc::vec::Vec<(usize, f32)> =
            (0..400).map(|frame| (frame, 1.0 + frame as f32)).collect();
        run_forward(&mut store, 400, 0.001, &edits);

        let log = store.entry(CLOCK).expect("timebase").breakpoints(&key("a"));
        assert!(log.len() <= SCRUB_LOG_CAP, "log grew to {}", log.len());
        assert!(
            log.len() >= SCRUB_LOG_CAP - 1,
            "…and the cap is what bounded it, not the window: {}",
            log.len()
        );
    }

    /// One integrator, one log: two consumers of a `Shared` key read the same
    /// reconstruction, because there is only one thing to reconstruct.
    #[cfg(feature = "scrub-log")]
    #[test]
    fn one_shared_key_is_one_log_for_every_consumer() {
        let mut store = store_with_delta(0.1);
        let shared = PhasorKey::Shared {
            scope: ScopeRef::Module { owner: NodeId(1) },
            channel: ChannelName("phase".into()),
        };
        let slow = PhasorConfig::with_period(2.0);
        let fast = PhasorConfig::with_period(0.5);

        let mut samples = alloc::vec::Vec::new();
        for frame in 0..16 {
            if frame > 0 {
                tick(&mut store, 0.1);
            }
            let config = if frame < 8 { &slow } else { &fast };
            // Two consumers, same key, same tick: the second sees the first's
            // advance, not one of its own.
            let first = store
                .phasor_tick(CLOCK, &shared, config, (READER_NODE, &reader()))
                .expect("phasor");
            let second = store
                .phasor_tick(CLOCK, &shared, config, (READER_NODE, &reader()))
                .expect("phasor");
            assert_eq!(first, second);
            samples.push((store.seconds(CLOCK).expect("timebase"), first));
        }

        assert_eq!(
            store.entry(CLOCK).expect("timebase").phasor_count(),
            1,
            "one integrator for the channel"
        );
        for (t, recorded) in samples.iter().rev() {
            scrub_to(&mut store, *t);
            let a = store
                .phasor_tick(CLOCK, &shared, &fast, (READER_NODE, &reader()))
                .expect("phasor");
            let b = store
                .phasor_tick(CLOCK, &shared, &fast, (READER_NODE, &reader()))
                .expect("phasor");
            assert_eq!((a, b), (*recorded, *recorded), "shared scrub at t={t}");
        }
    }

    /// Scrubbing past the oldest breakpoint is out of history, not out of
    /// bounds: the phasor keeps answering inside `[0,1)` instead of freezing
    /// or panicking on an empty log.
    #[cfg(feature = "scrub-log")]
    #[test]
    fn a_scrub_older_than_the_whole_log_still_answers_in_range() {
        let mut store = store_with_delta(0.1);
        let (_, period) = run_forward(&mut store, 10, 0.1, &[(5, 2.0)]);
        let live = PhasorConfig::with_period(period);

        scrub_to(&mut store, -500.0);
        let (phase, cycle) = store
            .phasor_tick(CLOCK, &key("a"), &live, (READER_NODE, &reader()))
            .expect("phasor");

        assert!((0.0..1.0).contains(&phase), "phase: {phase}");
        assert_eq!(cycle, 0, "the cycle counter saturates at the start");
    }

    /// A phasor that materializes while the clock is scrubbed back does not
    /// corrupt the live edge — it opens its own segment and reads from it.
    #[cfg(feature = "scrub-log")]
    #[test]
    fn a_phasor_born_while_scrubbed_does_not_move_the_live_edge() {
        let mut store = store_with_delta(0.1);
        let (samples, period) = run_forward(&mut store, 12, 0.1, &[(6, 2.0)]);
        let live = PhasorConfig::with_period(period);
        let edge = samples.last().expect("samples").0;

        scrub_to(&mut store, edge - 0.5);
        let born = store
            .phasor_tick(CLOCK, &key("b"), &live, (READER_NODE, &reader()))
            .expect("phasor");

        assert!((0.0..1.0).contains(&born.0), "phase: {born:?}");
        assert_eq!(
            store.entry(CLOCK).expect("timebase").live_edge(),
            Some(edge),
            "a birth behind the edge must not drag the edge back"
        );
    }

    #[test]
    fn set_timebase_overwrites_rather_than_duplicating() {
        let mut store = store_with_delta(0.25);
        store.phasor_tick(
            CLOCK,
            &key("a"),
            &PhasorConfig::default(),
            (READER_NODE, &reader()),
        );

        store.set_timebase(CLOCK, 7.5, -0.25, Revision::new(9));

        assert_eq!(store.len(), 1);
        assert_eq!(store.seconds(CLOCK), Some(7.5));
        assert_eq!(store.delta(CLOCK), Some(-0.25));
        assert_eq!(store.entry(CLOCK).unwrap().updated_at, Revision::new(9));
        assert_eq!(
            store.entry(CLOCK).unwrap().phasor_count(),
            1,
            "a timebase update must not disturb its phasors"
        );
    }
}
