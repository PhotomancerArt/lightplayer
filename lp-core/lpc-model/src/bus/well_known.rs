//! The well-known bus channel registry.
//!
//! Channels are created lazily by reference — this registry does not gate
//! anything. It is how the editing UX *teaches* the naming norms (ADR
//! 2026-07-08-binding-ref-syntax-and-channel-naming): the binding picker
//! seeds its list from here, kind/unit hints come from here, and slot
//! `default_bind` declarations target these names. Arbitrary channel names
//! remain legal.

use crate::Kind;

/// The channel whose resolved value is the project's **primary visual** —
/// "the project's face" for previews, gallery cards, and thumbnailers
/// (ADR 2026-07-16-primary-visual-product). The one place this name is
/// written; every consumer references the constant.
pub const PRIMARY_VISUAL_CHANNEL: &str = "visual.out";

/// The analogous convention for control-first projects (declared for
/// symmetry by the same ADR; no preview surface consumes it yet).
pub const PRIMARY_CONTROL_CHANNEL: &str = "control.out";

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
    /// The channel carries a **product handle** (a lazy graph capability),
    /// not a plain scalar. Binding one onto a scalar slot resolves to a
    /// value the consumer cannot convert — the engine reports that as a
    /// per-input failure and a `Warn` card, which is the backstop; this
    /// flag is what lets the binding picker say so BEFORE the pick
    /// (`bus:time` carries a `TimeProduct` since the M2 break).
    pub carries_product: bool,
}

/// The canonical channel set, in picker display order.
pub const WELL_KNOWN_CHANNELS: &[WellKnownChannel] = &[
    WellKnownChannel {
        name: "time",
        kind: Kind::Instant,
        doc: "Project time product; query it for seconds, delta, and phasors.",
        carries_product: true,
    },
    WellKnownChannel {
        name: "trigger",
        kind: Kind::Instant,
        doc: "Control events (button presses, remote triggers); map readers merge by message id.",
        carries_product: false,
    },
    WellKnownChannel {
        name: "brightness",
        kind: Kind::Amplitude,
        doc: "Master output brightness (0-1); fixtures consume it by default.",
        carries_product: false,
    },
    WellKnownChannel {
        name: PRIMARY_VISUAL_CHANNEL,
        kind: Kind::Color,
        doc: "The project's primary visual output; fixtures sample it.",
        carries_product: true,
    },
    WellKnownChannel {
        name: PRIMARY_CONTROL_CHANNEL,
        kind: Kind::Color,
        doc: "Rendered control samples; hardware outputs drive from it.",
        carries_product: true,
    },
];

/// Look up a well-known channel by name.
pub fn well_known_channel(name: &str) -> Option<&'static WellKnownChannel> {
    WELL_KNOWN_CHANNELS
        .iter()
        .find(|channel| channel.name == name)
}
