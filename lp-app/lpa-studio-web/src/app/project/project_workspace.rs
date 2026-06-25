use dioxus::prelude::*;
use lpa_studio_core::{
    ProjectEditorView, ProjectNodeStatusTone, ProjectNodeTreeItem, ProjectNodeView,
    ProjectSlotGroupView, ProjectSlotIssueView, ProjectSlotRowView, ProjectSlotValueView, UiAction,
};

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
        div { class: "tw:grid tw:min-w-0 tw:content-start tw:gap-3.5",
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
pub fn ProjectNodeWorkspace(view: ProjectEditorView) -> Element {
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
        "tw:grid tw:w-full tw:grid-cols-[minmax(0,1fr)_auto_auto] tw:items-center tw:gap-2 tw:rounded-sm tw:border tw:border-accent-border tw:bg-status-good-bg tw:px-2 tw:py-1.5 tw:text-left"
    } else {
        "tw:grid tw:w-full tw:grid-cols-[minmax(0,1fr)_auto_auto] tw:items-center tw:gap-2 tw:rounded-sm tw:border tw:border-transparent tw:bg-transparent tw:px-2 tw:py-1.5 tw:text-left tw:hover:bg-card-muted"
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
                span { class: "tw:min-w-0 tw:overflow-hidden tw:text-ellipsis tw:whitespace-nowrap tw:text-sm tw:text-soft-foreground", "{item.label}" }
                span { class: "tw:text-xs tw:text-subtle-foreground", "{item.kind}" }
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

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ProjectNodeCard(node: ProjectNodeView) -> Element {
    let has_slots = node.has_slots();
    let has_issues = !node.issues.is_empty();
    let card_class = if node.focused {
        "tw:grid tw:min-w-0 tw:gap-4 tw:rounded-md tw:border tw:border-accent-border tw:bg-card tw:p-4"
    } else {
        "tw:grid tw:min-w-0 tw:gap-4 tw:rounded-md tw:border tw:border-border tw:bg-card tw:p-4"
    };
    let status_class = node_status_class(node.status.tone);
    let status_label = node.status.label;
    let status_detail = node.status.detail;

    rsx! {
        article { class: "{card_class}",
            header { class: "tw:flex tw:flex-wrap tw:items-start tw:justify-between tw:gap-3",
                div { class: "tw:grid tw:min-w-0 tw:gap-1",
                    h3 { class: "tw:m-0 tw:text-base tw:font-bold tw:text-strong-foreground", "{node.label}" }
                    p { class: "tw:m-0 tw:font-mono tw:text-xs tw:text-subtle-foreground tw:break-words", "{node.path}" }
                }
                div { class: "tw:flex tw:flex-wrap tw:items-center tw:gap-2",
                    span { class: "tw:rounded-xs tw:border tw:border-border-subtle tw:bg-card-muted tw:px-2 tw:py-1 tw:text-xs tw:text-muted-foreground", "{node.kind}" }
                    span { class: "{status_class}", "{status_label}" }
                }
            }
            if let Some(detail) = status_detail.as_ref() {
                p { class: "tw:m-0 tw:text-sm tw:text-subtle-foreground", "{detail}" }
            }
            if has_issues {
                ul { class: "tw:m-0 tw:grid tw:list-none tw:gap-1 tw:rounded-sm tw:border tw:border-status-error-border tw:bg-status-error-bg tw:p-3",
                    for issue in node.issues {
                        li { class: "tw:text-sm tw:text-status-error-foreground", "{issue}" }
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
                p { class: "tw:m-0 tw:text-sm tw:text-subtle-foreground", "No synced slot roots for this node yet." }
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
        "tw:grid tw:min-w-0 tw:gap-2"
    } else {
        "tw:grid tw:min-w-0 tw:gap-2"
    };
    rsx! {
        section { class,
            h4 { class: "tw:m-0 tw:text-xs tw:font-bold tw:uppercase tw:text-heading", "{title}" }
            div { class: "tw:grid tw:min-w-0 tw:gap-2",
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
        div { class: "tw:grid tw:grid-cols-[minmax(110px,0.35fr)_minmax(0,1fr)] tw:gap-2 tw:rounded-sm tw:border tw:border-border-subtle tw:bg-card-muted tw:p-2",
            span { class: "tw:text-xs tw:text-subtle-foreground", "{value.label}" }
            span { class: "tw:min-w-0 tw:text-right tw:font-mono tw:text-xs tw:text-muted-foreground tw:break-words", "{value.value}" }
            if let Some(detail) = value.detail.as_ref() {
                small { class: "tw:col-span-2 tw:text-xs tw:text-subtle-foreground", "{detail}" }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ProjectSlotGroup(group: ProjectSlotGroupView) -> Element {
    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-2 tw:rounded-sm tw:border tw:border-border-subtle tw:bg-card-muted tw:p-2",
            div { class: "tw:flex tw:flex-wrap tw:items-baseline tw:justify-between tw:gap-2",
                span { "{group.label}" }
                if let Some(detail) = group.detail.as_ref() {
                    small { class: "tw:text-xs tw:text-subtle-foreground", "{detail}" }
                }
            }
            if group.rows.is_empty() {
                p { class: "tw:m-0 tw:text-sm tw:text-muted-foreground", "empty" }
            } else {
                div { class: "tw:grid tw:min-w-0 tw:gap-2 tw:border-l tw:border-border-muted tw:pl-2",
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
        div { class: "tw:grid tw:grid-cols-[minmax(110px,0.35fr)_minmax(0,1fr)] tw:gap-2 tw:rounded-sm tw:border tw:border-status-error-border tw:bg-status-error-bg tw:p-2",
            span { class: "tw:text-xs tw:text-status-error-foreground", "{issue.label}" }
            span { class: "tw:min-w-0 tw:text-right tw:text-xs tw:text-status-error-foreground tw:break-words", "{issue.message}" }
        }
    }
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
