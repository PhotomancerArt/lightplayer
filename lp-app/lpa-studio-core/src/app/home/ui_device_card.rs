//! One device as the gallery's *Connected* section shows it.

/// A device card. Visually distinct from package cards by contract: the
/// renderer gives it a hardware header (connection dot + transport) and a
/// parity footer so it never reads as "just another project".
#[derive(Clone, Debug, PartialEq)]
pub struct UiDeviceCard {
    /// `dev_…` uid when the device is registered; `None` for a live
    /// connection that has no stamped identity yet (pre-M5).
    pub uid: Option<String>,
    pub name: String,
    /// Transport label ("USB" today; a different glyph for networked later).
    pub transport: String,
    pub state: UiDeviceCardState,
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

/// The device card state chart (O2, settled in M5). Under D24
/// unification, a connected device holding a LOCALLY-KNOWN project has
/// no device card at all — the project card carries the connected
/// indication — so the connected states here cover only devices whose
/// contents aren't a local project.
#[derive(Clone, Debug, PartialEq)]
pub enum UiDeviceCardState {
    /// Connected, no firmware answering: click opens the deploy wizard.
    Blank,
    /// Connected and running a project (shown when the project is not a
    /// local library entry — otherwise D24 unifies onto the project
    /// card). Click opens the editor against the device.
    ConnectedRunning {
        /// The project the device holds, when known.
        project: Option<String>,
    },
    /// Connected but the contents are unreadable or awaiting identity.
    ConnectedUnknown { detail: String },
    /// Remembered but offline: muted card from the registry.
    RememberedOffline {
        /// f64 epoch seconds.
        last_seen_at: f64,
        /// "Name vN" of the last-known pushed project, when recorded.
        last_known: Option<String>,
    },
}
