//! Studio pane layout component: the shared chrome grammar for editing panes.
//!
//! Anatomy (the P5 ADR lifts this):
//!
//! ```text
//! [collapse?] [primary] [title/kind] [state chips] … [actions] [trailing] [detail]
//! --------------------------------- body ---------------------------------
//! ```
//!
//! - `collapse` — optional leftmost rail toggling the body.
//! - `primary` — primary-affordance element slot, left of the title (status
//!   icon, selection control, …).
//! - `title` / `kind` — pane identity text; the title truncates, everything
//!   else keeps its width.
//! - `chrome` — the pane's one neutral chrome struct ([`PaneChrome`]): header
//!   tone, accent outline, and state chips. Consumers map their domain state
//!   (`UiStatusKind`, `DirtySummary`, …) onto it; the pane imports no node,
//!   project, or device types.
//! - `actions` — contextual [`UiPaneAction`]s rendered as icon buttons that
//!   dispatch the wrapped action through the usual `on_action` conduit.
//! - `trailing` — free-form header extras between the actions and the detail
//!   popup (node tabs, the legacy upper-right select control, …).
//! - `detail` — detail-popup slot at the header's right edge (a
//!   `DetailPopover`).
//! - `body` — pane body below the header; `None` renders a header-only pane
//!   (the same shape a collapsed pane folds to).
//!
//! This is a **layout** component: slots, placement, and spacing only. All
//! domain knowledge stays with the consumers.

use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiPaneAction};

use crate::base::{StudioIcon, StudioIconName, action_icon_name};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn StudioPane(
    /// Optional collapse rail (state + handler); the pane holds no state.
    #[props(default)]
    collapse: Option<PaneCollapse>,
    /// Primary affordance slot, left of the title.
    #[props(default)]
    primary: Option<Element>,
    /// Pane title.
    title: String,
    /// Optional action dispatched when the title is activated. When set, the
    /// title becomes a hoverable selection control (e.g. a node pane whose name
    /// selects the node) instead of static text.
    #[props(default)]
    title_action: Option<UiAction>,
    /// Optional kind/subtype text after the title.
    #[props(default)]
    kind: Option<String>,
    /// Neutral chrome: header tone, accent outline, state chips.
    #[props(default)]
    chrome: PaneChrome,
    /// Contextual header actions rendered as icon buttons.
    #[props(default)]
    actions: Vec<UiPaneAction>,
    /// Action dispatch conduit for the actions slot.
    #[props(default)]
    on_action: Option<EventHandler<UiAction>>,
    /// Free-form header extras between the actions and the detail popup.
    #[props(default)]
    trailing: Option<Element>,
    /// Detail-popup slot at the header's right edge.
    #[props(default)]
    detail: Option<Element>,
    /// Pane body; `None` renders a header-only pane.
    #[props(default)]
    body: Option<Element>,
) -> Element {
    let collapsed = collapse.as_ref().is_some_and(|collapse| collapse.collapsed);
    let show_body = body.is_some() && !collapsed;
    let surface_class = pane_surface_class(chrome.accent);
    let header_class = pane_header_class(chrome.tone, collapse.is_some(), !show_body);

    rsx! {
        article { class: "{surface_class}",
            header { class: "{header_class}",
                if let Some(collapse) = collapse {
                    PaneCollapseButton { collapse }
                }
                div { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-2 tw:px-3",
                    if let Some(primary) = primary {
                        {primary}
                    }
                    if let Some(action) = title_action {
                        button {
                            class: "tw:m-0 tw:-mx-1 tw:min-w-0 tw:flex-1 tw:truncate tw:rounded-xs tw:border-0 tw:bg-transparent tw:px-1 tw:py-0.5 tw:text-left tw:text-[1.04rem] tw:font-bold tw:leading-tight tw:text-strong-foreground tw:transition-colors tw:hover:bg-card-subtle/70",
                            r#type: "button",
                            onclick: move |event| {
                                event.stop_propagation();
                                if let Some(handler) = on_action {
                                    handler.call(action.clone());
                                }
                            },
                            "{title}"
                        }
                    } else {
                        h3 { class: "tw:m-0 tw:min-w-0 tw:overflow-hidden tw:text-ellipsis tw:whitespace-nowrap tw:text-[1.04rem] tw:font-bold tw:leading-tight tw:text-strong-foreground",
                            "{title}"
                        }
                    }
                    if let Some(kind) = kind {
                        span { class: "tw:shrink-0 tw:text-xs tw:font-bold tw:text-subtle-foreground", "{kind}" }
                    }
                    for chip in chrome.chips {
                        span { class: pane_chip_class(chip.tone), title: "{chip.title}", "{chip.text}" }
                    }
                }
                div { class: "tw:flex tw:h-full tw:items-stretch",
                    for action in actions {
                        PaneActionButton { action, on_action }
                    }
                    if let Some(trailing) = trailing {
                        {trailing}
                    }
                    if let Some(detail) = detail {
                        div { class: "tw:flex tw:items-center tw:px-1.5", {detail} }
                    }
                }
            }
            if show_body {
                {body}
            }
        }
    }
}

