//! The clock card's face: the published time product plus a read-only
//! listing of the phasors riding its timebase (roadmap D10).
//!
//! Since the M2 break `bus:time` carries a `TimeProduct` handle, and
//! everything behind that handle — effective seconds, this tick's delta, and
//! every phasor a consumer materialized — lives in the engine's timebase
//! store rather than in any slot. Nothing on the ordinary read surface can
//! see it, which is why the clock's card had no way to answer "what is
//! actually running right now?".
//!
//! This face is that answer and **nothing more**: a debug listing, not a
//! control panel. There is no gesture here that creates, retunes, or deletes
//! a phasor — phasors materialize on query and despawn on silence, and both
//! are the consuming node's business. The one place a phasor's period IS
//! editable is the consuming shader's own period knob.

use lpc_model::Waveform;

use crate::{ProjectSlotAddress, UiProducedProduct};

/// Kind-specific face for a clock node.
#[derive(Clone, Debug, PartialEq)]
pub struct UiClockFace {
    /// The published time-product row: identity, detail, and the `bus:time`
    /// binding chip. The same row the produced-products section carries.
    pub product: UiProducedProduct,
    /// The transport instrument's live facts — run/pause, rate, scrub
    /// offset, and probe-anchored numeric seconds — lifted from the
    /// flattened `transport.*` Debug rows (plan
    /// 2026-08-04-2355-clock-tape-hero, P2). `None` when the Debug rows
    /// have not landed yet (unread project), the same "no read yet, not a
    /// failure" posture [`UiTimebaseState::Unread`] uses.
    pub transport: Option<UiClockTransport>,
    /// What the timebase probe last said about this product.
    pub timebase: UiTimebaseState,
    /// Trace cards, one per downstream READING riding this timebase, in
    /// store order (a shared integrator contributes one card per reader).
    ///
    /// **Empty is a normal state**, not a failure: a project whose shaders
    /// declare no phasor has none, and so does one whose phasors have all
    /// gone idle. Cards are inherently transient — a phasor no consumer
    /// asked for in the last couple of seconds is simply gone from the next
    /// read.
    pub phasors: Vec<UiPhasorReading>,
}

/// The clock's tape transport, as the card and panel widgets (P3/P4) will
/// render it: current values (edit buffer included — a staged drag reads
/// back through this DTO immediately, the echo-suppression contract the
/// widgets rely on) plus the addresses `SetValue` dispatches target.
///
/// `seconds` is the probe-anchored effective time, copied in by
/// [`crate::ProjectController::apply_clock_faces`] from the cached
/// [`UiTimebaseRead::Live`] read the same decoration pass already
/// consults for the phasor listing — numeric, never formatted, so the web
/// driver can extrapolate between pulls. It stays `0.0` until a probe
/// read lands, same as [`UiTimebaseState::Unread`] leaves the phasor list
/// empty rather than showing a stale number.
#[derive(Clone, Debug, PartialEq)]
pub struct UiClockTransport {
    /// Probe-anchored effective seconds (numeric, not display text).
    pub seconds: f32,
    /// Whether the transport is currently running, as currently
    /// staged/acked (edit buffer included).
    pub running: bool,
    /// The transport's rate multiplier, as currently staged/acked.
    pub rate: f32,
    /// The transport's scrub offset in seconds, as currently staged/acked.
    pub scrub_offset_seconds: f32,
    /// `SetValue` target for `running`; `None` = not editable.
    pub running_address: Option<ProjectSlotAddress>,
    /// `SetValue` target for `rate`; `None` = not editable.
    pub rate_address: Option<ProjectSlotAddress>,
    /// `SetValue` target for `scrub_offset_seconds`; `None` = not editable.
    pub scrub_address: Option<ProjectSlotAddress>,
}

/// The timebase probe's verdict for a clock's product.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiTimebaseState {
    /// No timebase read has landed yet (the card just mounted, or the clock
    /// is not subscribed for products).
    Unread,
    /// The runtime published a timebase; [`UiClockFace::phasors`] is what
    /// rides it.
    Live,
    /// The runtime resolves no timebase for this product — the node just
    /// left the tree, or has never produced. Structured rather than an
    /// error: a card asking about a node that just left is not a fault.
    Unknown,
}

