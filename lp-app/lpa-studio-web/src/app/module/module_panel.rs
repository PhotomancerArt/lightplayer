//! A module's **panel**: this scope's channels, plus each child module's
//! panel as a nested group.
//!
//! `docs/design/modules.md` R8. The recursion is presentation only —
//! nothing is promoted, and two embedded instances of one effect present
//! two independent groups because they are two different scopes. A nested
//! group is a hairline-boxed subsection with its own heading, collapsible
//! to a summary row; the root group is never collapsed (it *is* the panel).
//!
//! The two module-level affordances are **quiet chrome in the panel's
//! upper right**, not chips in the content flow — the controls are the
//! subject, and these are the settings that sit beside them:
//!
//! - **Reset panel** (`docs/design/panel.md` P2 clear at scope
//!   granularity): the revert glyph alone, with the count as a small
//!   superscript and the full sentence in its tooltip. Present only while
//!   something under this scope is held, so an untouched panel shows no
//!   destructive control at all.
//! - **Auto-save** (P11 — on by default, with a user toggle): a small
//!   switch. It sits on the module that owns the scope rather than in app
//!   settings, because panel state is per project folder
//!   (`.lp/state.json`) and this is the surface where it is produced.
//!
//! A nested group's reset wears the same small-icon treatment, on the right
//! of its heading row.

use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiPanelGroup};

use crate::base::{StudioIcon, StudioIconName};

use super::{ModulePanelControl, PanelGesture};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ModulePanel(
    /// The scope's panel.
    panel: UiPanelGroup,
    /// Panel-state auto-save (P11); `None` hides the toggle (nested groups
    /// and play mode do not repeat it).
    #[props(default = None)]
    auto_save: Option<bool>,
    /// Roomier play-mode rendering: bigger gaps, no authoring chrome.
    #[props(default = false)]
    play: bool,
    #[props(default = None)] on_panel: Option<EventHandler<PanelGesture>>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    if panel.is_empty() {
        return rsx! {
            div { class: "tw:grid tw:gap-1 tw:px-4 tw:py-3",
                p { class: "tw:m-0 tw:text-sm tw:text-subtle-foreground", "No public channels yet." }
                p { class: "tw:m-0 tw:text-xs tw:leading-snug tw:text-dim-foreground",
                    "Binding a slot to "
                    code { class: "tw:font-mono", "bus:…" }
                    " is what makes it public — and a control appears here."
                }
            }
        };
    }

    let held = panel.engaged_total();
    let scope = panel.scope.clone();
    let row_gap = if play { "tw:gap-6" } else { "tw:gap-4" };

    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-3 tw:px-4 tw:py-3",
            // Module-level affordances, upper right: quiet chrome beside
            // the controls, never competing with them. Reset only exists
            // when there is something to clear.
            if held > 0 || auto_save.is_some() {
                div { class: "tw:flex tw:min-w-0 tw:items-center tw:justify-end tw:gap-1.5",
                    if held > 0 && let Some(handler) = on_panel {
                        PanelResetButton { scope: scope.clone(), held, on_panel: handler }
                    }
                    if let Some(auto_save) = auto_save {
                        AutoSaveToggle { auto_save, on_panel }
                    }
                }
            }
            if !panel.controls.is_empty() {
                div { class: "tw:flex tw:min-w-0 tw:flex-wrap tw:items-start {row_gap}",
                    for control in panel.controls.clone() {
                        ModulePanelControl {
                            key: "{control.channel}",
                            view: control,
                            scope: scope.clone(),
                            play,
                            on_panel,
                            on_action,
                        }
                    }
                }
            }
            for group in panel.groups.clone() {
                NestedPanelGroup {
                    key: "{group.scope}",
                    group,
                    play,
                    on_panel,
                    on_action,
                }
            }
        }
    }
}

