//! The **module** card's face — the one face worn at every depth.
//!
//! `docs/design/modules.md` §5: one face, three zoom levels. The root
//! module wears it as the single top-level workspace card (the flat-root
//! reversal — the root now *does* something); an embedded module wears the
//! same face as a child card inside its host; play mode renders the root
//! module's panel alone, without any face at all.
//!
//! Top-down: product hero (control-first, R7 mirror one toggle away) →
//! panel (R8) → the bus-as-wiring
//! drawer → provenance. The split between the panel and the wiring drawer
//! REPLACED the sidebar bus pane, which is deleted (P3): bus-as-controls
//! sits on the face, bus-as-writers/readers goes in a drawer.
//!
//! **Children are not on the face.** They render below the card as full
//! sibling cards, through the same [`crate::UiNodeChild`] path the playlist
//! and the old project node use — the module contributes no new nesting
//! grammar. All of a module's children render, not just an active one:
//! module children are collaborators, not branches.
//!
//! Every field is derived from real scope data by
//! `ProjectController::module_face`; the story fixtures mirror those
//! shapes rather than standing in for them.

use crate::{ModuleHeroProduct, UiBusView, UiPanelGroup, UiProducedProduct};

/// A module node card's permanent face.
#[derive(Clone, Debug, PartialEq)]
pub struct UiModuleFace {
    /// The face hero: whichever of the module scope's primary products
    /// [`Self::hero_choice`] settles on — by default the `control.out`
    /// lamps, else the R7 `visual.out` mirror. `None` for a module whose
    /// scope resolves neither, which is a legitimate shape (E6).
    pub preview: Option<UiProducedProduct>,
    /// The hero's per-card product preference, present only when the scope
    /// resolves BOTH products — i.e. exactly when the hero is a *choice*
    /// and the face draws its toggle. `None` means there is nothing to
    /// offer: the hero is whatever single product the scope has.
    pub hero_choice: Option<ModuleHeroProduct>,
    /// The module's panel: this scope's channels plus each child module's
    /// panel as a nested group (R8).
    pub panel: UiPanelGroup,
    /// Bus-as-wiring: writers and readers for this scope's channels — what
    /// the sidebar bus pane used to show, now a drawer on the module that
    /// owns the scope. `Some` with no channels is a real shape (a module
    /// publishing nothing); `None` means no binding-graph snapshot yet.
    pub wiring: Option<UiBusView>,
    /// Whether the wiring drawer renders expanded.
    pub wiring_open: bool,
    /// Compact provenance line ("Yona · v1 · CC0-1.0"); `None` when the
    /// module carries no provenance fields (§8).
    pub provenance: Option<String>,
    /// Panel-state auto-save (panel.md P11 — on by default, with a user
    /// toggle). `Some` only on the module that presents the toggle: panel
    /// state persists per project folder (`.lp/state.json`), so it is the
    /// project's root module that owns it, and an embedded module carries
    /// `None` rather than repeating its host's switch.
    pub auto_save: Option<bool>,
    /// This module's own **export designation** row, for its detail popup
    /// (spike 2·i). `None` when designation is not a question for this card
    /// — the root module (an export must never point at the root), a
    /// non-library project, or a `Show`/`Rig` project.
    ///
    /// A `Some` whose [`UiModuleExport::disabled_reason`] is set still
    /// renders: the row explains why the box cannot be ticked rather than
    /// vanishing, the add-node picker's disabled-row precedent.
    pub export: Option<UiModuleExport>,
}

/// How the ROOT card's **child column** splits (module authoring unit, R-A).
///
/// P3 put the project's exports on the root card as a rail of names. G1's
/// ruling replaced it: the exports are the child CARDS themselves, so the
/// column below the root card groups them under an `exports` header and
/// leaves everything else under `rig` — the D17 word for the scaffolding
/// that stays home. Nothing about the root card's own face changed; this
/// rides [`crate::UiNodeView::exports`], because the thing being grouped is
/// the workspace column, not the face.
///
/// `None` (or an empty [`Self::keys`]) means the column renders exactly as
/// it always did — a project that exports nothing stays visually plain
/// (spike 2·ii).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UiExportsGroup {
    /// The child cards that ARE the project's exports, keyed by
    /// [`crate::UiNodeChild::detail`] (the child's address path) so the
    /// renderer partitions by identity, not by display name. Child order,
    /// not manifest order: the column stays the column.
    pub keys: Vec<String>,
    /// Every lint finding across all exports, in report order — the
    /// aggregate preamble under the `exports` header. Per-export findings
    /// still ride each child's own [`UiModuleExport::findings`], which is
    /// what tints its card chip.
    pub findings: Vec<lpc_model::ExportFinding>,
}

impl UiExportsGroup {
    /// The worst severity anywhere in the group, or `None` when every
    /// export reads clean.
    pub fn worst(&self) -> Option<lpc_model::ExportSeverity> {
        self.findings.iter().map(|finding| finding.severity).max()
    }
}

/// One module's export-designation state, as its detail popup presents it.
#[derive(Clone, Debug, PartialEq)]
pub struct UiModuleExport {
    /// The export folder name the toggle would add or remove — the module's
    /// own folder (`fire` for `/fire/module.json`). Empty when the module
    /// has no folder of its own, which is also a `disabled_reason`.
    pub folder: String,
    /// The project the export would ship from — the checkbox copy names it
    /// ("Export from yona-noise"), because the designation is manifest
    /// data even though the gesture lives on the module.
    pub project: String,
    /// Whether the manifest currently lists [`Self::folder`].
    pub designated: bool,
    /// `None` = the checkbox is live. `Some(reason)` = a disabled row whose
    /// sentence says why.
    pub disabled_reason: Option<String>,
    /// Ticking this box would make a `General` project a `Pattern` project
    /// (vision D14's upgrade gesture) — the popup says so plainly.
    pub upgrades_to_pattern: bool,
    /// Lint findings scoped to this export, in report order. Empty unless
    /// the module is actually designated — nothing is checked until it is.
    pub findings: Vec<lpc_model::ExportFinding>,
}

impl UiModuleFace {
    /// A face for `panel`'s module with nothing else filled in.
    pub fn new(panel: UiPanelGroup) -> Self {
        Self {
            preview: None,
            hero_choice: None,
            panel,
            wiring: None,
            wiring_open: false,
            provenance: None,
            auto_save: None,
            export: None,
        }
    }
}
