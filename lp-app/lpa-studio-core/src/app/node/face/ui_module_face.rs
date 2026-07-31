//! The **module** card's face — the one face worn at every depth.
//!
//! `docs/design/modules.md` §5: one face, three zoom levels. The root
//! module wears it as the single top-level workspace card (the flat-root
//! reversal — the root now *does* something); an embedded module wears the
//! same face as a child card inside its host; play mode renders the root
//! module's panel alone, without any face at all.
//!
//! Top-down: output-mirror hero (R7) → panel (R8) → children nested inside
//! the card → the bus-as-wiring drawer → provenance. The split between the
//! panel and the wiring drawer is the sidebar bus pane's replacement:
//! bus-as-controls sits on the face, bus-as-writers/readers goes in a
//! drawer.
//!
//! M2 UX spike: this DTO is fed by mock fixtures. Deriving it from real
//! scope data is M4's work.

use crate::{UiBusView, UiPanelControlView, UiPanelGroup, UiProducedProduct};

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
    /// owns the scope.
    pub wiring: Option<UiBusView>,
    /// Whether the wiring drawer renders expanded.
    pub wiring_open: bool,
    /// Children, nested INSIDE the card: the effect author's zoom level.
    pub children: Vec<UiModuleChild>,
    /// Compact provenance line ("Yona · v1 · CC0-1.0"); `None` when the
    /// module carries no provenance fields (§8).
    pub provenance: Option<String>,
    /// Panel-state auto-save (panel.md P11 — on by default, with a user
    /// toggle). Lives on the module that owns the scope.
    pub auto_save: bool,
}

impl UiModuleFace {
    /// A face for `panel`'s module with nothing else filled in.
    pub fn new(panel: UiPanelGroup) -> Self {
        Self {
            preview: None,
            panel,
            wiring: None,
            wiring_open: false,
            children: Vec::new(),
            provenance: None,
            auto_save: true,
        }
    }
}

/// One child inside a module card.
///
/// A child that is itself a module carries a whole [`UiModuleFace`] and
/// wears the same face one level in — that recursion *is* the "one face at
/// every depth" claim. A leaf child shows its preview and its own panel
/// (its bound slots, R8).
#[derive(Clone, Debug, PartialEq)]
pub struct UiModuleChild {
    /// Child node name.
    pub name: String,
    /// Human-readable node kind ("Shader", "Clock", "Module").
    pub kind: String,
    /// One-line role summary under the name.
    pub summary: Option<String>,
    /// The child's produced visual, when it has one.
    pub preview: Option<UiProducedProduct>,
    /// A leaf child's panel: exactly its bound slots (R3/R8).
    pub controls: Vec<UiPanelControlView>,
    /// Present when the child is a module — it wears the same face.
    pub module: Option<Box<UiModuleFace>>,
    /// Whether the child renders collapsed to its header row.
    pub collapsed: bool,
}

impl UiModuleChild {
    /// A leaf child (shader, clock, fixture…).
    pub fn leaf(name: impl Into<String>, kind: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            kind: kind.into(),
            summary: None,
            preview: None,
            controls: Vec::new(),
            module: None,
            collapsed: false,
        }
    }

    /// A child module, wearing the same face one level in.
    pub fn module(name: impl Into<String>, face: UiModuleFace) -> Self {
        Self {
            name: name.into(),
            kind: "Module".to_string(),
            summary: None,
            preview: face.preview.clone(),
            controls: Vec::new(),
            module: Some(Box::new(face)),
            collapsed: false,
        }
    }

    /// Attach the one-line role summary.
    pub fn with_summary(mut self, summary: impl Into<String>) -> Self {
        self.summary = Some(summary.into());
        self
    }

    /// Attach the child's produced visual.
    pub fn with_preview(mut self, preview: UiProducedProduct) -> Self {
        self.preview = Some(preview);
        self
    }

    /// Attach a leaf child's own panel controls.
    pub fn with_controls(mut self, controls: Vec<UiPanelControlView>) -> Self {
        self.controls = controls;
        self
    }

    /// Render the child collapsed to its header row.
    pub fn collapsed(mut self) -> Self {
        self.collapsed = true;
        self
    }
}
