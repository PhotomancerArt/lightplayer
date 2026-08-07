//! The **module** card's face — the one face worn at every depth.
//!
//! `docs/design/modules.md` §5: one face, three zoom levels. The root
//! module wears it as the single top-level workspace card (the flat-root
//! reversal — the root now *does* something); an embedded module wears the
//! same face as a child card inside its host; play mode renders the root
//! module's panel alone, without any face at all.
//!
//! Top-down: output-mirror hero (R7) → panel (R8) → the bus-as-wiring
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

use crate::{UiBusView, UiPanelGroup, UiProducedProduct};

/// A module node card's permanent face.
#[derive(Clone, Debug, PartialEq)]
pub struct UiModuleFace {
    /// The module's produced `output` slot, mirroring its own scope's
    /// `visual.out` (R7) — the face hero. `None` for a module with no
    /// visual, which is a legitimate shape (E6).
    pub preview: Option<UiProducedProduct>,
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
    /// The project's **exports rail** (module authoring unit, P3): the
    /// manifest's `exports` list with its lint verdict, rendered as a
    /// section between the wiring drawer and the provenance footer.
    ///
    /// Carried by the ROOT module only — exports are a property of the
    /// project container, and the root card is the container's face.
    /// `None` when the project exports nothing, which keeps a plain
    /// General project visually plain (spike 2·ii).
    pub exports: Option<UiExportsSection>,
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

/// The root card's `exports` section: one row per designated module plus
/// the aggregate lint lines beneath them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UiExportsSection {
    /// One row per manifest `exports` entry, in manifest order.
    pub rows: Vec<UiExportRow>,
    /// Every lint finding across all exports, in report order — the
    /// aggregate view the spike puts under the rows.
    pub findings: Vec<lpc_model::ExportFinding>,
}

impl UiExportsSection {
    /// The worst severity anywhere in the section, or `None` when every
    /// export reads clean.
    pub fn worst(&self) -> Option<lpc_model::ExportSeverity> {
        self.findings.iter().map(|finding| finding.severity).max()
    }
}

/// One row of the root card's exports section.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiExportRow {
    /// The export folder name, exactly as the manifest spells it.
    pub name: String,
    /// The worst finding about THIS export; `None` reads clean (an ok dot).
    pub worst: Option<lpc_model::ExportSeverity>,
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
            panel,
            wiring: None,
            wiring_open: false,
            provenance: None,
            auto_save: None,
            exports: None,
            export: None,
        }
    }
}
