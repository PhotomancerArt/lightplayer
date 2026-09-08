//! The signal fabric: where a peripheral's output actually goes.
//!
//! # Why this is on the bus and not inside GPIO
//!
//! A peripheral never sees another peripheral ([`crate::periph`]), so the RMT
//! block cannot read `GPIO.func_out_sel_cfg[18]` to find out which pad its
//! waveform reaches, and the GPIO block cannot ask the RMT what level its
//! signal is at. The routing is one fact that two blocks share, which is
//! exactly the shape the interrupt matrix already has (plan DD22): **one
//! state on the bus, register views writing into it**. [`Fabric`] is that
//! state for pads and signals (plan DD34 e), reached through
//! [`crate::periph::BusCx::pins`].
//!
//! - the chip's GPIO block is a routing **view**: a write to
//!   `func_out_sel_cfg[n]` becomes [`Fabric::route`], a write to `out` /
//!   `out_w1ts` / `out_w1tc` becomes [`Fabric::set_gpio_out`];
//! - an output peripheral **drives its signal** ([`Fabric::drive`]) and never
//!   learns whether anyone is listening;
//! - the machine drains [`Fabric::take_edges`] every slice and hands the
//!   edges to whatever is watching the wire — a strip decoder, a raw pin log.
//!
//! # No chip numbers
//!
//! Like the rest of this crate, the fabric holds none: it does not know that
//! the C6 calls signal 71 `RMT_SIG_0` or that 128 means "follow the GPIO
//! output register". The chip crate decides which [`SignalId`] a write names
//! and whether that write means [`RouteSource::GpioOut`]; the fabric only
//! remembers and propagates. [`MAX_PADS`] is a capacity, not a pin count.
//!
//! # The input side (M2 P1, plan RD5)
//!
//! A pad now has two sides. Something *outside* the chip can hold a level on
//! it ([`Fabric::drive_pad`], [`Fabric::release_pad`]) — a button to ground,
//! an encoder's quadrature pair, a jumper from another pad — and two pads can
//! be tied together with [`Fabric::wire`] so that whatever one carries the
//! other carries. [`Fabric::pad_level`] is the **resolved** level of the pad:
//! what a scope on the pin header would read.
//!
//! ## The resolution rule
//!
//! One rule, stated here because every input payload depends on it:
//!
//! 1. Collect the pad and everything [`wire`](Fabric::wire)d to it — the
//!    **tied group**. An untied pad is a group of one.
//! 2. If any pad in the group has an **outside driver**, the group carries
//!    that level. Where two outside drivers disagree, the **lowest-numbered
//!    pad's** driver wins, so a run is replayable from its flags.
//! 3. Otherwise, if any pad in the group is **routed**, the group carries
//!    that pad's output level (again lowest-numbered first).
//! 4. Otherwise the group is not observed at all and reads low.
//!
//! **An outside driver always wins**, and the group's own output never gates
//! it. When the losing side was an *enabled* output — the pad is routed and
//! either the peripheral owns its OE (`oen_sel = 0`) or the GPIO `enable` bit
//! is set — and the two levels disagree, that is a **conflict**: it is logged
//! with both levels and the cycle, counted in [`Fabric::conflicts`], and then
//! ignored. Never gated: an emulator that refused to run because a bench
//! shorted an output tells you less than one that says so and carries on.
//!
//! The output side's `enable`/`oen_sel` bits are *read* here for that one
//! purpose — deciding whether a disagreement is worth a line — and for
//! nothing else. A pad whose OE is low still records its own edges, exactly
//! as before.
//!
//! ## Edges are one stream
//!
//! An edge an outside driver causes goes into the same [`take_edges`](Fabric::take_edges)
//! queue as one the RMT caused, so the pin log and the strip decoders see it
//! with no change. A pad that only ever had an outside driver — never routed —
//! is observed from the first [`drive_pad`](Fabric::drive_pad).
//!
//! # What is modelled, and what is not
//!
//! Modelled: the routing itself, the driven level of a signal, the level of a
//! routed pad, the **edge** — the cycle a pad's resolved level changed —, an
//! outside driver on a pad, a pad-to-pad wire, and the pad's **input enable**
//! as a recorded bit ([`Fabric::set_pad_input_enable`], the chip's IO_MUX
//! `fun_ie`). The input enable is *recorded, never gated on* here: it does not
//! change what [`pad_level`](Fabric::pad_level) answers. The chip's GPIO block
//! reads it back to decide whether to serve the pad's bit in `GPIO.in_`, which
//! is where that bit belongs — this crate holds no chip numbers.
//!
//! Not modelled, and each of these is an electrical fact a logic analyser on
//! the pin header would not show you either: **drive strength**, the **value**
//! of a pull-up or pull-down (an undriven pad's level is this model's, not a
//! resistor's — a pad nothing drives reads low, not "pulled high"),
//! **open-drain** (`pad_driver`), **pad filters** and input glitch filters,
//! and **analog** anything. Nor is the *timing* of an outside edge relative to
//! whatever samples it beyond the cycle the caller stated: there is no
//! synchroniser, no metastability and no propagation delay — an edge is at the
//! cycle it is stamped.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use lp_emu_core::sched::Cycles;

