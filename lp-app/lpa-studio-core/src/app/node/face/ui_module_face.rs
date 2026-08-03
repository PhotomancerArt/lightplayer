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
        }
    }
}
