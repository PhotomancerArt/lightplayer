//! The module card's permanent face — **one face, every depth**.
//!
//! `docs/design/modules.md` §5. The root module wears this as the single
//! top-level workspace card (the flat-root reversal); an embedded module
//! wears the identical component as a child card inside its host, one level
//! in; play mode renders the panel alone with no face at all
//! ([`super::PlayModeSurface`]).
//!
//! Top-down, in the flat [`NodeCardSection`] grammar:
//!
//! 1. `output` — the module's `output` mirror (R7): its scope's
//!    `visual.out`, forwarded. A module with no visual has no hero, which
//!    is a legitimate shape (E6).
//! 2. `panel` — bus-as-**controls** (R8): this scope's channels plus each
//!    child module's panel as a nested group.
//! 3. `wiring` — bus-as-**writers/readers**, as a drawer: exactly the
//!    retired sidebar bus pane's content, scoped to this module and hung
//!    off it (P3). The split is deliberate: controls are the product
//!    surface, wiring is an authoring diagnostic. Its open state is
//!    core-owned (`NodeCardUiState::wiring_open`), like every other card
//!    drawer.
//! 4. `exports` — the project's own face (module authoring unit, P3), on
//!    the ROOT card only: what this project hands out, with its lint
//!    verdict. Absent when the project exports nothing, so a standalone
//!    project stays visually plain. DISPLAY only — designation is a gesture
//!    on each module's detail popup (D12).
//! 5. provenance — a quiet footer line (§8), derived from the module def's
//!    authored `ProvenanceDef` fields.
//!
//! **Children are NOT here.** They expand under the card as full sibling
//! cards, via [`crate::app::node::NodeChildren`] — the grammar the playlist
//! face and the old project node already use. All of them render, with no
//! active-child filtering: a module's children are collaborators, not
//! branches. A child module therefore wears this same face in a card of its
//! own, which is the "one face at every depth" claim made literal *and*
//! keeps the host card a fixed, readable height.
//!
//! An embedded module's controls consequently appear twice — as a nested
//! group on this panel, and on the child module's own card below. That is
//! deliberate, and it is the playlist precedent (a bound control shows on
//! the parent's face and on the child's card): one control, two views
//! (`panel.md` P1), which the shared `(scope, channel)` identity keeps in
//! lockstep.
//!
//! Widget writes ride their own wire op (`PanelWriteOp`, panel.md P8) via
//! the control's `panel_target`; nothing in this face depends on which
//! path carries the value.

use dioxus::prelude::*;
use lpa_studio_core::{
    ExportFinding, ExportSeverity, NodeCardDrawer, NodeUiOp, UiAction, UiExportsSection,
    UiModuleFace as UiModuleFaceData,
};

use crate::app::WiringDrawerBody;
use crate::app::node::{NodeCardSection, ProductPreview, node_ui_action};
use crate::base::StudioIconName;