/// How many pads a fabric can carry.
///
/// A capacity, not a chip fact: the C6 has 31 pads, an S3 has 49, and a pad
/// number at or past this is refused with a log line rather than a panic —
/// an emulator that aborts because a guest wrote a wide bitmap tells you
/// less than one that keeps running.
pub const MAX_PADS: usize = 64;

/// How many edges the fabric buffers before it starts dropping them.
///
/// The machine drains every slice, so the live count is a slice's worth
/// (a 256-LED WS2812 frame is 12,288 edges over 7.7 ms of guest time, and a
/// strict slice is 1,024 cycles). The cap only matters for a `Sandbox` test
/// that never drains.
pub const EDGE_CAP: usize = 1 << 20;

/// A peripheral **output signal** number, as the chip's interconnect names
/// it. This crate never interprets it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SignalId(pub u16);

/// A **pad** number — the chip's GPIO number.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PadId(pub u8);

impl core::fmt::Display for PadId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "gpio{}", self.0)
    }
}

/// What a pad's level follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouteSource {
    /// A peripheral signal, optionally inverted (`inv_sel`).
    Signal(SignalId, bool),
    /// The GPIO output register's bit for this pad.
    GpioOut,
}

/// One pad's routing, as the last write left it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Route {
    pub source: RouteSource,
    /// The cycle the routing was last written.
    pub at: Cycles,
    /// Whether the pad's output enable comes from the GPIO `enable` bitmap
    /// rather than the peripheral (`oen_sel`). **Recorded, never gated on**;
    /// the chip crate reports it in the trace note.
    pub oen_from_gpio: bool,
}

/// A level change on a routed pad, in guest cycles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Edge {
    pub at: Cycles,
    pub pad: PadId,
    pub level: bool,
}

/// The routing and the levels: one state, on the bus.
///
/// See the module docs. The fabric is deliberately dumb — it holds no
/// timing, schedules nothing, and every method is a few array writes, because
/// [`drive`](Self::drive) is called twice per WS2812 bit and a 256-LED frame
/// is 12,288 of them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fabric {
    /// Per pad, the routing — `None` until something routes it.
    routes: Vec<Option<Route>>,
    /// Per pad, the level the fabric last computed. A pad with no route is
    /// held low and records nothing.
    level: Vec<bool>,
    /// The GPIO output register, one bit per pad.
    gpio_out: Vec<bool>,
    /// The GPIO output-**enable** register, one bit per pad. Recorded so the
    /// chip's route note can report it; never gated on.
    gpio_enable: Vec<bool>,
    /// Signal levels, sparse: `(signal, level)` sorted by signal. A chip has
    /// a few hundred signals and a run drives one or two.
    signals: Vec<(SignalId, bool)>,
    /// Per pad, the level something **outside** the chip holds on it, or
    /// `None` when nothing does. See the module docs' resolution rule.
    driven: Vec<Option<bool>>,
    /// Per pad, the pad's input enable (the chip's IO_MUX `fun_ie`).
    /// Recorded; never gated on here.
    input_enable: Vec<bool>,
    /// Union-find over tied pads: `tie[i]` is `i`'s parent, or `i` itself.
    /// Only meaningful while [`Fabric::tied`] is set.
    tie: Vec<u8>,
    /// Whether any [`Fabric::wire`] has ever been made. The whole tied-group
    /// walk is skipped while this is false, which is the case on every run
    /// that is not a loopback: `settle` is called twice per WS2812 bit.
    tied: bool,
    /// How many pads currently carry an outside driver. Zero on every run
    /// with no input, which keeps the resolution one `Option` check.
    drivers: usize,
    /// Conflicts logged since the fabric was built (module docs).
    conflicts: u64,
    edges: Vec<Edge>,
    /// Edges dropped because [`EDGE_CAP`] was reached.
    dropped: u64,
    /// Bumped whenever a pad's routing changed. A machine that watches the
    /// routing compares this instead of walking every pad every slice.
    epoch: u64,
}

/// How many conflicts are logged in full before the fabric goes quiet.
///
/// A shorted output on a driven strip pad would otherwise write one warning
/// per WS2812 bit. The count in [`Fabric::conflicts`] keeps counting.
const CONFLICTS_LOGGED: u64 = 8;

impl Default for Fabric {
    fn default() -> Self {
        Self::new()
    }
}

