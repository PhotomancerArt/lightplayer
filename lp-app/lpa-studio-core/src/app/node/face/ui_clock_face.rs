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

use crate::{UiProducedProduct, UiProducedValue};

/// Kind-specific face for a clock node.
#[derive(Clone, Debug, PartialEq)]
pub struct UiClockFace {
    /// The published time-product row: identity, detail, and the `bus:time`
    /// binding chip. The same row the produced-products section carries.
    pub product: UiProducedProduct,
    /// Plain produced readings kept beside the listing (`seconds`,
    /// `delta_seconds`) — the numbers the product handle no longer puts on
    /// the bus, still produced-but-unbound for exactly this reason.
    pub readings: Vec<UiProducedValue>,
    /// What the timebase probe last said about this product.
    pub timebase: UiTimebaseState,
    /// Live integrators riding this timebase, in store order.
    ///
    /// **Empty is a normal state**, not a failure: a project whose shaders
    /// declare no phasor has none, and so does one whose phasors have all
    /// gone idle. Rows are inherently transient — a phasor no consumer asked
    /// for in the last couple of seconds is simply gone from the next read.
    pub phasors: Vec<UiPhasorRow>,
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

/// One live phasor integrator, prepared for display.
///
/// Read-only by construction: there is nothing here a client could use to
/// address the integrator, only enough provenance to *name* it in a row.
#[derive(Clone, Debug, PartialEq)]
pub struct UiPhasorRow {
    /// Who this integrator belongs to: the consuming node ("plasma"), or the
    /// bus channel every reader of it shares ("bus:speed").
    pub origin: String,
    /// What sits behind the origin: the CONSUMED slot for a private
    /// integrator (the uniform's own path — two uniforms on one node are
    /// two integrators), or the scope for a shared one.
    pub detail: Option<String>,
    /// The integrator is keyed by a `(scope, channel)` config channel, so
    /// **every reader of that channel rides this one phase** (parent D3).
    /// A private integrator (keyed by node + slot) is nobody else's.
    pub shared: bool,
    /// Wrapped cycle position in `[0,1)` — the geometry the position bar
    /// follows. Quantized with [`Self::phase_display`], so a slow phasor's
    /// row only changes when its readout does.
    pub phase: f32,
    /// The phase as text (≤2 decimals).
    ///
    /// The RAW ramp: `waveform` and `phase_offset` are per-consumer output
    /// shaping applied AFTER the store, so one shared integrator has exactly
    /// one phase and possibly several differently-shaped readings of it.
    /// Listing a waveform here would have to pick one reader's and call it
    /// the phasor's — so the listing reports the phase, never a shaped value.
    pub phase_display: String,
    /// Completed cycles since the integrator materialized.
    pub cycle: u32,
    /// The period the integrator last advanced at, as text ("4s"), or
    /// "frozen" when the rate is zero.
    pub period_display: String,
}

impl UiClockFace {
    /// A listing with nothing in it yet.
    #[must_use]
    pub fn new(product: UiProducedProduct) -> Self {
        Self {
            product,
            readings: Vec::new(),
            timebase: UiTimebaseState::Unread,
            phasors: Vec::new(),
        }
    }
}

/// Quantize a live phase for display: at most 2 decimals, so the DTO change
/// gate only fires when the *shown* number moves. A phasor with a 100 s
/// period then dirties the card roughly once a second instead of once a
/// frame; a 1 s phasor still moves every frame, which is the honest cost of
/// a live debug listing (and it only runs while the clock card is
/// subscribed).
#[must_use]
pub fn format_phase(phase: f32) -> String {
    if !phase.is_finite() {
        return "—".to_string();
    }
    let rounded = (phase * 100.0).round() / 100.0;
    if rounded.fract() == 0.0 {
        format!("{rounded:.1}")
    } else {
        rounded.to_string()
    }
}

/// A phasor's period as text. `0` (or a non-finite/negative value) is
/// **frozen** — the phasor holds its phase rather than resetting, and the
/// word says so instead of printing a rate nothing is running at.
#[must_use]
pub fn format_period_seconds(period_seconds: f32) -> String {
    if !period_seconds.is_finite() || period_seconds <= 0.0 {
        return "frozen".to_string();
    }
    let rounded = (period_seconds * 100.0).round() / 100.0;
    if rounded.fract() == 0.0 {
        format!("{rounded:.0}s")
    } else {
        format!("{rounded}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frozen_phasor_says_so_rather_than_printing_a_rate() {
        assert_eq!(format_period_seconds(0.0), "frozen");
        assert_eq!(format_period_seconds(-1.0), "frozen");
        assert_eq!(format_period_seconds(f32::NAN), "frozen");
        assert_eq!(format_period_seconds(4.0), "4s");
        assert_eq!(format_period_seconds(0.25), "0.25s");
        assert_eq!(format_period_seconds(100.0), "100s");
    }

    #[test]
    fn phase_display_quantizes_so_the_dto_gate_stays_quiet() {
        assert_eq!(format_phase(0.0), "0.0");
        assert_eq!(format_phase(0.25), "0.25");
        // Two ticks a hair apart read the same, so the row does not change.
        assert_eq!(format_phase(0.123_4), format_phase(0.124_9));
        assert_eq!(format_phase(f32::NAN), "—");
    }
}
