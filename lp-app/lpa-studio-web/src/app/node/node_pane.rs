use dioxus::prelude::*;
use lpa_studio_core::{
    DirtySummary, UiAction, UiNodeSection, UiNodeTabBody, UiNodeView, UiSlotRecord,
};

use crate::app::affordance::affordance_pane_tone;
use crate::app::layout::{PaneChrome, PaneCollapse, StudioPane};
use crate::app::node::{
    AssetEditorTab, NodeChildren, NodeDetailPopover, ProducedProducts, ProducedValues,
    SlotRecordEditor,
};
use crate::base::{StudioIcon, StudioIconName};

/// Which node tab is active. `Editor` addresses the asset editor tab by
/// role rather than position, so the "open in editor" affordance on asset
/// slot rows needs no index bookkeeping; it resolves to the tab's current
/// position at render (falling back to the main tab when no editor exists).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NodePaneTab {
    Index(usize),
    Editor,
}

/// Context provided by every [`NodePane`] so descendants rendered inside its
/// body (currently the asset slot row's "open in editor" affordance) can
/// switch the pane's active tab without prop-threading through the generic
/// slot editors.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct NodePaneActiveTab(pub(crate) Signal<NodePaneTab>);

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
    #[props(default)] dirty_tint: NodeDirtyTint,
    /// Tab active on first render (stories; defaults to the main tab).
    #[props(default)]
    initial_tab: Option<usize>,
    /// Seed the editor-local "modified" chrome on first render (stories
    /// only — a live editor derives it from typing).
    #[props(default = false)]
    initially_editor_modified: bool,
) -> Element {
    let mut active_tab = use_signal(|| NodePaneTab::Index(initial_tab.unwrap_or(0)));
    use_context_provider(|| NodePaneActiveTab(active_tab));
    let mut collapsed = use_signal(|| view.collapsed);
    // Editor-local state for the asset editor tab (see `AssetEditorTab`'s
    // module docs): the pane owns it because the Apply header action needs
    // the current text and modified flag while rendering the header.
    let editor_text = use_signal(String::new);
    let editor_modified = use_signal(|| initially_editor_modified);
    let editor_index = view
        .tabs
        .iter()
        .position(|tab| matches!(tab.body, UiNodeTabBody::AssetEditor(_)));
    let editor_tab = view.tabs.iter().find_map(|tab| match &tab.body {
        UiNodeTabBody::AssetEditor(editor) => Some(editor.clone()),
        _ => None,
    });
    let active_index = match active_tab() {
        NodePaneTab::Index(index) => index.min(view.tabs.len().saturating_sub(1)),
        NodePaneTab::Editor => editor_index.unwrap_or(0),
    };
    let editor_active = editor_index == Some(active_index);
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
    let focus_action = view.action.clone();
    let issues = view.issues.clone();
    // The Apply action joins the header's controller-produced actions only
    // while the editor tab is active; it is assembled through the DTO's
    // helper so icon/label/enablement rules stay in core — the web only
    // threads in the two editor-local inputs (text, modified).
    let mut header_actions = view.header_actions.clone();
    if editor_active && let Some(editor) = &editor_tab {
        header_actions.push(editor.apply_pane_action(&editor_text(), editor_modified()));
    }

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
                                on_action,
                            }
                        }
                    },
                    title,
                    chrome,
                    actions: header_actions,
                    on_action,
                    trailing: rsx! {
                        if tabs.len() > 1 {
                            NodeTabs {
                                tabs: tabs.clone(),
                                active_index,
                                on_select: move |index| active_tab.set(NodePaneTab::Index(index)),
                            }
                        }
                    },
                    detail: rsx! {
                        NodeDetailPopover { header }
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
                            // Rendered by the persistent editor block below
                            // (kept mounted across tab switches so unapplied
                            // editor text survives).
                            Some(UiNodeTabBody::AssetEditor(_)) => rsx! {},
                            None => rsx! {
                                p { class: "tw:m-0 tw:text-sm tw:text-subtle-foreground", "No node tabs are available." }
                            },
                        }
                        // The editor tab body mounts once and hides when
                        // inactive: the CodeMirror leaf owns unapplied user
                        // text, and unmounting it on a tab switch would
                        // destroy that text.
                        if let Some(editor) = editor_tab {
                            div { class: if editor_active { "tw:-mx-4 tw:-mb-4 tw:grid tw:min-w-0" } else { "tw:hidden" },
                                AssetEditorTab {
                                    tab: editor,
                                    text: editor_text,
                                    modified: editor_modified,
                                    on_action,
                                }
                            }
                        }
                    },
                }
            }
            if !collapsed() && !view.children.is_empty() {
                NodeChildren {
                    items: view.children.clone(),
                    on_action,
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
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let (class, icon, label) = if focused {
        (
            "tw:inline-flex tw:h-8 tw:w-8 tw:shrink-0 tw:items-center tw:justify-center tw:rounded-full tw:border tw:border-selection-border tw:bg-transparent tw:p-0 tw:text-strong-foreground",
            StudioIconName::NodeSelected,
            "Node is selected; probes follow this node",
        )
    } else {
        (
            "tw:inline-flex tw:h-8 tw:w-8 tw:shrink-0 tw:items-center tw:justify-center tw:rounded-full tw:border tw:border-border-subtle tw:bg-transparent tw:p-0 tw:text-subtle-foreground tw:hover:border-accent-border tw:hover:text-accent",
            StudioIconName::NodeSelect,
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
                NodeChildren { items: children, on_action, dirty_tint }
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