/// One embedded module's panel inside its host's panel (R8 recursion).
///
/// Hairline-boxed rather than full-bleed: the box is what makes "these
/// knobs belong to plasma_1, those to plasma_2" readable at a glance, which
/// is the whole point of two side-by-side instances.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn NestedPanelGroup(
    group: UiPanelGroup,
    #[props(default = false)] play: bool,
    #[props(default = None)] on_panel: Option<EventHandler<PanelGesture>>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let held = group.engaged_total();
    let summary = group.summary();
    let scope = group.scope.clone();
    let toggle_scope = group.scope.clone();
    let collapsed = group.collapsed;
    let label = group.label.clone();
    let heading_class = if held > 0 {
        "tw:text-[0.62rem] tw:font-bold tw:uppercase tw:tracking-[0.12em] tw:text-status-attention-foreground"
    } else {
        "tw:text-[0.62rem] tw:font-bold tw:uppercase tw:tracking-[0.12em] tw:text-subtle-foreground"
    };

    rsx! {
        section { class: "tw:grid tw:min-w-0 tw:gap-2 tw:rounded-sm tw:border tw:border-border-muted tw:bg-card-subtle tw:px-3 tw:py-2",
            // Wraps rather than squeezes: at phone width the summary and
            // reset chip drop to a second line instead of crushing the
            // group's name to one letter.
            div { class: "tw:flex tw:min-w-0 tw:flex-wrap tw:items-center tw:gap-x-2 tw:gap-y-1",
                button {
                    class: "tw:group tw:inline-flex tw:flex-none tw:cursor-pointer tw:appearance-none tw:items-center tw:gap-1.5 tw:border-0 tw:bg-transparent tw:p-0 tw:text-left",
                    r#type: "button",
                    aria_expanded: "{!collapsed}",
                    title: if collapsed { "Expand {label}" } else { "Collapse {label}" },
                    onclick: move |event| {
                        event.stop_propagation();
                        if let Some(handler) = on_panel {
                            handler.call(PanelGesture::ToggleGroup { scope: toggle_scope.clone() });
                        }
                    },
                    span { class: "tw:inline-flex tw:flex-none tw:text-subtle-foreground",
                        StudioIcon {
                            name: if collapsed { StudioIconName::Collapsed } else { StudioIconName::Expanded },
                            size: 11,
                        }
                    }
                    span { class: heading_class, "{group.label}" }
                }
                // The scope path: the identity half of every control below
                // (panel.md P1), and what makes two instances distinct.
                span { class: "tw:min-w-0 tw:truncate tw:font-mono tw:text-[0.6rem] tw:text-dim-foreground",
                    "{group.scope}"
                }
                span { class: "tw:ml-auto tw:flex tw:flex-none tw:items-center tw:gap-2",
                    span { class: "tw:font-mono tw:text-[0.6rem] tw:text-dim-foreground", "{summary}" }
                    if held > 0 && let Some(handler) = on_panel {
                        PanelResetButton { scope: scope.clone(), held, on_panel: handler }
                    }
                }
            }
            if !collapsed {
                if !group.controls.is_empty() {
                    div { class: "tw:flex tw:min-w-0 tw:flex-wrap tw:items-start tw:gap-4",
                        for control in group.controls.clone() {
                            ModulePanelControl {
                                key: "{control.channel}",
                                view: control,
                                scope: scope.clone(),
                                play,
                                on_panel,
                                on_action,
                            }
                        }
                    }
                }
                for nested in group.groups.clone() {
                    NestedPanelGroup {
                        key: "{nested.scope}",
                        group: nested,
                        play,
                        on_panel,
                        on_action,
                    }
                }
            }
        }
    }
}

/// The per-scope clear (P2), as a small icon button: the revert glyph
/// alone, amber to match the state it removes, with the count as a tiny
/// superscript. It appears only when there is a writer to remove, so the
/// glyph's mere presence is part of the state signal — and the tooltip
/// carries the whole sentence, so the chrome does not have to.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PanelResetButton(scope: String, held: usize, on_panel: EventHandler<PanelGesture>) -> Element {
    let noun = if held == 1 { "control" } else { "controls" };
    let label = format!(
        "Reset {held} held {noun} in {scope} — panel writers are dropped and the project drives again"
    );
    rsx! {
        button {
            class: "tw:inline-flex tw:h-5 tw:flex-none tw:cursor-pointer tw:appearance-none tw:items-start tw:gap-px tw:rounded-xs tw:border-0 tw:bg-transparent tw:px-1 tw:py-0.5 tw:text-status-attention-foreground tw:opacity-70 tw:hover:opacity-100",
            r#type: "button",
            title: "{label}",
            aria_label: "{label}",
            onclick: move |event| {
                event.stop_propagation();
                on_panel.call(PanelGesture::ClearScope { scope: scope.clone() });
            },
            StudioIcon { name: StudioIconName::Revert, size: 11 }
            // The count, small enough to read as an annotation on the
            // glyph rather than as a chip of its own.
            span { class: "tw:font-mono tw:text-[0.5rem] tw:leading-none", "{held}" }
        }
    }
}

/// The P11 auto-save toggle: whether held values persist to
/// `.lp/state.json` and come back on the next boot. A small switch —
/// a track with a knob, the same language the panel's own toggle control
/// speaks, at chrome size.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn AutoSaveToggle(
    auto_save: bool,
    #[props(default = None)] on_panel: Option<EventHandler<PanelGesture>>,
) -> Element {
    let track_class = if auto_save {
        "tw:relative tw:inline-flex tw:h-[11px] tw:w-[20px] tw:flex-none tw:items-center tw:rounded-full tw:border tw:border-border-strong tw:bg-card-raised"
    } else {
        "tw:relative tw:inline-flex tw:h-[11px] tw:w-[20px] tw:flex-none tw:items-center tw:rounded-full tw:border tw:border-border-muted tw:bg-transparent"
    };
    let knob_class = if auto_save {
        "tw:absolute tw:left-[9px] tw:h-[7px] tw:w-[7px] tw:rounded-full tw:bg-muted-foreground"
    } else {
        "tw:absolute tw:left-[1px] tw:h-[7px] tw:w-[7px] tw:rounded-full tw:bg-dim-foreground"
    };
    let title = if auto_save {
        "Auto-save on — panel settings are saved and restored on boot (.lp/state.json)"
    } else {
        "Auto-save off — panel settings are NOT saved, and are lost on restart"
    };
    let icon_class = if auto_save {
        "tw:inline-flex tw:flex-none tw:text-subtle-foreground"
    } else {
        "tw:inline-flex tw:flex-none tw:text-dim-foreground"
    };

    rsx! {
        button {
            class: "tw:inline-flex tw:h-5 tw:flex-none tw:cursor-pointer tw:appearance-none tw:items-center tw:gap-1 tw:rounded-xs tw:border-0 tw:bg-transparent tw:px-1",
            r#type: "button",
            role: "switch",
            aria_checked: "{auto_save}",
            aria_label: "Auto-save panel settings",
            title: "{title}",
            onclick: move |event| {
                event.stop_propagation();
                if let Some(handler) = on_panel {
                    handler.call(PanelGesture::SetAutoSave(!auto_save));
                }
            },
            span { class: icon_class,
                StudioIcon { name: StudioIconName::Save, size: 11 }
            }
            span { class: track_class,
                span { class: knob_class }
            }
        }
    }
}