impl Fabric {
    pub fn new() -> Self {
        Self {
            routes: alloc::vec![None; MAX_PADS],
            level: alloc::vec![false; MAX_PADS],
            gpio_out: alloc::vec![false; MAX_PADS],
            gpio_enable: alloc::vec![false; MAX_PADS],
            signals: Vec::new(),
            driven: alloc::vec![None; MAX_PADS],
            input_enable: alloc::vec![false; MAX_PADS],
            tie: (0..MAX_PADS as u8).collect(),
            tied: false,
            drivers: 0,
            conflicts: 0,
            edges: Vec::new(),
            dropped: 0,
            epoch: 0,
        }
    }

    /// Bumped on every routing change. See [`Fabric::routes`].
    pub fn route_epoch(&self) -> u64 {
        self.epoch
    }

    fn index(pad: PadId) -> Option<usize> {
        let i = usize::from(pad.0);
        if i >= MAX_PADS {
            log::warn!("Fabric: pad {i} is past MAX_PADS ({MAX_PADS}), ignored");
            return None;
        }
        Some(i)
    }

    /// Point pad `pad` at `source` from cycle `at`.
    ///
    /// The pad's level is recomputed immediately: routing a pad to a signal
    /// that is already high **is** a rising edge at `at`, which is what
    /// silicon does when `func_out_sel_cfg` connects a running peripheral.
    pub fn route(&mut self, pad: PadId, source: RouteSource, oen_from_gpio: bool, at: Cycles) {
        let Some(i) = Self::index(pad) else { return };
        self.routes[i] = Some(Route {
            source,
            at,
            oen_from_gpio,
        });
        self.epoch += 1;
        self.settle(i, at);
    }

    /// Drop pad `pad`'s routing. It stops recording edges and reads low.
    pub fn unroute(&mut self, pad: PadId, at: Cycles) {
        let Some(i) = Self::index(pad) else { return };
        if self.routes[i].is_none() {
            return;
        }
        self.routes[i] = None;
        self.epoch += 1;
        // A pad an outside driver still holds keeps that level; one nothing
        // touches any more falls low and records the edge.
        self.settle_group(i, at);
    }

    /// Set the GPIO output register's bit for `pad`.
    pub fn set_gpio_out(&mut self, pad: PadId, level: bool, at: Cycles) {
        let Some(i) = Self::index(pad) else { return };
        if self.gpio_out[i] == level {
            return;
        }
        self.gpio_out[i] = level;
        self.settle(i, at);
    }

    /// Set the GPIO output-enable bit for `pad`. Recorded only; see
    /// [`Route::oen_from_gpio`].
    pub fn set_gpio_enable(&mut self, pad: PadId, enabled: bool) {
        let Some(i) = Self::index(pad) else { return };
        self.gpio_enable[i] = enabled;
    }

    /// A peripheral's output signal changed level at cycle `at`.
    ///
    /// Every pad routed to it settles; a signal nothing is routed to records
    /// nothing, which is the whole point — an unrouted RMT channel is
    /// invisible, exactly as it is on the pin header.
    pub fn drive(&mut self, signal: SignalId, level: bool, at: Cycles) {
        match self.signals.binary_search_by_key(&signal, |(s, _)| *s) {
            Ok(k) => {
                if self.signals[k].1 == level {
                    return;
                }
                self.signals[k].1 = level;
            }
            Err(k) => self.signals.insert(k, (signal, level)),
        }
        for i in 0..MAX_PADS {
            if matches!(
                self.routes[i].map(|r| r.source),
                Some(RouteSource::Signal(s, _)) if s == signal
            ) {
                self.settle(i, at);
            }
        }
    }

    /// The level a signal is being driven at (`false` if nothing drives it).
    pub fn signal_level(&self, signal: SignalId) -> bool {
        self.signals
            .binary_search_by_key(&signal, |(s, _)| *s)
            .map(|k| self.signals[k].1)
            .unwrap_or(false)
    }

    /// The level of `pad` now. A pad with no route reads low.
    pub fn pad_level(&self, pad: PadId) -> bool {
        Self::index(pad).map(|i| self.level[i]).unwrap_or(false)
    }

    /// `pad`'s routing, if it has one.
    pub fn route_of(&self, pad: PadId) -> Option<Route> {
        Self::index(pad).and_then(|i| self.routes[i])
    }

    /// The GPIO output-enable bit for `pad`, as last written.
    pub fn gpio_enable(&self, pad: PadId) -> bool {
        Self::index(pad)
            .map(|i| self.gpio_enable[i])
            .unwrap_or(false)
    }

