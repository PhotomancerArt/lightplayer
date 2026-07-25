//! The card-state presentation spec: treatment × status family,
//! renderer-independent.
//!
//! Born as the status-circle spec (M2; the ADR
//! `2026-07-16-device-card-state-vocabulary.md`). The circle retired with
//! M7′ — the card's tinted LEFT EDGE carries the same grammar now — but
//! the DERIVATION is unchanged: renderers (the web card's edge chrome
//! today; on-device LEDs later) map this spec onto their own medium.
//!
//! Treatment and motion carry meaning without color:
//! filled = live link, remembered = no live link, working = in flight.
//! The tone reuses the existing status families (green good, amber
//! attention, red broken, gray neutral) — no parallel color vocabulary.

use crate::UiStatusKind;

/// What a roster card's state presentation should communicate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RosterStateSpec {
    pub treatment: RosterTreatment,
    pub tone: UiStatusKind,
}

/// The treatment grammar (direction.md "Card grammar"; the retired
/// circle's shapes, re-homed on the edge: solid→filled, hollow→
/// remembered, pulsing→working).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RosterTreatment {
    /// A live link exists to the thing this card describes.
    Filled,
    /// Remembered only — no live link (offline registry cards).
    Remembered,
    /// Work is in flight (connecting, flashing, pushing).
    Working,
}
