use dioxus::prelude::*;
use lpa_studio_core::{
    DirtySummary, ProjectEditorView, ProjectNodeStatusTone, ProjectNodeTreeItem, UiAction,
};

use crate::app::node::NodePane;
use crate::app::project::ProjectHeader;
use crate::base::{StudioIcon, StudioIconName};
use crate::core::MetricGrid;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ProjectWorkspace(
    view: ProjectEditorView,
    running: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    rsx! {
        div { class: "tw:grid tw:grid-cols-[minmax(170px,240px)_minmax(0,1fr)] tw:gap-3.5 tw:max-[640px]:grid-cols-1",
            ProjectSidebar {
                view: view.clone(),
                running,
                on_action,
            }
            ProjectNodeWorkspace { view, on_action }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ProjectSidebar(
    view: ProjectEditorView,
    running: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    let project_id = view.project_id.clone();
    let dirty = view.dirty;
    let overlay_revision = view.sync.overlay_revision;
    let edits_in_flight = view.edits_in_flight;
    let header_actions = view.header_actions.clone();
    let sync_issue = view.sync.issue;
    let stats = view.stats;
    let roots = view.tree.roots;

    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:content-start tw:gap-3.5",
            ProjectHeader {
                project_id,
                dirty,
                overlay_revision,
                edits_in_flight,
                actions: header_actions,
                on_action,
            }
            div { class: "tw:rounded-md tw:border tw:border-border tw:bg-card tw:p-4",
                h3 { class: "tw:m-0 tw:mb-3 tw:text-xs tw:font-bold tw:uppercase tw:text-heading", "Node tree" }
                if let Some(issue) = sync_issue.as_ref() {
                    div { class: "tw:mb-3 tw:grid tw:gap-1 tw:rounded-sm tw:border tw:border-status-error-border tw:bg-status-error-bg tw:p-3 tw:text-sm tw:text-status-error-foreground",
                        strong { "{issue.message}" }
                        if let Some(detail) = issue.detail.as_ref() {
                            p { class: "tw:m-0 tw:text-xs tw:text-status-error-foreground", "{detail}" }
                        }
                    }
                }
                if roots.is_empty() {
                    p { class: "tw:m-0 tw:text-sm tw:text-subtle-foreground", "Project sync has not returned nodes yet." }
                } else {
                    ol { class: "tw:m-0 tw:grid tw:list-none tw:gap-1 tw:p-0",
                        for item in roots {
                            ProjectNodeTreeItemView {
                                key: "{item.node_id}",
                                item,
                                depth: 0,
                                running,
                                on_action,
                            }
                        }
                    }
                }
            }
            div { class: "tw:rounded-md tw:border tw:border-border tw:bg-card tw:p-4",
                h3 { class: "tw:m-0 tw:mb-3 tw:text-xs tw:font-bold tw:uppercase tw:text-heading", "Project stats" }
                MetricGrid { metrics: stats }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ProjectNodeWorkspace(view: ProjectEditorView, on_action: EventHandler<UiAction>) -> Element {
    let nodes = view.nodes;

    rsx! {
        section { class: "tw:grid tw:min-w-0 tw:content-start tw:gap-3.5",
            if nodes.is_empty() {
                div { class: "tw:grid tw:min-w-0 tw:gap-2 tw:rounded-md tw:border tw:border-border-subtle tw:bg-card-subtle tw:p-4",
                    h3 { class: "tw:m-0 tw:text-base tw:text-strong-foreground", "Waiting for project data" }
                    p { class: "tw:m-0 tw:text-sm tw:text-muted-foreground", "Studio will show node bodies here once the project mirror has synced." }
                }
            } else {
                for node in nodes {
                    NodePane {
                        key: "{node.node_id}",
                        view: node,
                        on_action,
                    }
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ProjectNodeTreeItemView(
    item: ProjectNodeTreeItem,
    depth: usize,
    running: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    let focused = item.focused;
    let action = item.action.clone();
    let children = item.children;
    let dirty = item.dirty;
    let class = tree_item_row_class(focused, dirty);
    let indent = depth * 14;
    let status_class = node_status_class(item.status.tone);
    let status_label = item.status.label;
    let detail = item.status.detail;
    let label = item.label;
    let kind_label = item.kind;

    rsx! {
        li {
            button {
                class,
                r#type: "button",
                disabled: running,
                style: "padding-left: {indent}px;",
                title: "{kind_label}",
                onclick: move |_| on_action.call(action.clone()),
                span { class: "tw:inline-flex tw:h-4 tw:w-4 tw:items-center tw:justify-center tw:text-subtle-foreground",
                    StudioIcon {
                        name: StudioIconName::NodeTreeItem,
                        size: 14,
                    }
                }
                span { class: "tw:min-w-0 tw:overflow-hidden tw:text-ellipsis tw:whitespace-nowrap tw:text-sm tw:text-soft-foreground", "{label}" }
                if !dirty.is_clean() {
                    span { class: tree_item_badge_class(dirty), title: tree_item_badge_title(dirty), "{dirty.total()}" }
                }
                span { class: "{status_class}", "{status_label}" }
            }
            if let Some(detail) = detail.as_ref() {
                p { class: "tw:m-0 tw:pl-2 tw:text-xs tw:text-subtle-foreground", "{detail}" }
            }
            if !children.is_empty() {
                ol { class: "tw:m-0 tw:grid tw:list-none tw:gap-1 tw:p-0",
                    for child in children {
                        ProjectNodeTreeItemView {
                            key: "{child.node_id}",
                            item: child,
                            depth: depth + 1,
                            running,
                            on_action,
                        }
                    }
                }
            }
        }
    }
}

/// Row class for one tree item: the focused row keeps its accent treatment;
/// an unfocused dirty row wears a subtle left-edge tint in its dominant dirty
/// color (Q2: yellow = unsaved in subtree, blue = live-only, red = failed).
fn tree_item_row_class(focused: bool, dirty: DirtySummary) -> &'static str {
    if focused {
        return "tw:grid tw:w-full tw:grid-cols-[18px_minmax(0,1fr)_auto_auto] tw:items-center tw:gap-2 tw:rounded-sm tw:border tw:border-accent-border tw:bg-status-good-bg tw:px-2 tw:py-1.5 tw:text-left";
    }
    if dirty.failed > 0 {
        "tw:grid tw:w-full tw:grid-cols-[18px_minmax(0,1fr)_auto_auto] tw:items-center tw:gap-2 tw:rounded-sm tw:border tw:border-transparent tw:bg-[linear-gradient(90deg,var(--studio-status-error-bg),transparent_70%)] tw:px-2 tw:py-1.5 tw:text-left tw:hover:bg-card-muted"
    } else if dirty.persisted > 0 {
        "tw:grid tw:w-full tw:grid-cols-[18px_minmax(0,1fr)_auto_auto] tw:items-center tw:gap-2 tw:rounded-sm tw:border tw:border-transparent tw:bg-[linear-gradient(90deg,var(--studio-status-warning-bg),transparent_70%)] tw:px-2 tw:py-1.5 tw:text-left tw:hover:bg-card-muted"
    } else if dirty.transient > 0 {
        "tw:grid tw:w-full tw:grid-cols-[18px_minmax(0,1fr)_auto_auto] tw:items-center tw:gap-2 tw:rounded-sm tw:border tw:border-transparent tw:bg-[linear-gradient(90deg,var(--studio-status-live-bg),transparent_70%)] tw:px-2 tw:py-1.5 tw:text-left tw:hover:bg-card-muted"
    } else {
        "tw:grid tw:w-full tw:grid-cols-[18px_minmax(0,1fr)_auto_auto] tw:items-center tw:gap-2 tw:rounded-sm tw:border tw:border-transparent tw:bg-transparent tw:px-2 tw:py-1.5 tw:text-left tw:hover:bg-card-muted"
    }
}

/// Small count badge for a dirty tree item, toned by the dominant dirty
/// bucket (failed > unsaved > live, matching the pane-chip precedence).
fn tree_item_badge_class(dirty: DirtySummary) -> &'static str {
    if dirty.failed > 0 {
        "tw:shrink-0 tw:rounded-pill tw:border tw:border-status-error-border tw:bg-status-error-bg tw:px-1.5 tw:py-0.5 tw:text-[0.65rem] tw:font-bold tw:leading-none tw:text-status-error-foreground"
    } else if dirty.persisted > 0 {
        "tw:shrink-0 tw:rounded-pill tw:border tw:border-status-warning-border tw:bg-status-warning-bg tw:px-1.5 tw:py-0.5 tw:text-[0.65rem] tw:font-bold tw:leading-none tw:text-status-warning-foreground"
    } else {
        "tw:shrink-0 tw:rounded-pill tw:border tw:border-status-live-border tw:bg-status-live-bg tw:px-1.5 tw:py-0.5 tw:text-[0.65rem] tw:font-bold tw:leading-none tw:text-status-live-foreground"
    }
}

/// Tooltip for the tree-item dirty badge: the per-bucket breakdown behind
/// the aggregate count.
fn tree_item_badge_title(dirty: DirtySummary) -> String {
    let mut parts = Vec::new();
    if dirty.persisted > 0 {
        parts.push(format!("{} unsaved", dirty.persisted));
    }
    if dirty.transient > 0 {
        parts.push(format!("{} live", dirty.transient));
    }
    if dirty.failed > 0 {
        parts.push(format!("{} failed", dirty.failed));
    }
    format!("Edits in this subtree: {}", parts.join(", "))
}

fn node_status_class(tone: ProjectNodeStatusTone) -> &'static str {
    match tone {
        ProjectNodeStatusTone::Neutral => {
            "tw:rounded-pill tw:border tw:border-status-neutral-border tw:bg-status-neutral-bg tw:px-2 tw:py-1 tw:text-xs tw:font-bold tw:text-status-neutral-foreground"
        }
        ProjectNodeStatusTone::Good => {
            "tw:rounded-pill tw:border tw:border-status-good-border tw:bg-status-good-bg tw:px-2 tw:py-1 tw:text-xs tw:font-bold tw:text-status-good-foreground"
        }
        ProjectNodeStatusTone::Warning => {
            "tw:rounded-pill tw:border tw:border-status-warning-border tw:bg-status-warning-bg tw:px-2 tw:py-1 tw:text-xs tw:font-bold tw:text-status-warning-foreground"
        }
        ProjectNodeStatusTone::Error => {
            "tw:rounded-pill tw:border tw:border-status-error-border tw:bg-status-error-bg tw:px-2 tw:py-1 tw:text-xs tw:font-bold tw:text-status-error-foreground"
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
    fn clean_row_keeps_the_plain_background() {
        let class = tree_item_row_class(false, DirtySummary::clean());
        assert!(class.contains("tw:bg-transparent"));
        assert!(!class.contains("linear-gradient"));
    }

    #[test]
    fn dirty_row_tint_follows_the_dominant_bucket() {
        assert!(tree_item_row_class(false, dirty(2, 0, 0)).contains("--studio-status-warning-bg"));
        assert!(tree_item_row_class(false, dirty(0, 1, 0)).contains("--studio-status-live-bg"));
        assert!(tree_item_row_class(false, dirty(1, 1, 1)).contains("--studio-status-error-bg"));
    }

    #[test]
    fn focused_row_keeps_its_accent_treatment_even_when_dirty() {
        let class = tree_item_row_class(true, dirty(2, 0, 0));
        assert!(class.contains("tw:border-accent-border"));
        assert!(!class.contains("linear-gradient"));
    }

    #[test]
    fn badge_tone_follows_the_dominant_bucket_and_title_breaks_down_counts() {
        assert!(tree_item_badge_class(dirty(2, 1, 0)).contains("status-warning"));
        assert!(tree_item_badge_class(dirty(0, 1, 0)).contains("status-live"));
        assert!(tree_item_badge_class(dirty(2, 1, 1)).contains("status-error"));
        assert_eq!(
            tree_item_badge_title(dirty(2, 1, 0)),
            "Edits in this subtree: 2 unsaved, 1 live"
        );
        assert_eq!(
            tree_item_badge_title(dirty(0, 0, 3)),
            "Edits in this subtree: 3 failed"
        );
    }
}