use super::{ModulePanel, PanelGesture};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ModuleFace(
    face: UiModuleFaceData,
    /// The module's address path — the card UI state key the wiring
    /// drawer's toggle op carries back. `None` in story fixtures that
    /// render a face outside a card, where there is nothing to key.
    #[props(default = None)]
    node: Option<String>,
    #[props(default = None)] on_panel: Option<EventHandler<PanelGesture>>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let preview = face.preview.clone();
    let wiring_open = face.wiring_open;
    let wiring_summary = face.wiring.as_ref().map(|wiring| {
        let channels = wiring.channels.len();
        let noun = if channels == 1 { "channel" } else { "channels" };
        format!("{channels} {noun} · writers → readers")
    });

    rsx! {
        if let Some(preview) = preview {
            NodeCardSection { label: "output", first: true,
                ProductPreview {
                    kind: preview.kind,
                    preview: preview.preview.clone(),
                    tracking: preview.tracking,
                    frame: preview.frame,
                    focus_action: None,
                    on_action,
                }
            }
        }
        // The panel's teaching treatment (spike gate 2): the panel-primary
        // wash + the ▶ rail icon mark THE performable surface — this
        // section is what play mode renders. Leaf cards say "settings";
        // only modules wear "panel".
        NodeCardSection {
            label: "panel",
            first: face.preview.is_none(),
            panel_tint: true,
            icon: StudioIconName::Play,
            ModulePanel {
                panel: face.panel.clone(),
                auto_save: face.auto_save,
                on_panel,
                on_action,
            }
        }
        if let Some(wiring) = face.wiring.clone() {
            NodeCardSection {
                label: "wiring",
                summary: wiring_summary,
                open: Some(wiring_open),
                on_toggle: move |()| {
                    // Core-owned disclosure, exactly like the other card
                    // drawers: the op is keyed by the module's address so
                    // the open state survives re-render and is e2e-drivable.
                    if let (Some(handler), Some(node)) = (on_action, node.as_ref()) {
                        handler.call(node_ui_action(NodeUiOp::SetDrawer {
                            node: node.clone(),
                            drawer: NodeCardDrawer::Wiring,
                            open: !wiring_open,
                        }));
                    }
                },
                div { class: "tw:grid tw:min-w-0 tw:gap-2 tw:px-4 tw:py-3",
                    p { class: "tw:m-0 tw:text-xs tw:leading-snug tw:text-dim-foreground",
                        "Every channel in this module's scope, and what writes and reads it. "
                        "The panel above is the same bus, presented for playing rather than patching."
                    }
                    WiringDrawerBody { view: wiring, on_action: move |action| {
                        if let Some(handler) = on_action {
                            handler.call(action);
                        }
                    } }
                }
            }
        }
        // 5. exports — the CONTAINER's face (module authoring unit, P3).
        // Root card only, and only when the project exports something: a
        // standalone project stays visually a plain project (spike 2·ii).
        // Display only; the designation gesture lives in each module's own
        // detail popup (D12).
        if let Some(exports) = face.exports.clone() {
            NodeCardSection { label: "exports", export_tint: true,
                ExportsRail { exports }
            }
        }
        if let Some(provenance) = face.provenance.clone() {
            div { class: "tw:border-t tw:border-border-strong tw:px-4 tw:py-2 tw:text-xs tw:text-dim-foreground",
                "{provenance}"
            }
        }
    }
}

/// The exports section's body: one row per export with its own lint dot,
/// then the aggregate findings underneath (spike §1).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ExportsRail(exports: UiExportsSection) -> Element {
    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-2 tw:px-4 tw:py-3",
            ul { class: "tw:m-0 tw:grid tw:min-w-0 tw:list-none tw:gap-1 tw:p-0",
                for row in exports.rows.iter() {
                    li {
                        key: "{row.name}",
                        class: "tw:flex tw:min-w-0 tw:items-center tw:gap-2 tw:text-xs",
                        span {
                            class: export_dot_class(row.worst),
                            aria_hidden: "true",
                        }
                        span { class: "tw:min-w-0 tw:truncate tw:font-mono tw:text-muted-foreground",
                            "{row.name}/"
                        }
                    }
                }
            }
            for finding in exports.findings.iter() {
                ExportFindingRow { finding: finding.clone() }
            }
        }
    }
}

/// One lint line, in the severity's own tone. Shared by the card rail and
/// the module detail popup so a finding reads the same in both places.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ExportFindingRow(finding: ExportFinding) -> Element {
    let (class, glyph) = match finding.severity {
        ExportSeverity::Warning => (
            "tw:flex tw:min-w-0 tw:items-start tw:gap-1.5 tw:text-[0.68rem] tw:leading-snug tw:text-status-warning-foreground",
            "⚠",
        ),
        ExportSeverity::Error => (
            "tw:flex tw:min-w-0 tw:items-start tw:gap-1.5 tw:text-[0.68rem] tw:leading-snug tw:text-status-error-foreground",
            "✕",
        ),
    };

    rsx! {
        p { class,
            span { class: "tw:flex-none", aria_hidden: "true", "{glyph}" }
            span { class: "tw:min-w-0 tw:break-words", "{finding.message}" }
        }
    }
}

/// The per-export status dot: sage when the export reads clean, and the
/// warning/error tone when it does not.
fn export_dot_class(worst: Option<ExportSeverity>) -> &'static str {
    match worst {
        None => "tw:h-1.5 tw:w-1.5 tw:flex-none tw:rounded-full tw:bg-status-export-foreground",
        Some(ExportSeverity::Warning) => {
            "tw:h-1.5 tw:w-1.5 tw:flex-none tw:rounded-full tw:bg-status-warning-foreground"
        }
        Some(ExportSeverity::Error) => {
            "tw:h-1.5 tw:w-1.5 tw:flex-none tw:rounded-full tw:bg-status-error-foreground"
        }
    }
}
