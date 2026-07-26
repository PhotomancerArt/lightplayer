//! Per-field provenance: which layer an effective settings value came from.

/// Which layer supplied an effective value (lowest to highest precedence),
/// so the UI can show provenance hints like "from dev settings".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SettingsLayer {
    /// The baked compile-time default (or, for fields without one, "unset").
    #[default]
    Default,
    /// Supplied by the host channel (`/dev-settings.json` in dev; an
    /// Electron preload later).
    Host,
    /// The user's own override, persisted in this browser's localStorage.
    User,
}
