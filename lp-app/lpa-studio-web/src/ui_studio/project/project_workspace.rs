use dioxus::prelude::*;
use lpa_studio_ux::{
    ProjectEditorView, ProjectNodeStatusTone, ProjectNodeTreeItem, ProjectNodeView,
    ProjectSlotGroupView, ProjectSlotIssueView, ProjectSlotRowView, ProjectSlotValueView, UiAction,
};

use crate::ui_core::MetricGrid;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ProjectWorkspace(
    view: ProjectEditorView,
    running: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    rsx! {
        div { class: "ux-project-workspace",
            ProjectSidebar {
                view: view.clone(),
                running,
                on_action,
            }
            ProjectNodeWorkspace { view }
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
    let sync_issue = view.sync.issue;
    let stats = view.stats;
    let roots = view.tree.roots;

    rsx! {
        div { class: "ux-project-sidebar",
            div { class: "ux-project-tree-panel",
                h3 { "Node tree" }
                if let Some(issue) = sync_issue.as_ref() {
                    div { class: "ux-project-sync-issue",
                        strong { "{issue.message}" }
                        if let Some(detail) = issue.detail.as_ref() {
                            p { "{detail}" }
                        }
                    }
                }
                if roots.is_empty() {
                    p { class: "ux-panel-copy ux-panel-detail", "Project sync has not returned nodes yet." }
                } else {
                    ol { class: "ux-project-tree",
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
            div { class: "ux-project-stats-panel",
                h3 { "Project stats" }
                MetricGrid { metrics: stats }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ProjectNodeWorkspace(view: ProjectEditorView) -> Element {
    let nodes = view.nodes;

    rsx! {
        section { class: "ux-project-nodes",
            if nodes.is_empty() {
                div { class: "ux-project-empty",
                    h3 { "Waiting for project data" }
                    p { "Studio will show node bodies here once the project mirror has synced." }
                }
            } else {
                for node in nodes {
                    ProjectNodeCard {
                        key: "{node.node_id}",
                        node,
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
    let action = item.action.clone();
    let children = item.children;
    let class = if item.focused {
        "ux-project-tree-item ux-project-tree-item-focused"
    } else {
        "ux-project-tree-item"
    };
    let indent = depth * 14;
    let status_class = node_status_class(item.status.tone);
    let status_label = item.status.label;
    let detail = item.status.detail;

    rsx! {
        li {
            button {
                class,
                r#type: "button",
                disabled: running,
                style: "padding-left: {indent}px;",
                onclick: move |_| on_action.call(action.clone()),
                span { class: "ux-project-tree-label", "{item.label}" }
                span { class: "ux-project-tree-kind", "{item.kind}" }
                span { class: "{status_class}", "{status_label}" }
            }
            if let Some(detail) = detail.as_ref() {
                p { class: "ux-project-tree-detail", "{detail}" }
            }
            if !children.is_empty() {
                ol { class: "ux-project-tree-children",
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

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ProjectNodeCard(node: ProjectNodeView) -> Element {
    let has_slots = node.has_slots();
    let has_issues = !node.issues.is_empty();
    let card_class = if node.focused {
        "ux-project-node-card ux-project-node-card-focused"
    } else {
        "ux-project-node-card"
    };
    let status_class = node_status_class(node.status.tone);
    let status_label = node.status.label;
    let status_detail = node.status.detail;

    rsx! {
        article { class: "{card_class}",
            header { class: "ux-project-node-header",
                div { class: "ux-project-node-title",
                    h3 { "{node.label}" }
                    p { "{node.path}" }
                }
                div { class: "ux-project-node-meta",
                    span { class: "ux-project-node-kind", "{node.kind}" }
                    span { class: "{status_class}", "{status_label}" }
                }
            }
            if let Some(detail) = status_detail.as_ref() {
                p { class: "ux-project-node-status-detail", "{detail}" }
            }
            if has_issues {
                ul { class: "ux-project-node-issues",
                    for issue in node.issues {
                        li { "{issue}" }
                    }
                }
            }
            if !node.prominent_slots.is_empty() {
                ProjectSlotSection {
                    title: "Prominent",
                    rows: node.prominent_slots,
                    prominent: true,
                }
            }
            if !node.config_slots.is_empty() {
                ProjectSlotSection {
                    title: "Config",
                    rows: node.config_slots,
                    prominent: false,
                }
            }
            if !node.state_slots.is_empty() {
                ProjectSlotSection {
                    title: "State",
                    rows: node.state_slots,
                    prominent: false,
                }
            }
            if !node.binding_slots.is_empty() {
                ProjectSlotSection {
                    title: "Bindings",
                    rows: node.binding_slots,
                    prominent: false,
                }
            }
            if !has_slots && !has_issues {
                p { class: "ux-panel-copy ux-panel-detail", "No synced slot roots for this node yet." }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ProjectSlotSection(
    title: &'static str,
    rows: Vec<ProjectSlotRowView>,
    prominent: bool,
) -> Element {
    let class = if prominent {
        "ux-project-slot-section ux-project-slot-section-prominent"
    } else {
        "ux-project-slot-section"
    };
    rsx! {
        section { class,
            h4 { "{title}" }
            div { class: "ux-project-slot-rows",
                for row in rows {
                    ProjectSlotRow { row }
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ProjectSlotRow(row: ProjectSlotRowView) -> Element {
    match row {
        ProjectSlotRowView::Value(value) => rsx! {
            ProjectSlotValue { value }
        },
        ProjectSlotRowView::Group(group) => rsx! {
            ProjectSlotGroup { group }
        },
        ProjectSlotRowView::Issue(issue) => rsx! {
            ProjectSlotIssue { issue }
        },
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ProjectSlotValue(value: ProjectSlotValueView) -> Element {
    rsx! {
        div { class: "ux-project-slot-row",
            span { class: "ux-project-slot-label", "{value.label}" }
            span { class: "ux-project-slot-value", "{value.value}" }
            if let Some(detail) = value.detail.as_ref() {
                small { "{detail}" }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ProjectSlotGroup(group: ProjectSlotGroupView) -> Element {
    rsx! {
        div { class: "ux-project-slot-group",
            div { class: "ux-project-slot-group-heading",
                span { "{group.label}" }
                if let Some(detail) = group.detail.as_ref() {
                    small { "{detail}" }
                }
            }
            if group.rows.is_empty() {
                p { class: "ux-project-slot-empty", "empty" }
            } else {
                div { class: "ux-project-slot-group-rows",
                    for row in group.rows {
                        ProjectSlotRow { row }
                    }
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ProjectSlotIssue(issue: ProjectSlotIssueView) -> Element {
    rsx! {
        div { class: "ux-project-slot-row ux-project-slot-row-issue",
            span { class: "ux-project-slot-label", "{issue.label}" }
            span { class: "ux-project-slot-value", "{issue.message}" }
        }
    }
}

fn node_status_class(tone: ProjectNodeStatusTone) -> &'static str {
    match tone {
        ProjectNodeStatusTone::Neutral => "ux-project-node-status ux-project-node-status-neutral",
        ProjectNodeStatusTone::Good => "ux-project-node-status ux-project-node-status-good",
        ProjectNodeStatusTone::Warning => "ux-project-node-status ux-project-node-status-warning",
        ProjectNodeStatusTone::Error => "ux-project-node-status ux-project-node-status-error",
    }
}
