//! The well-known bus channel registry.
//!
//! Channels are created lazily by reference — this registry does not gate
//! anything. It is how the editing UX *teaches* the naming norms (ADR
//! 2026-07-08-binding-ref-syntax-and-channel-naming): the binding picker
//! seeds its list from here, kind/unit hints come from here, and slot
//! `default_bind` declarations target these names. Arbitrary channel names
//! remain legal.

use crate::Kind;

/// One well-known channel: canonical name, semantic kind, and the docs the
/// picker surfaces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WellKnownChannel {
    /// Canonical channel name (`purpose[.in|.out]`, unitless — unit truth
    /// lives in slot metadata and `doc`).
    pub name: &'static str,
    /// Semantic kind used for picker labels and mismatch hints.
    pub kind: Kind,
    /// One-line description shown by the picker.
    pub doc: &'static str,
}

/// The canonical channel set, in picker display order.
pub const WELL_KNOWN_CHANNELS: &[WellKnownChannel] = &[
    WellKnownChannel {
        name: "time",
        kind: Kind::Instant,
        doc: "Project clock in seconds; the clock publishes it by default.",
    },
    WellKnownChannel {
        name: "trigger",
        kind: Kind::Instant,
        doc: "Control events (button presses, remote triggers); map readers merge by message id.",
    },
    WellKnownChannel {
        name: "visual.out",
        kind: Kind::Color,
        doc: "The project's primary visual output; fixtures sample it.",
    },
    WellKnownChannel {
        name: "control.out",
        kind: Kind::Color,
        doc: "Rendered control samples; hardware outputs drive from it.",
    },
];

/// Look up a well-known channel by name.
pub fn well_known_channel(name: &str) -> Option<&'static WellKnownChannel> {
    WELL_KNOWN_CHANNELS
        .iter()
        .find(|channel| channel.name == name)
}
