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
//!   resetting the phase.
//!
//! Firmware-safe by construction: `VecMap`, `no_std` + alloc, and wrapping
//! done with plain arithmetic (no `std::f32`, no `libm`).

use lp_collection::VecMap;
use lpc_model::{ChannelName, NodeId, PhasorConfig, Revision, SlotPath};

use crate::node::ScopeRef;

/// Store ticks a phasor may go unqueried before it despawns.
///
/// ~2 s at 60 fps. Long enough that a consumer skipped for a frame or two
/// (a playlist entry off-screen, a paused preview) keeps its phase; short
/// enough that a deleted shader's phasors do not accumulate on a device.
pub const PHASOR_IDLE_TICKS: u32 = 120;

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

/// One phasor's integrator. Config-free by design; ~16 bytes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhasorState {
    /// Wrapped cycle position, always in `[0,1)`.
    pub phase: f32,
    /// Completed cycles since materialization (saturating both ways).
    pub cycle: u32,
    /// Store tick this phasor last advanced in — the advance-once-per-tick
    /// key.
    advanced_at: u32,
    /// Store tick this phasor was last queried in — the despawn clock.
    last_queried_at: u32,
}

impl PhasorState {
    fn materialized(tick: u32) -> Self {
        Self {
            phase: 0.0,
            cycle: 0,
            // Deliberately not `tick`: a phasor that materializes mid-tick
            // still owes this tick's advance to the query that created it.
            // Whether that first query advances is decided by the caller
            // (see `phasor_tick`), not by pretending the tick already ran.
            advanced_at: tick.wrapping_sub(1),
            last_queried_at: tick,
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
}

impl TimebaseEntry {
    /// How many phasors currently ride this timebase.
    #[must_use]
    pub fn phasor_count(&self) -> usize {
        self.phasors.len()
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
                        phasors: VecMap::new(),
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
    /// `None` only when the clock has no timebase yet (nothing has produced
    /// it) — an unmaterialized phasor is not an error, it is a birth.
    pub fn phasor_tick(
        &mut self,
        clock: NodeId,
        key: &PhasorKey,
        config: &PhasorConfig,
    ) -> Option<(f32, u32)> {
        let tick = self.tick;
        let entry = self.entries.get_mut(&clock)?;
        let delta_seconds = entry.delta_seconds;
        let state = match entry.phasors.get_mut(key) {
            Some(state) => state,
            None => {
                entry
                    .phasors
                    .insert(key.clone(), PhasorState::materialized(tick));
                entry
                    .phasors
                    .get_mut(key)
                    .expect("phasor was just materialized")
            }
        };
        state.last_queried_at = tick;
        if state.advanced_at != tick {
            state.advanced_at = tick;
            advance(state, config.rate_hz() * delta_seconds);
        }
        Some((state.phase, state.cycle))
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
/// Handles a negative advance (a device scrubbing backwards) by wrapping
/// downward and stepping the cycle counter back, and refuses to act on a
/// non-finite advance rather than poisoning the integrator with a NaN it can
/// never recover from.
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

    #[test]
    fn phasor_state_stays_small() {
        assert!(
            core::mem::size_of::<PhasorState>() <= 16,
            "PhasorState grew to {} bytes",
            core::mem::size_of::<PhasorState>()
        );
    }

    #[test]
    fn a_phasor_materializes_at_zero_and_advances_by_rate_times_delta() {
        let mut store = store_with_delta(0.5);
        let config = PhasorConfig::with_period(2.0);

        // The materializing query advances too: the tick's delta is owed.
        assert_eq!(
            store.phasor_tick(CLOCK, &key("a"), &config),
            Some((0.25, 0))
        );
        store.sweep(|_| true);
        assert_eq!(store.phasor_tick(CLOCK, &key("a"), &config), Some((0.5, 0)));
    }

    #[test]
    fn a_query_against_an_unpublished_clock_answers_none() {
        let mut store = TimebaseStore::new();

        assert_eq!(
            store.phasor_tick(NodeId(42), &key("a"), &PhasorConfig::default()),
            None
        );
        assert_eq!(store.seconds(NodeId(42)), None);
        assert_eq!(store.delta(NodeId(42)), None);
    }

    #[test]
    fn two_queries_in_one_tick_see_the_same_phase() {
        let mut store = store_with_delta(0.1);
        let config = PhasorConfig::with_period(1.0);

        let first = store.phasor_tick(CLOCK, &key("a"), &config).unwrap();
        let second = store.phasor_tick(CLOCK, &key("a"), &config).unwrap();
        let third = store.phasor_tick(CLOCK, &key("a"), &config).unwrap();

        assert_eq!(first, (0.1, 0));
        assert_eq!(second, first);
        assert_eq!(third, first);
    }

    #[test]
    fn phase_stays_in_unit_range_and_cycles_count_up() {
        let mut store = store_with_delta(0.25);
        let config = PhasorConfig::with_period(1.0);

        for _ in 0..40 {
            let (phase, _) = store.phasor_tick(CLOCK, &key("a"), &config).unwrap();
            assert!((0.0..1.0).contains(&phase), "phase escaped [0,1): {phase}");
            store.sweep(|_| true);
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
            store.phasor_tick(CLOCK, &key("a"), &slow);
            store.sweep(|_| true);
        }
        let before = store.phasor_read(CLOCK, &key("a")).unwrap();

        // Swapping config with no time elapsed must move nothing at all.
        store.set_timebase(CLOCK, 0.0, 0.0, Revision::new(2));
        let at_event = store.phasor_tick(CLOCK, &key("a"), &fast).unwrap();
        assert_eq!(at_event, before);

        // …and the next advance simply uses the new rate from there.
        store.sweep(|_| true);
        store.set_timebase(CLOCK, 0.0, 0.1, Revision::new(3));
        let after = store.phasor_tick(CLOCK, &key("a"), &fast).unwrap();
        assert!(
            (after.0 - (before.0 + 0.1)).abs() <= f32::EPSILON,
            "expected continuity at the config change: {before:?} -> {after:?}"
        );
    }

    #[test]
    fn a_zero_period_freezes_the_phase_and_resumes_continuously() {
        let mut store = store_with_delta(0.1);
        let running = PhasorConfig::with_period(1.0);
        let frozen = PhasorConfig::with_period(0.0);

        store.phasor_tick(CLOCK, &key("a"), &running);
        store.sweep(|_| true);
        let held = store.phasor_read(CLOCK, &key("a")).unwrap();

        for _ in 0..5 {
            assert_eq!(store.phasor_tick(CLOCK, &key("a"), &frozen), Some(held));
            store.sweep(|_| true);
        }

        let resumed = store.phasor_tick(CLOCK, &key("a"), &running).unwrap();
        assert!((resumed.0 - (held.0 + 0.1)).abs() <= f32::EPSILON);
    }

    #[test]
    fn a_zero_delta_freezes_every_phasor_on_the_clock() {
        let mut store = store_with_delta(0.1);
        let config = PhasorConfig::with_period(1.0);
        store.phasor_tick(CLOCK, &key("a"), &config);
        store.phasor_tick(CLOCK, &key("b"), &config);
        store.sweep(|_| true);

        store.set_timebase(CLOCK, 0.0, 0.0, Revision::new(2));
        for _ in 0..4 {
            assert_eq!(store.phasor_tick(CLOCK, &key("a"), &config), Some((0.1, 0)));
            assert_eq!(store.phasor_tick(CLOCK, &key("b"), &config), Some((0.1, 0)));
            store.sweep(|_| true);
        }
    }

    #[test]
    fn a_negative_delta_wraps_downward() {
        let mut store = store_with_delta(0.25);
        let config = PhasorConfig::with_period(1.0);

        for _ in 0..5 {
            store.phasor_tick(CLOCK, &key("a"), &config);
            store.sweep(|_| true);
        }
        assert_eq!(store.phasor_read(CLOCK, &key("a")), Some((0.25, 1)));

        store.set_timebase(CLOCK, 0.0, -0.5, Revision::new(2));
        let (phase, cycle) = store.phasor_tick(CLOCK, &key("a"), &config).unwrap();

        assert!((phase - 0.75).abs() < 1e-6, "phase: {phase}");
        assert_eq!(cycle, 0);
    }

    #[test]
    fn scrubbing_below_the_first_cycle_clamps_the_counter_not_the_phase() {
        let mut store = store_with_delta(-2.5);
        let config = PhasorConfig::with_period(1.0);

        let (phase, cycle) = store.phasor_tick(CLOCK, &key("a"), &config).unwrap();

        assert!((0.0..1.0).contains(&phase), "phase: {phase}");
        assert!((phase - 0.5).abs() < 1e-6, "phase: {phase}");
        assert_eq!(cycle, 0);
    }

    #[test]
    fn a_wild_advance_does_not_poison_the_integrator() {
        let mut store = store_with_delta(f32::INFINITY);
        let config = PhasorConfig::with_period(1.0);

        let (phase, _) = store.phasor_tick(CLOCK, &key("a"), &config).unwrap();
        assert!(phase.is_finite() && (0.0..1.0).contains(&phase));

        store.sweep(|_| true);
        store.set_timebase(CLOCK, 0.0, f32::NAN, Revision::new(2));
        let (phase, _) = store.phasor_tick(CLOCK, &key("a"), &config).unwrap();
        assert!(phase.is_finite() && (0.0..1.0).contains(&phase));
    }

    #[test]
    fn a_render_read_never_materializes_or_advances() {
        let mut store = store_with_delta(0.5);

        assert_eq!(store.phasor_read(CLOCK, &key("a")), Some((0.0, 0)));
        assert_eq!(store.entry(CLOCK).unwrap().phasor_count(), 0);

        store.phasor_tick(CLOCK, &key("a"), &PhasorConfig::with_period(1.0));
        let read = store.phasor_read(CLOCK, &key("a")).unwrap();
        assert_eq!(store.phasor_read(CLOCK, &key("a")).unwrap(), read);
        assert_eq!(read, (0.5, 0));
    }

    #[test]
    fn an_unqueried_phasor_despawns_and_re_materializes_at_zero() {
        let mut store = store_with_delta(0.25);
        let config = PhasorConfig::with_period(1.0);

        store.phasor_tick(CLOCK, &key("a"), &config);
        assert_eq!(store.entry(CLOCK).unwrap().phasor_count(), 1);

        for _ in 0..PHASOR_IDLE_TICKS {
            store.sweep(|_| true);
            assert_eq!(
                store.entry(CLOCK).unwrap().phasor_count(),
                1,
                "despawned early at tick {}",
                store.tick()
            );
        }
        store.sweep(|_| true);
        assert_eq!(store.entry(CLOCK).unwrap().phasor_count(), 0);

        // The timebase itself survives; only the phasor went.
        assert_eq!(store.delta(CLOCK), Some(0.25));
        assert_eq!(
            store.phasor_tick(CLOCK, &key("a"), &config),
            Some((0.25, 0))
        );
    }

    #[test]
    fn a_phasor_queried_every_tick_never_despawns() {
        let mut store = store_with_delta(0.0);
        let config = PhasorConfig::with_period(1.0);

        for _ in 0..(PHASOR_IDLE_TICKS * 3) {
            store.phasor_tick(CLOCK, &key("a"), &config);
            store.sweep(|_| true);
        }

        assert_eq!(store.entry(CLOCK).unwrap().phasor_count(), 1);
    }

    #[test]
    fn a_dead_clock_loses_its_whole_timebase() {
        let mut store = store_with_delta(0.25);
        store.set_timebase(NodeId(2), 1.0, 0.25, Revision::new(1));
        store.phasor_tick(CLOCK, &key("a"), &PhasorConfig::default());

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

        let outer_phase = store.phasor_tick(outer, &key("a"), &config).unwrap();
        let inner_phase = store.phasor_tick(inner, &key("a"), &config).unwrap();

        assert_eq!(store.seconds(outer), Some(10.0));
        assert_eq!(store.seconds(inner), Some(3.0));
        assert_eq!(outer_phase, (0.5, 0));
        assert_eq!(inner_phase, (0.1, 0));
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

        store.phasor_tick(CLOCK, &private, &config);
        store.sweep(|_| true);
        store.phasor_tick(CLOCK, &private, &config);
        store.phasor_tick(CLOCK, &shared, &config);

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

        store.phasor_tick(CLOCK, &outer, &config);
        store.sweep(|_| true);
        store.phasor_tick(CLOCK, &outer, &config);
        store.phasor_tick(CLOCK, &inner, &config);

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

        let a = store.phasor_tick(CLOCK, &key("a"), &ramp).unwrap();
        let b = store.phasor_tick(CLOCK, &key("b"), &shaped).unwrap();

        assert_eq!(a, b, "the store's contract is the raw ramp");
    }

    #[test]
    fn set_timebase_overwrites_rather_than_duplicating() {
        let mut store = store_with_delta(0.25);
        store.phasor_tick(CLOCK, &key("a"), &PhasorConfig::default());

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
