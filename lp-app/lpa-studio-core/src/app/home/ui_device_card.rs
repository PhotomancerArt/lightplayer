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

/// The M4 slice of the device state chart. The full chart
/// (blank / mid-flash / diverged verbs) is M5's — recorded as O2.
#[derive(Clone, Debug, PartialEq)]
pub enum UiDeviceCardState {
    /// Connected and running: click opens the editor against the device.
    ConnectedRunning {
        /// The project the device holds, when known.
        project: Option<String>,
    },
    /// Remembered but offline: muted card from the registry.
    RememberedOffline {
        /// f64 epoch seconds.
        last_seen_at: f64,
        /// "Name vN" of the last-known pushed project, when recorded.
        last_known: Option<String>,
    },
}
