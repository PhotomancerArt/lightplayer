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
//! 3. `children` — the module's children, nested INSIDE the card. This is
//!    the effect-author zoom level, and the reason the root module belongs
//!    back in the node area: the workspace column becomes one card whose
//!    inside *is* the project.
//! 4. `wiring` — bus-as-**writers/readers** (today's sidebar bus-pane
//!    content), as a drawer. The split is deliberate: controls are the
//!    product surface, wiring is an authoring diagnostic.
//! 5. provenance — a quiet footer line (§8).
//!
//! **Spike shortcut, not a proposal:** a knob drag still dispatches
//! `SlotEditOp::SetValue` at the control's mock slot address, because that
//! is what makes knob v2 work untouched. Panel writes are their own wire op
//! (panel.md P8) and M4 routes them there; nothing in this face depends on
//! which path carries the value.

use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiModuleChild, UiModuleFace as UiModuleFaceData};

use crate::app::BusPaneBody;
use crate::app::node::{NodeCardSection, ProductPreview};
use crate::base::{StudioIcon, node_kind_icon};

use super::{ModulePanel, ModulePanelControl, PanelGesture};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ModuleFace(
    face: UiModuleFaceData,
    /// Nesting depth: 0 is the root module's workspace card, 1+ is an
    /// embedded module inside its host. Depth only affects chrome density
    /// — the sections and their order are identical, which is the claim
    /// under test at G2.
    #[props(default = 0)]
    depth: usize,
    /// Toggle handler for the wiring drawer (spike-local view state).
    #[props(default = None)]
    on_wiring_toggle: Option<EventHandler<()>>,
    #[props(default = None)] on_panel: Option<EventHandler<PanelGesture>>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let preview = face.preview.clone();
    let has_children = !face.children.is_empty();
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
        NodeCardSection { label: "panel", first: face.preview.is_none(),
            ModulePanel {
                panel: face.panel.clone(),
                // Auto-save belongs to the module that OWNS the scope, so a
                // nested face does not repeat its host's toggle.
                auto_save: (depth == 0).then_some(face.auto_save),
                on_panel,
                on_action,
            }
        }
        if has_children {
            NodeCardSection { label: "children",
                div { class: "tw:grid tw:min-w-0 tw:gap-2 tw:px-4 tw:py-3",
                    for child in face.children.clone() {
                        ModuleChildCard {
                            key: "{child.name}",
                            child,
                            depth: depth + 1,
                            scope: face.panel.scope.clone(),
                            on_panel,
                            on_action,
                        }
                    }
                }
            }
        }
        if let Some(wiring) = face.wiring.clone() {
            NodeCardSection {
                label: "wiring",
                summary: wiring_summary,
                open: Some(wiring_open),
                on_toggle: move |()| {
                    if let Some(handler) = on_wiring_toggle {
                        handler.call(());
                    }
                },
                div { class: "tw:grid tw:min-w-0 tw:gap-2 tw:px-4 tw:py-3",
                    p { class: "tw:m-0 tw:text-xs tw:leading-snug tw:text-dim-foreground",
                        "Every channel in this module's scope, and what writes and reads it. "
                        "The controls above are the same bus, presented for playing rather than patching."
                    }
                    BusPaneBody { view: wiring, on_action: move |action| {
                        if let Some(handler) = on_action {
                            handler.call(action);
                        }
                    } }
                }
            }
        }
        if let Some(provenance) = face.provenance.clone() {
            div { class: "tw:border-t tw:border-border-strong tw:px-4 tw:py-2 tw:text-xs tw:text-dim-foreground",
                "{provenance}"
            }
        }
    }
}

/// One child inside a module card.
///
/// A child module renders the SAME [`ModuleFace`] one level in — that
/// recursion is the "one face at every depth" claim made literal. A leaf
/// child renders its preview and its own panel (its bound slots, R3/R8).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ModuleChildCard(
    child: UiModuleChild,
    depth: usize,
    /// The ENCLOSING module's scope. A leaf child introduces no scope of
    /// its own (R1), so its bound slots' channels — and therefore its
    /// controls' identity (panel.md P1) — belong to this one.
    scope: String,
    #[props(default = None)] on_panel: Option<EventHandler<PanelGesture>>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let UiModuleChild {
        name,
        kind,
        summary,
        preview,
        controls,
        module,
        collapsed,
    } = child;
    let is_module = module.is_some();
    // A nested module card wears a slightly stronger edge than a leaf: the
    // thing that owns a scope should read as a container.
    let card_class = if is_module {
        "tw:grid tw:min-w-0 tw:gap-0 tw:overflow-hidden tw:rounded-sm tw:border tw:border-border-strong tw:bg-card"
    } else {
        "tw:grid tw:min-w-0 tw:gap-0 tw:overflow-hidden tw:rounded-sm tw:border tw:border-border-muted tw:bg-card-subtle"
    };
    let scope_hint = module.as_ref().map(|module| module.panel.scope.clone());

    rsx! {
        section { class: card_class,
            div { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-2 tw:border-b tw:border-border-muted tw:px-3 tw:py-1.5",
                span { class: "tw:inline-flex tw:flex-none tw:text-subtle-foreground",
                    StudioIcon { name: node_kind_icon(&kind), size: 12 }
                }
                span { class: "tw:min-w-0 tw:truncate tw:text-sm tw:text-strong-foreground", "{name}" }
                if let Some(summary) = summary {
                    span { class: "tw:min-w-0 tw:truncate tw:text-[11px] tw:text-dim-foreground", "{summary}" }
                }
                span { class: "tw:ml-auto tw:flex tw:flex-none tw:items-center tw:gap-2",
                    if let Some(scope) = scope_hint {
                        span {
                            class: "tw:font-mono tw:text-[0.6rem] tw:text-dim-foreground",
                            title: "This module introduces a scope",
                            "scope {scope}"
                        }
                    }
                    span { class: "tw:text-[11px] tw:font-bold tw:lowercase tw:tracking-wide tw:text-dim-foreground",
                        "{kind}"
                    }
                }
            }
            if !collapsed {
                if let Some(module) = module {
                    // The same face, one level in.
                    ModuleFace {
                        face: *module,
                        depth,
                        on_panel,
                        on_action,
                    }
                } else {
                    if let Some(preview) = preview {
                        ProductPreview {
                            kind: preview.kind,
                            preview: preview.preview.clone(),
                            tracking: preview.tracking,
                            frame: preview.frame,
                            focus_action: None,
                            on_action,
                        }
                    }
                    if !controls.is_empty() {
                        div { class: "tw:flex tw:min-w-0 tw:flex-wrap tw:items-start tw:gap-4 tw:px-3 tw:py-2",
                            for control in controls {
                                ModulePanelControl {
                                    key: "{control.channel}",
                                    view: control,
                                    scope: scope.clone(),
                                    on_panel,
                                    on_action,
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