/// One downstream reading of a live phasor, prepared for a trace card.
///
/// Read-only by construction: there is nothing here a client could use to
/// address the integrator, only enough provenance to *name* the card and
/// enough shaping to *draw* the consumer's actual waveform.
#[derive(Clone, Debug, PartialEq)]
pub struct UiPhasorReading {
    /// Who reads it: "plasma · phase" (reader node label · consumed slot),
    /// with the departed-node fallback ("node 8 · phase"). A row the probe
    /// caught before its first tick-side reading landed falls back to the
    /// integrator's own origin name so the card count never flickers to
    /// zero.
    pub label: String,
    /// The shared channel behind a shared integrator ("bus:speed in Orbit")
    /// — `None` for a private one. Tooltip fodder; the violet border is the
    /// visible signal.
    pub detail: Option<String>,
    /// The integrator is keyed by a `(scope, channel)` config channel, so
    /// **every reader of that channel rides this one phase** (parent D3).
    /// Violet border + violet id per the bound-violet convention; the trace
    /// itself stays black-and-white.
    pub shared: bool,
    /// Wrapped cycle position in `[0,1)` at the probe — the RAW ramp. The
    /// trace extrapolates from here between probes (`φ + elapsed/T`) and
    /// corrects when the next probe lands.
    pub phase: f32,
    /// Completed cycles since the integrator materialized.
    pub cycle: u32,
    /// The period the integrator last advanced at, in seconds. `0.0` means
    /// frozen — the trace holds still.
    pub period_seconds: f32,
    /// Auto-denominated rate ("2/s", "3/min", "15/hr"; frozen = "0/s") —
    /// the unit-awareness principle, same string the speed knob shows.
    pub rate_display: String,
    /// This reader's output shaping — what the trace actually draws.
    pub waveform: Waveform,
    /// Added to the wrapped phase before shaping.
    pub phase_offset: f32,
}

impl UiClockFace {
    /// A listing with nothing in it yet.
    #[must_use]
    pub fn new(product: UiProducedProduct) -> Self {
        Self {
            product,
            transport: None,
            timebase: UiTimebaseState::Unread,
            phasors: Vec::new(),
        }
    }
}

/// A period presented as an auto-denominated rate: `0.5` → `2/s`, `20` →
/// `3/min`, `240` → `15/hr` (G2 convergence — pick the smallest time unit
/// that keeps the number ≥ 1, so the reading is always a natural count; the
/// unit is part of the string). A frozen phasor (period ≤ 0, or non-finite)
/// never cycles: `0/s`.
#[must_use]
pub fn phasor_rate_display(period_seconds: f32) -> String {
    if !period_seconds.is_finite() || period_seconds <= 0.0 {
        return "0/s".to_string();
    }
    // Smallest unit whose count reaches 1; /hr is the floor either way.
    let (count, unit) = [(1.0, "s"), (60.0, "min"), (3600.0, "hr")]
        .into_iter()
        .map(|(seconds, unit)| (seconds / period_seconds, unit))
        .find(|(count, unit)| *count >= 1.0 || *unit == "hr")
        .expect("the ladder always yields");
    let number = if count >= 9.95 {
        format!("{}", count.round() as i64)
    } else {
        let rounded = (count * 10.0).round() / 10.0;
        if rounded.fract() == 0.0 {
            format!("{}", rounded as i64)
        } else {
            format!("{rounded:.1}")
        }
    };
    format!("{number}/{unit}")
}

/// [`phasor_rate_display`] over a formatted period reading — the shape the
/// panel readout path has in hand. A reading that does not parse passes
/// through untouched.
#[must_use]
pub fn phasor_speed_display(shown: &str) -> String {
    match shown.trim().parse::<f32>() {
        Ok(period) => phasor_rate_display(period),
        Err(_) => shown.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The auto-denominated ladder (G2 convergence): smallest unit keeping
    /// the count ≥ 1, unit riding the string, frozen = 0/s.
    #[test]
    fn rates_auto_denominate_and_frozen_never_cycles() {
        assert_eq!(phasor_rate_display(0.5), "2/s");
        assert_eq!(phasor_rate_display(20.0), "3/min");
        assert_eq!(phasor_rate_display(240.0), "15/hr");
        assert_eq!(phasor_rate_display(100.0), "36/hr");
        assert_eq!(phasor_rate_display(1.0), "1/s");
        assert_eq!(phasor_rate_display(0.0), "0/s");
        assert_eq!(phasor_rate_display(-3.0), "0/s");
        assert_eq!(phasor_rate_display(f32::NAN), "0/s");
    }

    /// The string entry point (panel readouts): parses and delegates, and a
    /// non-numeric reading passes through untouched.
    #[test]
    fn speed_display_parses_or_passes_through() {
        assert_eq!(phasor_speed_display("0.5"), "2/s");
        assert_eq!(phasor_speed_display("  20 "), "3/min");
        assert_eq!(phasor_speed_display("frozen"), "frozen");
    }
}
