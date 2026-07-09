use dioxus::prelude::*;
use lpa_studio_core::{
    DirtySummary, UiAction, UiNodeSection, UiNodeTabBody, UiNodeView, UiPendingEdit, UiSlotRecord,
};

use crate::app::affordance::affordance_pane_tone;
use crate::app::layout::{PaneChrome, PaneCollapse, StudioPane};
use crate::app::node::{
    NodeChildren, NodeDetailPopover, ProducedProducts, ProducedValues, SlotRecordEditor,
};
use crate::base::{StudioIcon, node_kind_icon};

/// Which surface treatment a dirty node pane wears — the D7 tint experiment,
/// story-selectable pending the user's P5 pick.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum NodeDirtyTint {
    /// Dirty tint on the header strip only, plus the state chip (live
    /// default).
    #[default]
    HeaderOnly,
    /// Dirty tint re-mixed into the whole pane surface (header + body).
    FullSurface,
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn NodePane(
    view: UiNodeView,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
    /// The editor-level pending-edit list, threaded to every pane's detail
    /// popover (which filters it to its own node) and down through nested
    /// child panes.
    #[props(default)]
    pending_edits: Vec<UiPendingEdit>,
    #[props(default)] dirty_tint: NodeDirtyTint,
) -> Element {
    let mut active_tab = use_signal(|| 0_usize);
    let mut collapsed = use_signal(|| view.collapsed);
    let active_index = active_tab().min(view.tabs.len().saturating_sub(1));
    let active_body = view.tabs.get(active_index).map(|tab| tab.body.clone());
    let dirty = view.header.dirty;
    // P6 affordance model: the header carries no count chips — the merged
    // affordance on the detail trigger is the whole announcement, and the
    // per-bucket counts live in the detail popup.
    let chrome = PaneChrome {
        tone: affordance_pane_tone(view.header.affordance(), view.header.status.kind),
        accent: view.focused,
        chips: Vec::new(),
    };
    let surface_class = pane_surface_tint_class(dirty_tint, dirty);
    let header = view.header.clone();
    let title = view.header.title.clone();
    let tabs = view.tabs.clone();
    let focused = view.focused;
    let select_action = view.action.clone();
    let select_kind = view.header.kind.clone();
    let focus_action = view.action.clone();
    let issues = view.issues.clone();
    let header_actions = view.header_actions.clone();

    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-3",
            div { class: surface_class,
                StudioPane {
                    collapse: PaneCollapse {
                        collapsed: collapsed(),
                        expand_label: "Expand node".to_string(),
                        collapse_label: "Collapse node".to_string(),
                        on_toggle: EventHandler::new(move |()| collapsed.set(!collapsed())),
                    },
                    primary: rsx! {
                        if let Some(action) = select_action {
                            NodeSelectButton {
                                action,
                                focused,
                                kind: select_kind,
                                on_action,
                            }
                        }
                    },
                    title,
                    title_action: focus_action.clone(),
                    chrome,
                    actions: header_actions,
                    on_action,
                    trailing: rsx! {
                        if tabs.len() > 1 {
                            NodeTabs {
                                tabs: tabs.clone(),
                                active_index,
                                on_select: move |index| active_tab.set(index),
                            }
                        }
                    },
                    detail: rsx! {
                        NodeDetailPopover {
                            header,
                            pending_edits: pending_edits.clone(),
                            on_action,
                        }
                    },
                    body: rsx! {
                        if !issues.is_empty() {
                            ul { class: "tw:m-0 tw:grid tw:list-none tw:gap-1 tw:rounded-sm tw:border tw:border-status-error-border tw:bg-status-error-bg tw:p-3",
                                for issue in issues.clone() {
                                    li { class: "tw:text-sm tw:text-status-error-foreground", "{issue}" }
                                }
                            }
                        }
                        match active_body {
                            Some(UiNodeTabBody::Sections(sections)) => rsx! {
                                div { class: "tw:-mx-4 tw:-mb-4 tw:grid tw:min-w-0",
                                    for (index, section) in sections.into_iter().enumerate() {
                                        NodeSection {
                                            section,
                                            first: index == 0,
                                            focus_action: focus_action.clone(),
                                            on_action,
                                            pending_edits: pending_edits.clone(),
                                            dirty_tint,
                                        }
                                    }
                                }
                            },
                            Some(UiNodeTabBody::Text { title, body }) => rsx! {
                                section { class: "tw:grid tw:min-w-0 tw:gap-2",
                                    h4 { class: "tw:m-0 tw:text-xs tw:font-bold tw:uppercase tw:text-heading", "{title}" }
                                    pre { class: "tw:m-0 tw:max-h-80 tw:overflow-auto tw:rounded-sm tw:border tw:border-border-subtle tw:bg-page tw:p-3 tw:text-xs tw:leading-normal tw:text-muted-foreground",
                                        code { "{body}" }
                                    }
                                }
                            },
                            None => rsx! {
                                p { class: "tw:m-0 tw:text-sm tw:text-subtle-foreground", "No node tabs are available." }
                            },
                        }
                    },
                }
            }
            if !collapsed() && !view.children.is_empty() {
                NodeChildren {
                    items: view.children.clone(),
                    on_action,
                    pending_edits,
                    dirty_tint,
                }
            }
        }
    }
}

