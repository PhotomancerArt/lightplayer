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
//! # What is modelled, and what is not
//!
//! Modelled: the routing itself, the driven level of a signal, the level of a
//! routed pad, and the **edge** — the cycle a routed pad's level changed.
//!
//! Not modelled: input (nothing drives a pad from outside), output enable
//! (`oen_sel` and the `enable` bitmap are *recorded* and reported in the
//! trace note, never gated on — a pad whose OE is low still records its
//! edges here), drive strength, pull-ups, open-drain and pad filters. Those
//! are electrical facts, and a decoder that reads this fabric would not see
//! them on a real logic analyser either.

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
    edges: Vec<Edge>,
    /// Edges dropped because [`EDGE_CAP`] was reached.
    dropped: u64,
    /// Bumped whenever a pad's routing changed. A machine that watches the
    /// routing compares this instead of walking every pad every slice.
    epoch: u64,
}

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
        if self.level[i] {
            self.level[i] = false;
            self.push(Edge {
                at,
                pad,
                level: false,
            });
        }
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

    /// Take the edges recorded since the last call. The machine drains this
    /// every slice, in order.
    pub fn take_edges(&mut self) -> Vec<Edge> {
        core::mem::take(&mut self.edges)
    }

    /// Edges lost to [`EDGE_CAP`] since the fabric was built.
    pub fn dropped_edges(&self) -> u64 {
        self.dropped
    }

    /// Recompute pad `i`'s level from its route and record an edge if it
    /// moved. A pad with no route is not observed at all.
    fn settle(&mut self, i: usize, at: Cycles) {
        let Some(route) = self.routes[i] else { return };
        let level = match route.source {
            RouteSource::Signal(s, invert) => self.signal_level(s) != invert,
            RouteSource::GpioOut => self.gpio_out[i],
        };
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
