//! Soft limits: measured envelopes a board×firmware pairing has actually
//! run clean at.
//!
//! These are the *other* kind of limit from
//! `lpc_model::ManifestLimits` — that struct carries facts true by
//! construction (partition layout, chip RAM), and its docs promise that
//! measured envelopes never live there. A soft limit is evidence, not
//! policy: exceeding one **warns and proceeds** — the record tells an
//! operator what has been proven, not what is forbidden. Refusing would
//! invert the convention (and punish anyone probing past the envelope with
//! a scope in hand).
//!
//! The first record is the total-LED budget: on the classic ESP32 the
//! binding resource at high LED counts is the heap (~89.5 B/LED of
//! duplication across engine and output stages, and the heap is two
//! regions — watch `largest_free`), which binds well before frame time
//! does. "8 wires" must never be read as 8× dome strips; the honest 8-wire
//! tier is 8×~200 at today's envelope.
//!
//! The second and third records are the smoothing limits: total-LED counts
//! above which the output provider opens display pipelines with frame
//! interpolation (`interpolation_leds`) and then temporal dithering
//! (`dithering_leds`) turned off. Interpolation holds `prev` + `next`
//! (12 B/LED) and dithering a carry (3 B/LED), per port — at 100 LEDs that
//! is nothing, at a dome strip's 1,500 it is the largest per-LED line on
//! the classic's heap. The limits make that trade board-side and
//! deterministic (a function of the lamps open, never of the heap's mood);
//! a board without a record never degrades. See
//! `docs/adr/2026-09-06-smoothing-degrades-by-measured-lamp-limits.md`.

use alloc::string::String;

use serde::{Deserialize, Serialize};

/// One measured envelope: a value plus the provenance that makes it
/// evidence rather than a guess.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct HwMeasuredLimit {
    /// The measured envelope value (unit is the field's, e.g. LEDs).
    pub value: u32,
    /// Where the number came from: date, firmware, workload, and the
    /// observed margins. Free text, for humans reading a warning.
    pub measured: String,
}

/// The soft-limit records a board manifest carries. All optional — a
/// manifest states only what has actually been measured.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct HwSoftLimits {
    /// Total LEDs across all wires this board×firmware has run clean at.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_leds: Option<HwMeasuredLimit>,
    /// Total LEDs across all open ports above which the output provider
    /// opens display pipelines with frame interpolation OFF (freeing
    /// 12 B/LED). Absent = interpolation follows the authored option at any
    /// scale. Ordered below `dithering_leds` by intent: interpolation is
    /// 4× the bytes and a second `write_frame` per tick, so it goes first.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interpolation_leds: Option<HwMeasuredLimit>,
    /// Total LEDs across all open ports above which the output provider
    /// opens display pipelines with temporal dithering OFF too (freeing
    /// 3 B/LED). Absent = dithering follows the authored option at any
    /// scale.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dithering_leds: Option<HwMeasuredLimit>,
}

impl HwSoftLimits {
    /// Is there anything here at all? (Serialization gate.)
    pub fn is_empty(&self) -> bool {
        self.total_leds.is_none()
            && self.interpolation_leds.is_none()
            && self.dithering_leds.is_none()
    }
}