    /// Every routed pad, ascending.
    pub fn routes(&self) -> impl Iterator<Item = (PadId, Route)> + '_ {
        self.routes
            .iter()
            .enumerate()
            .filter_map(|(i, r)| r.map(|r| (PadId(i as u8), r)))
    }

    // ---- the input side (M2 P1) ------------------------------------------

    /// Something **outside** the chip holds `level` on `pad` from cycle `at`.
    ///
    /// A bench driver: a button pulling a pad to ground, an encoder's channel,
    /// the far end of a jumper. It wins the pad's resolved level over the
    /// chip's own output — see the module docs' resolution rule — and it does
    /// so whether or not anything has routed the pad, so a pad driven before
    /// the guest has configured it is observed from here.
    pub fn drive_pad(&mut self, pad: PadId, level: bool, at: Cycles) {
        let Some(i) = Self::index(pad) else { return };
        if self.driven[i] == Some(level) {
            return;
        }
        if self.driven[i].is_none() {
            self.drivers += 1;
        }
        self.driven[i] = Some(level);
        self.settle_group(i, at);
    }

    /// The outside driver on `pad` lets go: the pad falls back to whatever
    /// the chip's own side is doing (and to low if that is nothing).
    pub fn release_pad(&mut self, pad: PadId, at: Cycles) {
        let Some(i) = Self::index(pad) else { return };
        if self.driven[i].is_none() {
            return;
        }
        self.driven[i] = None;
        self.drivers -= 1;
        self.settle_group(i, at);
    }

    /// The level an outside driver holds on `pad`, if one does.
    pub fn driven_level(&self, pad: PadId) -> Option<bool> {
        Self::index(pad).and_then(|i| self.driven[i])
    }

    /// Record `pad`'s **input enable** — the chip's IO_MUX `fun_ie`.
    ///
    /// Recorded, never gated on here: the pad's resolved level is the same
    /// either way, and the edges are the same edges. The chip's GPIO view
    /// reads it back with [`pad_input_enable`](Self::pad_input_enable) to
    /// decide whether to serve the pad's bit in its input register, which is
    /// where a chip fact belongs.
    pub fn set_pad_input_enable(&mut self, pad: PadId, enabled: bool) {
        let Some(i) = Self::index(pad) else { return };
        self.input_enable[i] = enabled;
    }

    /// `pad`'s input enable, as last recorded.
    pub fn pad_input_enable(&self, pad: PadId) -> bool {
        Self::index(pad)
            .map(|i| self.input_enable[i])
            .unwrap_or(false)
    }

    /// Tie `a` and `b`: whatever one carries, the other carries.
    ///
    /// A jumper between two pin-header pins, and the only way one emulated
    /// chip's output reaches its own input. Ties are transitive — wiring
    /// `a:b` and `b:c` makes one group of three — and the resolution rule in
    /// the module docs applies to the group as a whole.
    ///
    /// Both pads settle immediately at `at`, so wiring a pad to one that is
    /// already high **is** a rising edge, the same way [`route`](Self::route)
    /// is.
    pub fn wire(&mut self, a: PadId, b: PadId, at: Cycles) -> Result<(), String> {
        if a == b {
            return Err(format!(
                "wire {a}:{b}: a pad cannot be wired to itself (a wire ties two different pads)"
            ));
        }
        let (Some(ia), Some(ib)) = (Self::index(a), Self::index(b)) else {
            return Err(format!(
                "wire {a}:{b}: a pad number must be under {MAX_PADS}"
            ));
        };
        self.tied = true;
        let (ra, rb) = (self.find(ia), self.find(ib));
        if ra != rb {
            // Lowest root wins, so the group's identity does not depend on
            // the order the wires were declared in.
            let (keep, drop) = if ra < rb { (ra, rb) } else { (rb, ra) };
            self.tie[drop] = keep as u8;
        }
        self.settle_group(ia, at);
        Ok(())
    }

    /// Untying is not a thing a run can do, and saying so beats a silent
    /// no-op.
    ///
    /// A wire is a **bench** fact — a jumper on the header, declared once by
    /// `--wire a:b` before the machine boots — so a run is replayable from
    /// its flags alone. A guest cannot see a wire and so cannot ask for this;
    /// a host tool that reaches for it is asking for a run nobody could
    /// reproduce, and gets a message instead.
    pub fn unwire(&mut self, a: PadId, b: PadId) -> Result<(), String> {
        Err(format!(
            "unwire {a}:{b}: a wire is declared before the run and holds for all of it; \
             there is no unwire (start a run without the `--wire`)"
        ))
    }

    /// Every pad tied to `pad`, itself included, ascending. A pad with no
    /// wire answers just itself.
    pub fn wired_group(&self, pad: PadId) -> Vec<PadId> {
        let Some(i) = Self::index(pad) else {
            return Vec::new();
        };
        if !self.tied {
            return alloc::vec![pad];
        }
        let root = self.find(i);
        (0..MAX_PADS)
            .filter(|j| self.find(*j) == root)
            .map(|j| PadId(j as u8))
            .collect()
    }

    /// Conflicts — an outside driver disagreeing with an enabled output —
    /// seen since the fabric was built. See the module docs.
    pub fn conflicts(&self) -> u64 {
        self.conflicts
    }

    /// Take the edges recorded since the last call. The machine drains this
    /// every slice, in order.
    pub fn take_edges(&mut self) -> Vec<Edge> {
        core::mem::take(&mut self.edges)
    }

    /// Edges lost to [`EDGE_CAP`] since the fabric was built.
    pub fn dropped_edges(&self) -> u64 {
        self.dropped
    }

    /// The union-find root of `i`, without path compression (the sets are at
    /// most [`MAX_PADS`] wide and a wire chain is a handful of pads).
    fn find(&self, mut i: usize) -> usize {
        while self.tie[i] as usize != i {
            i = self.tie[i] as usize;
        }
        i
    }

    /// The level pad `i`'s **own** side is driving, or `None` when nothing
    /// has routed it.
    fn output_of(&self, i: usize) -> Option<bool> {
        let route = self.routes[i]?;
        Some(match route.source {
            RouteSource::Signal(s, invert) => self.signal_level(s) != invert,
            RouteSource::GpioOut => self.gpio_out[i],
        })
    }

    /// Whether pad `i`'s output is *enabled*, as the recorded bits say: the
    /// peripheral owns the OE (`oen_sel = 0`), or the GPIO `enable` bit is
    /// set. Read for one purpose only — deciding whether a disagreement with
    /// an outside driver is worth a line. Never gated on.
    fn output_enabled(&self, i: usize) -> bool {
        match self.routes[i] {
            Some(route) => !route.oen_from_gpio || self.gpio_enable[i],
            None => false,
        }
    }

    /// Recompute pad `i`'s level and record an edge if it moved.
    ///
    /// The fast path — no wire anywhere, no outside driver on this pad — is
    /// the pre-M2 one: the pad's own route, or nothing at all.
    fn settle(&mut self, i: usize, at: Cycles) {
        if self.tied {
            self.settle_group(i, at);
            return;
        }
        let level = self.resolve(i, at);
        self.apply(i, level, at);
    }

    /// Settle every pad tied to `i` (see [`Fabric::wire`]). Called instead of
    /// [`Fabric::settle`] once anything is wired, and whenever an outside
    /// driver moves.
    fn settle_group(&mut self, i: usize, at: Cycles) {
        if !self.tied {
            let level = self.resolve(i, at);
            self.apply(i, level, at);
            return;
        }
        let root = self.find(i);
        let level = self.resolve(root, at);
        for j in 0..MAX_PADS {
            if self.find(j) == root {
                self.apply(j, level, at);
            }
        }
    }

    /// The resolved level of the tied group `i` belongs to.
    ///
    /// A group nothing drives and nothing routes resolves **low**, which is
    /// also what a pad that was never observed already reads — so
    /// [`Fabric::apply`] records nothing for it, and "an untouched pad stays
    /// unobserved" survives the input side unchanged.
    ///
    /// The module docs state the rule; this is it, and the conflict line is
    /// written from here because this is the only place both sides are in
    /// hand at once.
    fn resolve(&mut self, i: usize, at: Cycles) -> bool {
        // Rule 2 and 3, over the group — or over `{i}` when nothing is tied.
        let (driver, output) = if self.tied {
            let root = self.find(i);
            let mut driver = None;
            let mut output = None;
            for j in 0..MAX_PADS {
                if self.find(j) != root {
                    continue;
                }
                if driver.is_none()
                    && let Some(level) = self.driven[j]
                {
                    driver = Some((j, level));
                }
                if output.is_none()
                    && let Some(level) = self.output_of(j)
                {
                    output = Some((j, level));
                }
            }
            (driver, output)
        } else {
            (
                self.driven[i].map(|level| (i, level)),
                self.output_of(i).map(|level| (i, level)),
            )
        };
        match (driver, output) {
            (Some((_, d)), Some((oj, o))) => {
                if d != o && self.output_enabled(oj) {
                    self.log_conflict(i, oj, d, o, at);
                }
                d
            }
            (Some((_, d)), None) => d,
            (None, Some((_, o))) => o,
            (None, None) => false,
        }
    }

    /// Write pad `i`'s level and record the edge if it moved.
    fn apply(&mut self, i: usize, level: bool, at: Cycles) {
        if self.level[i] == level {
            return;
        }
        self.level[i] = level;
        self.push(Edge {
            at,
            pad: PadId(i as u8),
            level,
        });
    }

    fn log_conflict(
        &mut self,
        i: usize,
        output_pad: usize,
        driver: bool,
        output: bool,
        at: Cycles,
    ) {
        self.conflicts += 1;
        if self.conflicts <= CONFLICTS_LOGGED {
            log::warn!(
                "Fabric: cyc={at} conflict on gpio{i}: an outside driver holds \
                 {driver} while gpio{output_pad}'s enabled output drives {output}; \
                 the driver wins (logged, never gated)",
                driver = u8::from(driver),
                output = u8::from(output),
            );
            if self.conflicts == CONFLICTS_LOGGED {
                log::warn!(
                    "Fabric: {CONFLICTS_LOGGED} conflicts logged; later ones are counted \
                     (`Fabric::conflicts`) and not logged"
                );
            }
        }
    }

    fn push(&mut self, edge: Edge) {
        if self.edges.len() >= EDGE_CAP {
            if self.dropped == 0 {
                log::warn!(
                    "Fabric: edge buffer cap ({EDGE_CAP}) reached with nothing draining it; \
                     later edges are dropped"
                );
            }
            self.dropped += 1;
            return;
        }
        self.edges.push(edge);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIG: SignalId = SignalId(71);
    const PAD: PadId = PadId(18);

    #[test]
    fn an_unrouted_signal_records_nothing() {
        let mut f = Fabric::new();
        f.drive(SIG, true, 10);
        f.drive(SIG, false, 20);
        assert!(f.take_edges().is_empty());
        assert!(!f.pad_level(PAD));
        assert!(f.signal_level(SIG) == false);
    }

    #[test]
    fn routing_a_pad_to_a_high_signal_records_a_rising_edge_at_the_route_cycle() {
        let mut f = Fabric::new();
        f.drive(SIG, true, 10);
        f.route(PAD, RouteSource::Signal(SIG, false), false, 40);
        assert_eq!(
            f.take_edges(),
            [Edge {
                at: 40,
                pad: PAD,
                level: true
            }]
        );
        assert!(f.pad_level(PAD));
        // And the pad follows it from then on, at the driving cycle.
        f.drive(SIG, false, 50);
        f.drive(SIG, true, 60);
        assert_eq!(
            f.take_edges(),
            [
                Edge {
                    at: 50,
                    pad: PAD,
                    level: false
                },
                Edge {
                    at: 60,
                    pad: PAD,
                    level: true
                }
            ]
        );
    }

    #[test]
    fn only_the_pads_routed_to_the_driven_signal_move() {
        let mut f = Fabric::new();
        f.route(PAD, RouteSource::Signal(SIG, false), false, 0);
        f.route(
            PadId(20),
            RouteSource::Signal(SignalId(72), false),
            false,
            0,
        );
        f.drive(SIG, true, 100);
        let edges = f.take_edges();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].pad, PAD);
        assert!(!f.pad_level(PadId(20)));
    }

    #[test]
    fn a_level_that_does_not_change_is_not_an_edge() {
        let mut f = Fabric::new();
        f.route(PAD, RouteSource::Signal(SIG, false), false, 0);
        f.drive(SIG, true, 10);
        f.drive(SIG, true, 20);
        assert_eq!(f.take_edges().len(), 1);
    }

    #[test]
    fn inv_sel_inverts() {
        let mut f = Fabric::new();
        f.route(PAD, RouteSource::Signal(SIG, true), false, 5);
        // The signal rests low, so an inverted route is high from the route.
        assert_eq!(
            f.take_edges(),
            [Edge {
                at: 5,
                pad: PAD,
                level: true
            }]
        );
        f.drive(SIG, true, 10);
        assert_eq!(
            f.take_edges(),
            [Edge {
                at: 10,
                pad: PAD,
                level: false
            }]
        );
    }

    #[test]
    fn rerouting_to_gpio_out_follows_the_out_bit() {
        let mut f = Fabric::new();
        f.set_gpio_out(PAD, true, 1);
        // Nothing is routed yet: the out bit alone is not an edge.
        assert!(f.take_edges().is_empty());

        f.route(PAD, RouteSource::Signal(SIG, false), false, 10);
        assert!(f.take_edges().is_empty(), "the signal rests low");

        f.route(PAD, RouteSource::GpioOut, true, 20);
        assert_eq!(
            f.take_edges(),
            [Edge {
                at: 20,
                pad: PAD,
                level: true
            }]
        );
        assert!(f.route_of(PAD).unwrap().oen_from_gpio);

        f.set_gpio_out(PAD, false, 30);
        assert_eq!(
            f.take_edges(),
            [Edge {
                at: 30,
                pad: PAD,
                level: false
            }]
        );
        // And the signal it left no longer reaches it.
        f.drive(SIG, true, 40);
        assert!(f.take_edges().is_empty());
    }

    #[test]
    fn unrouting_drops_the_pad_low() {
        let mut f = Fabric::new();
        f.route(PAD, RouteSource::Signal(SIG, false), false, 0);
        f.drive(SIG, true, 10);
        assert_eq!(f.take_edges().len(), 1);
        f.unroute(PAD, 20);
        assert_eq!(
            f.take_edges(),
            [Edge {
                at: 20,
                pad: PAD,
                level: false
            }]
        );
        assert!(f.route_of(PAD).is_none());
        assert_eq!(f.routes().count(), 0);
    }

    #[test]
    fn the_enable_bitmap_is_recorded_and_never_gates() {
        let mut f = Fabric::new();
        f.route(PAD, RouteSource::Signal(SIG, false), false, 0);
        assert!(!f.gpio_enable(PAD));
        f.drive(SIG, true, 10);
        assert_eq!(f.take_edges().len(), 1, "OE low does not suppress the edge");
        f.set_gpio_enable(PAD, true);
        assert!(f.gpio_enable(PAD));
    }

    #[test]
    fn routes_lists_every_routed_pad_ascending() {
        let mut f = Fabric::new();
        f.route(PadId(20), RouteSource::GpioOut, false, 0);
        f.route(PAD, RouteSource::Signal(SIG, false), false, 0);
        let pads: Vec<u8> = f.routes().map(|(p, _)| p.0).collect();
        assert_eq!(pads, [18, 20]);
    }

    // ---- the input side (M2 P1) ------------------------------------------

    #[test]
    fn a_pad_driven_before_anything_routes_it_is_observed_from_the_drive() {
        let mut f = Fabric::new();
        f.drive_pad(PadId(20), true, 100);
        assert!(f.pad_level(PadId(20)), "the driver holds it high");
        assert_eq!(
            f.take_edges(),
            [Edge {
                at: 100,
                pad: PadId(20),
                level: true
            }],
            "nothing routed it and it is observed anyway"
        );
        assert_eq!(f.driven_level(PadId(20)), Some(true));
        assert!(
            f.route_of(PadId(20)).is_none(),
            "an outside driver is not a route"
        );
        // Releasing hands the pad back to a chip side that is not there.
        f.release_pad(PadId(20), 200);
        assert_eq!(
            f.take_edges(),
            [Edge {
                at: 200,
                pad: PadId(20),
                level: false
            }]
        );
        assert_eq!(f.driven_level(PadId(20)), None);
    }

    #[test]
    fn a_driver_that_holds_the_level_it_already_held_is_not_an_edge() {
        let mut f = Fabric::new();
        f.drive_pad(PAD, true, 10);
        f.drive_pad(PAD, true, 20);
        assert_eq!(f.take_edges().len(), 1);
        f.release_pad(PAD, 30);
        f.release_pad(PAD, 40);
        assert_eq!(f.take_edges().len(), 1);
    }

    #[test]
    fn the_input_enable_is_recorded_and_changes_no_level_and_no_edge() {
        let mut f = Fabric::new();
        assert!(!f.pad_input_enable(PAD), "off at reset");
        f.drive_pad(PAD, true, 10);
        assert!(
            f.pad_level(PAD),
            "input enable off, the pad still carries it"
        );
        assert_eq!(f.take_edges().len(), 1);

        f.set_pad_input_enable(PAD, true);
        assert!(f.pad_input_enable(PAD));
        assert!(f.take_edges().is_empty(), "recording a bit is not an edge");
        assert!(f.pad_level(PAD), "and the level did not move either");

        f.set_pad_input_enable(PAD, false);
        assert!(!f.pad_input_enable(PAD));
        assert!(f.pad_level(PAD), "still not gated on");
        assert!(f.take_edges().is_empty());
    }

    #[test]
    fn an_outside_driver_wins_over_an_enabled_output_and_the_conflict_is_logged() {
        let mut f = Fabric::new();
        // gpio18 routed to its own `out` bit, output enabled, driving high.
        f.route(PAD, RouteSource::GpioOut, true, 0);
        f.set_gpio_enable(PAD, true);
        f.set_gpio_out(PAD, true, 10);
        assert!(f.pad_level(PAD));
        assert_eq!(f.conflicts(), 0);

        // A bench driver pulls it to ground: the driver wins, and it is a
        // conflict because the output is enabled.
        f.drive_pad(PAD, false, 20);
        assert!(!f.pad_level(PAD), "the outside driver wins");
        assert_eq!(f.conflicts(), 1);
        assert_eq!(
            f.take_edges().last().copied(),
            Some(Edge {
                at: 20,
                pad: PAD,
                level: false
            }),
            "never gated: the edge is recorded"
        );

        // Agreeing is not a conflict.
        f.set_gpio_out(PAD, false, 30);
        assert_eq!(f.conflicts(), 1);

        // And the driver still wins once the output moves back.
        f.set_gpio_out(PAD, true, 40);
        assert!(!f.pad_level(PAD));
        assert_eq!(f.conflicts(), 2);
    }

    #[test]
    fn a_disagreement_with_a_disabled_output_is_no_conflict() {
        let mut f = Fabric::new();
        // `oen_sel = 1` (the GPIO `enable` bitmap owns the OE) and the bit
        // is clear: the pad is not driving, so there is nothing to conflict
        // with.
        f.route(PAD, RouteSource::GpioOut, true, 0);
        f.set_gpio_out(PAD, true, 10);
        f.drive_pad(PAD, false, 20);
        assert!(!f.pad_level(PAD));
        assert_eq!(f.conflicts(), 0, "an output that is not enabled is quiet");
    }

    #[test]
    fn a_wire_carries_the_level_both_ways_and_both_pads_record_the_edge() {
        let mut f = Fabric::new();
        f.wire(PadId(18), PadId(19), 0).unwrap();
        assert!(f.take_edges().is_empty(), "nothing drives either yet");

        f.drive_pad(PadId(18), true, 100);
        assert!(f.pad_level(PadId(18)));
        assert!(
            f.pad_level(PadId(19)),
            "whatever one carries, the other does"
        );
        assert_eq!(
            f.take_edges(),
            [
                Edge {
                    at: 100,
                    pad: PadId(18),
                    level: true
                },
                Edge {
                    at: 100,
                    pad: PadId(19),
                    level: true
                }
            ]
        );

        // And an output on one side reaches the other — the loopback the
        // RMT's TX pad needs to reach an RX pad.
        f.release_pad(PadId(18), 150);
        let _ = f.take_edges();
        f.route(PadId(18), RouteSource::Signal(SIG, false), false, 200);
        f.drive(SIG, true, 300);
        assert!(f.pad_level(PadId(19)), "gpio18's signal reaches gpio19");
        assert_eq!(
            f.take_edges(),
            [
                Edge {
                    at: 300,
                    pad: PadId(18),
                    level: true
                },
                Edge {
                    at: 300,
                    pad: PadId(19),
                    level: true
                }
            ]
        );
    }

    #[test]
    fn wires_are_transitive_and_the_group_lists_every_tied_pad() {
        let mut f = Fabric::new();
        f.wire(PadId(4), PadId(5), 0).unwrap();
        f.wire(PadId(5), PadId(6), 0).unwrap();
        assert_eq!(
            f.wired_group(PadId(6)),
            [PadId(4), PadId(5), PadId(6)],
            "one group of three, ascending"
        );
        f.drive_pad(PadId(6), true, 10);
        assert!(f.pad_level(PadId(4)));
        assert!(f.pad_level(PadId(5)));
        // A pad nobody wired is a group of one.
        assert_eq!(f.wired_group(PAD), [PAD]);
        assert!(!f.pad_level(PAD));
    }

    #[test]
    fn two_drivers_on_one_group_resolve_to_the_lowest_numbered_pads() {
        let mut f = Fabric::new();
        f.wire(PadId(7), PadId(3), 0).unwrap();
        f.drive_pad(PadId(7), true, 10);
        assert!(f.pad_level(PadId(3)));
        // gpio3 now disagrees; the lower pad number decides, so the group
        // follows gpio3 — deterministic, and stated in the module docs.
        f.drive_pad(PadId(3), false, 20);
        assert!(!f.pad_level(PadId(7)));
        assert!(!f.pad_level(PadId(3)));
    }

    #[test]
    fn a_wire_to_a_pad_that_is_already_high_is_a_rising_edge_at_the_wire() {
        let mut f = Fabric::new();
        f.drive_pad(PadId(8), true, 10);
        let _ = f.take_edges();
        f.wire(PadId(8), PadId(9), 50).unwrap();
        assert_eq!(
            f.take_edges(),
            [Edge {
                at: 50,
                pad: PadId(9),
                level: true
            }]
        );
    }

    #[test]
    fn a_wire_that_makes_no_sense_is_an_error_with_a_message() {
        let mut f = Fabric::new();
        let err = f.wire(PAD, PAD, 0).unwrap_err();
        assert!(err.contains("itself"), "{err}");
        let err = f.wire(PadId(200), PAD, 0).unwrap_err();
        assert!(err.contains("under 64"), "{err}");
        // There is no unwire, and asking says why rather than doing nothing.
        let err = f.unwire(PadId(18), PadId(19)).unwrap_err();
        assert!(err.contains("no unwire"), "{err}");
    }

    #[test]
    fn unrouting_a_pad_an_outside_driver_still_holds_keeps_the_driven_level() {
        let mut f = Fabric::new();
        f.route(PAD, RouteSource::GpioOut, false, 0);
        f.drive_pad(PAD, true, 10);
        let _ = f.take_edges();
        f.unroute(PAD, 20);
        assert!(f.pad_level(PAD), "the bench is still holding it");
        assert!(f.take_edges().is_empty());
        f.release_pad(PAD, 30);
        assert_eq!(
            f.take_edges(),
            [Edge {
                at: 30,
                pad: PAD,
                level: false
            }]
        );
    }

    #[test]
    fn a_pad_past_the_capacity_is_ignored_not_fatal() {
        let mut f = Fabric::new();
        f.route(PadId(200), RouteSource::GpioOut, false, 0);
        f.set_gpio_out(PadId(200), true, 0);
        assert!(!f.pad_level(PadId(200)));
        assert!(f.take_edges().is_empty());
        assert_eq!(f.routes().count(), 0);
    }
}