/// Neutral chrome for one pane: everything the pane draws that is not an
/// element slot. Consumers map their domain state onto it (`UiStatusKind` →
/// [`PaneTone`], `DirtySummary` → chips, focus → `accent`, …).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PaneChrome {
    /// Tone family washing the header strip.
    pub tone: PaneTone,
    /// Draw the pane outline in the neutral selection color (e.g. the
    /// focused node) — deliberately not a status color, so selection never
    /// reads as semantic beside a dirty tint.
    pub accent: bool,
    /// State chips after the title; empty renders no chip.
    pub chips: Vec<PaneChip>,
}

/// Neutral tone vocabulary for pane chrome (header wash, chips).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PaneTone {
    /// Idle/unchanged.
    #[default]
    Neutral,
    /// In progress.
    Working,
    /// Healthy/running.
    Good,
    /// Live-only (transient) state, blue.
    Live,
    /// Unsaved/edited, yellow (node edit vocabulary).
    Warning,
    /// Health needs a look, orange (device/roster attention family).
    Attention,
    /// Failed, red.
    Error,
}

/// One state chip in the pane header: a toned pill with a short text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaneChip {
    /// Tone family for the pill.
    pub tone: PaneTone,
    /// Pill text (e.g. a count: "3 unsaved").
    pub text: String,
    /// Tooltip explaining the state.
    pub title: String,
}

/// State + handler for the pane's optional collapse rail.
#[derive(Clone, PartialEq)]
pub struct PaneCollapse {
    /// Whether the body is currently folded away.
    pub collapsed: bool,
    /// Accessible label while collapsed (e.g. "Expand node").
    pub expand_label: String,
    /// Accessible label while expanded (e.g. "Collapse node").
    pub collapse_label: String,
    /// Toggle handler; the consumer owns the state.
    pub on_toggle: EventHandler<()>,
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PaneCollapseButton(collapse: PaneCollapse) -> Element {
    let PaneCollapse {
        collapsed,
        expand_label,
        collapse_label,
        on_toggle,
    } = collapse;
    let label = if collapsed {
        expand_label
    } else {
        collapse_label
    };

    rsx! {
        button {
            class: "tw:inline-flex tw:h-full tw:min-h-[46px] tw:w-[34px] tw:items-center tw:justify-center tw:border-0 tw:border-r tw:border-border-muted tw:bg-transparent tw:p-0 tw:text-subtle-foreground tw:hover:bg-card-subtle/60",
            r#type: "button",
            aria_label: "{label}",
            title: "{label}",
            onclick: move |event| {
                event.stop_propagation();
                on_toggle.call(());
            },
            StudioIcon {
                name: if collapsed { StudioIconName::Collapsed } else { StudioIconName::Expanded },
                size: 14,
            }
        }
    }
}

/// One contextual action icon button in the header's actions slot.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PaneActionButton(
    action: UiPaneAction,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let enabled = action.is_enabled();
    let icon = action_icon_name(Some(action.icon.as_str())).unwrap_or(StudioIconName::Info);
    let label = action.label().to_string();
    let title = if action.summary().is_empty() {
        label.clone()
    } else {
        action.summary().to_string()
    };
    let class = pane_action_button_class(action.is_primary(), enabled);
    let dispatch = action.action.clone();

    rsx! {
        button {
            class,
            r#type: "button",
            disabled: !enabled,
            aria_label: "{label}",
            title: "{title}",
            onclick: move |event| {
                event.stop_propagation();
                if let Some(handler) = on_action {
                    handler.call(dispatch.clone());
                }
            },
            StudioIcon {
                name: icon,
                size: 15,
            }
        }
    }
}

fn pane_surface_class(accent: bool) -> String {
    let border_class = if accent {
        "tw:border-selection-border"
    } else {
        "tw:border-border"
    };
    format!(
        "tw:grid tw:min-w-0 tw:overflow-hidden tw:rounded-md tw:border {border_class} tw:bg-card tw:p-4"
    )
}

fn pane_header_class(tone: PaneTone, has_collapse: bool, header_only: bool) -> String {
    let columns_class = if has_collapse {
        "tw:grid-cols-[34px_minmax(0,1fr)_auto]"
    } else {
        "tw:grid-cols-[minmax(0,1fr)_auto]"
    };
    let shape_class = if header_only {
        "tw:-mb-4 tw:rounded-md"
    } else {
        "tw:rounded-t-md tw:border-b tw:border-border-muted"
    };
    let tint_class = pane_header_tint_class(tone);

    format!(
        "tw:-mx-4 tw:-mt-4 tw:grid tw:min-h-[46px] tw:min-w-0 {columns_class} tw:items-stretch tw:overflow-hidden {shape_class} tw:bg-card-subtle {tint_class}"
    )
}

