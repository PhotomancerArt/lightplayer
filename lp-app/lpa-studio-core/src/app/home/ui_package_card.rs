//! One library package as the gallery shows it.

/// A "Your projects" card. The thumbnail is deliberately absent from the
/// model: the source is swappable by design (placeholder now, cached rendered
/// frame later) and lives entirely in the renderer.
#[derive(Clone, Debug, PartialEq)]
pub struct UiPackageCard {
    /// `prj_…` uid string — the identity every card action carries.
    pub uid: String,
    /// Manifest kind ("Project" today, "Module" later).
    pub kind: String,
    /// THE user-facing identifier (dated: `2026-07-09-1421-basic`): card
    /// title, URL, export name. Rename edits it.
    pub slug: String,
    /// The last `Saved` event's timestamp (f64 epoch seconds), or the
    /// package's creation time before any save.
    pub last_saved_at: Option<f64>,
    /// Human provenance line for remixes/forks/imports; `None` for
    /// created-from-scratch packages.
    pub provenance: Option<String>,
    /// Parity line: the name of a registered device currently holding this
    /// package's head, when one does ("On <name> ✓").
    pub on_device: Option<String>,
}
