//! One library package as the gallery shows it.

use crate::app::library::PackageHealth;

/// A "Your projects" card. The thumbnail is deliberately absent from the
/// model: the source is swappable by design (placeholder now, cached rendered
/// frame later) and lives entirely in the renderer.
#[derive(Clone, Debug, PartialEq)]
pub struct UiPackageCard {
    /// `prj…` uid string — the identity every card action carries.
    pub uid: String,
    /// Manifest kind (`"Module"`; pre-rename packages said `"Project"`).
    pub kind: String,
    /// Display label for the project's authored kind (`"General"` |
    /// `"Pattern"` | `"Show"` | `"Rig"`), from `ProjectManifest.kind`/
    /// `exports` via `ProjectManifest::project_kind` (module authoring
    /// unit, P1). Distinct from [`Self::kind`] above (the pre-mitosis
    /// root-artifact kind tag, always `"Module"` today) — this is the
    /// project's own authored designation, feeding the P4/P5 gallery UI.
    /// `"General"` for a degraded card whose manifest could not be read.
    pub project_kind: String,
    /// The module folders this project exports, in manifest order (module
    /// authoring unit, P1/P5). Empty for every kind that exports nothing —
    /// and for a degraded card whose manifest could not be read. Feeds the
    /// card's "New project from this…" row (which export to vendor) and
    /// the add-node picker's import source.
    pub exports: Vec<String>,
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
    /// Another tab holds this project open (its `lp-project` Web Lock).
    /// Structural actions refuse while set; the card gets the badge
    /// treatment (M4b P4).
    pub open_elsewhere: bool,
    /// The live SIM session currently runs this project (the D28 grammar's
    /// sim arm — one fact, two views: the sim card wears the project chip,
    /// this card wears the "Running in simulator" indication).
    pub running_in_sim: bool,
    /// The project's advisory `target` (gallery-rework vision D3): a board
    /// catalog id in the registry's `vendor/product` vocabulary, straight
    /// from `ProjectManifest.target`. `None` for an untargeted project. The
    /// renderer turns this into a quiet "for \<board\>" badge; no other
    /// meaning attaches to it here — the engine never reads it, and the
    /// mismatch warning is P06's job, not this card's.
    pub target: Option<String>,

    /// The package's format standing: openable as-is, openable after an
    /// automatic migration, or not openable at all — in which case the card
    /// says what was found and what to do instead of the package quietly
    /// not being here (P3).
    pub health: PackageHealth,
}
