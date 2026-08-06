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
}

impl HwSoftLimits {
    /// Is there anything here at all? (Serialization gate.)
    pub fn is_empty(&self) -> bool {
        self.total_leds.is_none()
    }
}
