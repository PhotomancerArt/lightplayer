//! One device as the gallery's *Devices* roster shows it.

use crate::app::roster::RosterCardState;

/// A device card. Visually distinct from package cards by contract: the
/// renderer gives it a hardware header (status circle + transport) so it
/// never reads as "just another project". The card's health lives in
/// [`RosterCardState`] (the 14-state vocabulary, derived from evidence by
/// `derive_roster_card_state`); the project chip is identity, not status.
#[derive(Clone, Debug, PartialEq)]
pub struct UiDeviceCard {
    /// `dev_…` uid when the device is registered; `None` for a live
    /// connection that has no stamped identity yet.
    pub uid: Option<String>,
    pub name: String,
    /// Transport label ("USB" today; a different glyph for networked
    /// later). Empty while a connect is still resolving the provider.
    pub transport: String,
    /// Where the card stands in the honest roster vocabulary.
    pub state: RosterCardState,
    /// The project the device holds (live cards) or last ran (offline
    /// cards) — identity for the header chip, never health.
    pub project: Option<UiDeviceProjectChip>,
}

impl UiDeviceCard {
    /// Stable identity for keyed rendering. Names are NOT unique — erasing
    /// and re-provisioning a board registers a new `dev_…` uid under the
    /// same name, and a keyed list with duplicate keys panics Dioxus (the
    /// 2026-07-15 home-gallery crash). Registered cards key by uid; only
    /// the (single) identity-less live card falls back to its name.
    pub fn render_key(&self) -> &str {
        self.uid.as_deref().unwrap_or(&self.name)
    }
}

/// The header chip naming the device's project: thumbnail seed + display
/// name. Identity only — the status line and circle carry health. On
/// offline/error cards the renderer mutes it (last-known, not current).
#[derive(Clone, Debug, PartialEq)]
pub struct UiDeviceProjectChip {
    /// `prj_…` uid — thumbnail seed and the push/review target key.
    pub uid: String,
    /// Display name (library slug; a deleted project falls back to uid).
    pub name: String,
}
