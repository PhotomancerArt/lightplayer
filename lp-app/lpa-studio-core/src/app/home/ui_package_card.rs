//! One library package as the home page shows it.

use crate::app::library::PackageHealth;
use crate::app::library::package_manifest::PATTERN_KIND_LABEL;

/// A library project's card (Other projects, Projects, Your patterns). The
/// thumbnail is deliberately absent from the model: the source is swappable
/// by design (placeholder now, cached rendered frame later) and lives
/// entirely in the renderer.
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
    /// The names of the boards that play this project, in roster order
    /// (empty when none does): the board↔project join's answer
    /// ([`BoardProjects::boards_playing`](crate::BoardProjects::boards_playing)),
    /// stamped by `StudioController::home_view`. An offline board counts.
    pub on_boards: Vec<String>,
    /// Another tab holds this project open (its `lp-project` Web Lock).
    /// Structural actions refuse while set; the card gets the badge
    /// treatment (M4b P4).
    pub open_elsewhere: bool,
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

impl UiPackageCard {
    /// Whether this is a pattern project: one that exports an effect to
    /// build other projects around. The one place core compares the
    /// project's kind label to the pattern's.
    pub fn is_pattern(&self) -> bool {
        self.project_kind == PATTERN_KIND_LABEL
    }
}

#[cfg(test)]
mod tests {
    use lpc_model::ProjectKind;

    use super::*;
    use crate::app::library::package_manifest::kind_label;

    #[test]
    fn only_a_pattern_kind_label_is_a_pattern() {
        let card = |kind: &ProjectKind| UiPackageCard {
            uid: "prjx".to_string(),
            kind: "Module".to_string(),
            project_kind: kind_label(kind).to_string(),
            exports: Vec::new(),
            slug: "x".to_string(),
            last_saved_at: None,
            provenance: None,
            on_boards: Vec::new(),
            open_elsewhere: false,
            target: None,
            health: PackageHealth::Ready,
        };
        let pattern = ProjectKind::Pattern {
            exports: vec!["effect".to_string()],
        };
        let rig = ProjectKind::Rig {
            exports: vec!["panel".to_string()],
        };
        assert!(card(&pattern).is_pattern());
        assert!(!card(&ProjectKind::General).is_pattern());
        assert!(!card(&ProjectKind::Show).is_pattern());
        assert!(!card(&rig).is_pattern());
    }
}