fn pane_header_tint_class(tone: PaneTone) -> &'static str {
    match tone {
        PaneTone::Neutral => {
            "tw:bg-[linear-gradient(90deg,var(--studio-status-neutral-bg),transparent_62%)]"
        }
        PaneTone::Working => {
            "tw:bg-[linear-gradient(90deg,var(--studio-status-working-bg),transparent_62%)]"
        }
        PaneTone::Good => {
            "tw:bg-[linear-gradient(90deg,var(--studio-status-good-bg),transparent_62%)]"
        }
        PaneTone::Live => {
            "tw:bg-[linear-gradient(90deg,var(--studio-status-live-bg),transparent_62%)]"
        }
        PaneTone::Warning => {
            "tw:bg-[linear-gradient(90deg,var(--studio-status-warning-bg),transparent_62%)]"
        }
        PaneTone::Attention => {
            "tw:bg-[linear-gradient(90deg,var(--studio-status-attention-bg),transparent_62%)]"
        }
        PaneTone::Error => {
            "tw:bg-[linear-gradient(90deg,var(--studio-status-error-bg),transparent_66%)]"
        }
    }
}

fn pane_chip_class(tone: PaneTone) -> &'static str {
    match tone {
        PaneTone::Neutral => {
            "tw:shrink-0 tw:whitespace-nowrap tw:rounded-pill tw:border tw:border-status-neutral-border tw:bg-status-neutral-bg tw:px-2 tw:py-0.5 tw:text-xs tw:font-bold tw:leading-none tw:text-status-neutral-foreground"
        }
        PaneTone::Working => {
            "tw:shrink-0 tw:whitespace-nowrap tw:rounded-pill tw:border tw:border-status-working-border tw:bg-status-working-bg tw:px-2 tw:py-0.5 tw:text-xs tw:font-bold tw:leading-none tw:text-status-working-foreground"
        }
        PaneTone::Good => {
            "tw:shrink-0 tw:whitespace-nowrap tw:rounded-pill tw:border tw:border-status-good-border tw:bg-status-good-bg tw:px-2 tw:py-0.5 tw:text-xs tw:font-bold tw:leading-none tw:text-status-good-foreground"
        }
        PaneTone::Live => {
            "tw:shrink-0 tw:whitespace-nowrap tw:rounded-pill tw:border tw:border-status-live-border tw:bg-status-live-bg tw:px-2 tw:py-0.5 tw:text-xs tw:font-bold tw:leading-none tw:text-status-live-foreground"
        }
        PaneTone::Warning => {
            "tw:shrink-0 tw:whitespace-nowrap tw:rounded-pill tw:border tw:border-status-warning-border tw:bg-status-warning-bg tw:px-2 tw:py-0.5 tw:text-xs tw:font-bold tw:leading-none tw:text-status-warning-foreground"
        }
        PaneTone::Attention => {
            "tw:shrink-0 tw:whitespace-nowrap tw:rounded-pill tw:border tw:border-status-attention-border tw:bg-status-attention-bg tw:px-2 tw:py-0.5 tw:text-xs tw:font-bold tw:leading-none tw:text-status-attention-foreground"
        }
        PaneTone::Error => {
            "tw:shrink-0 tw:whitespace-nowrap tw:rounded-pill tw:border tw:border-status-error-border tw:bg-status-error-bg tw:px-2 tw:py-0.5 tw:text-xs tw:font-bold tw:leading-none tw:text-status-error-foreground"
        }
    }
}

fn pane_action_button_class(primary: bool, enabled: bool) -> &'static str {
    match (primary, enabled) {
        (_, false) => {
            "tw:inline-flex tw:h-full tw:min-h-[46px] tw:w-[34px] tw:items-center tw:justify-center tw:border-0 tw:border-l tw:border-border-muted tw:bg-transparent tw:p-0 tw:text-dim-foreground tw:opacity-50 tw:cursor-not-allowed"
        }
        (true, true) => {
            "tw:inline-flex tw:h-full tw:min-h-[46px] tw:w-[34px] tw:items-center tw:justify-center tw:border-0 tw:border-l tw:border-border-muted tw:bg-transparent tw:p-0 tw:text-accent tw:hover:bg-card-subtle/60"
        }
        (false, true) => {
            "tw:inline-flex tw:h-full tw:min-h-[46px] tw:w-[34px] tw:items-center tw:justify-center tw:border-0 tw:border-l tw:border-border-muted tw:bg-transparent tw:p-0 tw:text-subtle-foreground tw:hover:bg-card-subtle/60 tw:hover:text-strong-foreground"
        }
    }
}