/// Selection indicator/toggle in the pane's primary-affordance slot, left of
/// the node name (D3).
///
/// Selecting a node focuses it (probes ride the focused node), so body
/// clicks stay inert and only this control dispatches the focus action —
/// editing another node's slots never steals the selection.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn NodeSelectButton(
    action: UiAction,
    focused: bool,
    /// The node's kind label — its glyph doubles as the select control.
    kind: String,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let icon = node_kind_icon(&kind);
    let (class, label) = if focused {
        (
            "tw:inline-flex tw:h-8 tw:w-8 tw:shrink-0 tw:items-center tw:justify-center tw:rounded-full tw:border tw:border-selection-border tw:bg-transparent tw:p-0 tw:text-strong-foreground",
            "Node is selected; probes follow this node",
        )
    } else {
        (
            "tw:inline-flex tw:h-8 tw:w-8 tw:shrink-0 tw:items-center tw:justify-center tw:rounded-full tw:border tw:border-border-subtle tw:bg-transparent tw:p-0 tw:text-subtle-foreground tw:hover:border-accent-border tw:hover:text-accent",
            "Select this node so probes follow it",
        )
    };

    rsx! {
        button {
            class,
            r#type: "button",
            aria_label: label,
            aria_pressed: "{focused}",
            title: label,
            onclick: move |event| {
                event.stop_propagation();
                if let Some(handler) = on_action {
                    handler.call(action.clone());
                }
            },
            StudioIcon {
                name: icon,
                size: 15,
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn NodeSection(
    section: UiNodeSection,
    #[props(default = false)] first: bool,
    #[props(default)] focus_action: Option<UiAction>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
    /// The editor-level pending-edit list, threaded through extracted child
    /// sections into their nested panes' detail popovers.
    #[props(default)]
    pending_edits: Vec<UiPendingEdit>,
    #[props(default)] dirty_tint: NodeDirtyTint,
) -> Element {
    match section {
        UiNodeSection::ProducedProducts(products) => rsx! {
            section { class: section_class("tw:bg-card tw:p-0", first),
                ProducedProducts { products, focus_action, on_action }
            }
        },
        UiNodeSection::ProducedValues(values) => rsx! {
            section { class: section_class("tw:bg-card-subtle tw:px-4 tw:py-4", first),
                ProducedValues { values }
            }
        },
        UiNodeSection::ConfigSlots(slots) => rsx! {
            section { class: section_class("tw:bg-card tw:p-0", first),
                SlotRecordEditor {
                    record: UiSlotRecord::new(slots),
                    on_action,
                }
            }
        },
        UiNodeSection::AssetSlots(assets) => rsx! {
            section { class: section_class("tw:bg-card tw:p-0", first),
                SlotRecordEditor {
                    record: UiSlotRecord::new(assets),
                    on_action,
                }
            }
        },
        UiNodeSection::Children(children) => rsx! {
            section { class: section_class("tw:bg-card tw:px-4 tw:py-4", first),
                NodeChildren {
                    items: children,
                    on_action,
                    pending_edits,
                    dirty_tint,
                }
            }
        },
    }
}

fn section_class(body_class: &'static str, first: bool) -> String {
    if first {
        format!("tw:min-w-0 {body_class}")
    } else {
        format!("tw:min-w-0 tw:border-t tw:border-border-muted {body_class}")
    }
}

/// Wrapper class around the pane for the D7 full-surface variant: a
/// `display: contents` wrapper that re-mixes the pane's card tokens with the
/// dominant dirty status color so the whole pane (header and body) tints.
///
/// The override targets the Tailwind-level `--tw-color-card*` tokens: the
/// Studio `--studio-color-*` custom properties are substituted into them at
/// `:root`, so re-declaring the Studio tokens on a wrapper never reaches
/// descendants' `bg-card*` utilities.
///
/// `HeaderOnly` (the live default) never re-mixes; `FullSurface` re-mixes on
/// dirty panes and resets a clean pane back to the base surface tokens so a
/// clean child nested inside a dirty parent's body is not tinted.
fn pane_surface_tint_class(variant: NodeDirtyTint, dirty: DirtySummary) -> &'static str {
    match variant {
        NodeDirtyTint::HeaderOnly => "tw:contents",
        NodeDirtyTint::FullSurface => {
            if dirty.failed > 0 {
                "tw:contents tw:[--tw-color-card:color-mix(in_oklab,var(--studio-status-error-bg)_55%,var(--studio-color-surface))] tw:[--tw-color-card-subtle:color-mix(in_oklab,var(--studio-status-error-bg)_55%,var(--studio-color-surface-subtle))] tw:[--tw-color-card-muted:color-mix(in_oklab,var(--studio-status-error-bg)_55%,var(--studio-color-surface-muted))]"
            } else if dirty.persisted > 0 {
                "tw:contents tw:[--tw-color-card:color-mix(in_oklab,var(--studio-status-warning-bg)_55%,var(--studio-color-surface))] tw:[--tw-color-card-subtle:color-mix(in_oklab,var(--studio-status-warning-bg)_55%,var(--studio-color-surface-subtle))] tw:[--tw-color-card-muted:color-mix(in_oklab,var(--studio-status-warning-bg)_55%,var(--studio-color-surface-muted))]"
            } else if dirty.transient > 0 {
                "tw:contents tw:[--tw-color-card:color-mix(in_oklab,var(--studio-status-live-bg)_55%,var(--studio-color-surface))] tw:[--tw-color-card-subtle:color-mix(in_oklab,var(--studio-status-live-bg)_55%,var(--studio-color-surface-subtle))] tw:[--tw-color-card-muted:color-mix(in_oklab,var(--studio-status-live-bg)_55%,var(--studio-color-surface-muted))]"
            } else {
                "tw:contents tw:[--tw-color-card:var(--studio-color-surface)] tw:[--tw-color-card-subtle:var(--studio-color-surface-subtle)] tw:[--tw-color-card-muted:var(--studio-color-surface-muted)]"
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn NodeTabs(
    tabs: Vec<lpa_studio_core::UiNodeTab>,
    active_index: usize,
    on_select: EventHandler<usize>,
) -> Element {
    rsx! {
        div { class: "tw:flex tw:h-full tw:items-stretch tw:overflow-hidden tw:border-l tw:border-border-muted tw:bg-card-muted", role: "tablist",
            for (index, tab) in tabs.into_iter().enumerate() {
                button {
                    class: if index == active_index {
                        "tw:min-h-full tw:border-0 tw:border-r tw:border-border-muted tw:bg-card-subtle tw:px-4 tw:text-xs tw:font-bold tw:text-strong-foreground"
                    } else {
                        "tw:min-h-full tw:border-0 tw:border-r tw:border-border-muted tw:bg-transparent tw:px-4 tw:text-xs tw:font-bold tw:text-muted-foreground tw:hover:bg-card-subtle tw:hover:text-strong-foreground"
                    },
                    r#type: "button",
                    role: "tab",
                    aria_selected: "{index == active_index}",
                    onclick: move |event| {
                        event.stop_propagation();
                        on_select.call(index);
                    },
                    "{tab.label}"
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dirty(persisted: usize, transient: usize, failed: usize) -> DirtySummary {
        DirtySummary {
            persisted,
            transient,
            failed,
        }
    }

    #[test]
    fn header_tone_rides_the_shared_affordance_merge() {
        use lpa_studio_core::{UiNodeHeader, UiStatus};

        use crate::app::layout::PaneTone;

        let tone = |status: UiStatus, dirty: DirtySummary| {
            let header = UiNodeHeader::new("Clock", "Clock", "/clock")
                .with_status(status)
                .with_dirty(dirty);
            affordance_pane_tone(header.affordance(), header.status.kind)
        };

        // A clean node keeps its runtime status tone on the wash.
        assert_eq!(
            tone(UiStatus::good("Running"), DirtySummary::clean()),
            PaneTone::Good
        );
        // Dirty precedence: failed > unsaved > live.
        assert_eq!(
            tone(UiStatus::good("Running"), dirty(2, 1, 1)),
            PaneTone::Error
        );
        assert_eq!(
            tone(UiStatus::good("Running"), dirty(2, 1, 0)),
            PaneTone::Warning
        );
        assert_eq!(
            tone(UiStatus::good("Running"), dirty(0, 1, 0)),
            PaneTone::Live
        );
        // An error status is never masked by a dirty wash.
        assert_eq!(
            tone(UiStatus::error("Failed"), dirty(0, 1, 0)),
            PaneTone::Error
        );
    }

    #[test]
    fn surface_tint_applies_only_in_full_surface_variant_on_dirty_panes() {
        assert_eq!(
            pane_surface_tint_class(NodeDirtyTint::HeaderOnly, dirty(2, 0, 0)),
            "tw:contents"
        );

        let unsaved = pane_surface_tint_class(NodeDirtyTint::FullSurface, dirty(2, 0, 0));
        assert!(unsaved.contains("--studio-status-warning-bg"));
        let live = pane_surface_tint_class(NodeDirtyTint::FullSurface, dirty(0, 1, 0));
        assert!(live.contains("--studio-status-live-bg"));
        let failed = pane_surface_tint_class(NodeDirtyTint::FullSurface, dirty(1, 1, 1));
        assert!(failed.contains("--studio-status-error-bg"));

        let clean = pane_surface_tint_class(NodeDirtyTint::FullSurface, DirtySummary::clean());
        assert!(!clean.contains("color-mix"));
        assert!(clean.contains("--tw-color-card:var(--studio-color-surface)"));
    }
}
