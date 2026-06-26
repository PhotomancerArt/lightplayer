use std::rc::Rc;

use dioxus::prelude::dioxus_core::use_after_render;
use dioxus::prelude::*;
use lpa_studio_core::{ProjectEditorView, ProjectNodeStatusTone, ProjectNodeTreeItem, UiAction};

use crate::app::node::NodePane;
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
    let mut focused_button = use_signal(|| None::<Rc<MountedData>>);
    use_after_render(move || {
        if !focused {
            return;
        }
        let Some(element) = focused_button.read().as_ref().cloned() else {
            return;
        };
        spawn(async move {
            let _ = element.scroll_to(ScrollBehavior::Smooth).await;
        });
    });

    let action = item.action.clone();
    let children = item.children;
    let class = if focused {
        "tw:grid tw:w-full tw:grid-cols-[18px_minmax(0,1fr)_auto] tw:items-center tw:gap-2 tw:scroll-my-4 tw:rounded-sm tw:border tw:border-accent-border tw:bg-status-good-bg tw:px-2 tw:py-1.5 tw:text-left"
    } else {
        "tw:grid tw:w-full tw:grid-cols-[18px_minmax(0,1fr)_auto] tw:items-center tw:gap-2 tw:rounded-sm tw:border tw:border-transparent tw:bg-transparent tw:px-2 tw:py-1.5 tw:text-left tw:hover:bg-card-muted"
    };
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
                onmounted: move |event| {
                    if focused {
                        focused_button.set(Some(event.data()));
                    }
                },
                onclick: move |_| on_action.call(action.clone()),
                span { class: "tw:inline-flex tw:h-4 tw:w-4 tw:items-center tw:justify-center tw:text-subtle-foreground",
                    StudioIcon {
                        name: StudioIconName::NodeTreeItem,
                        size: 14,
                    }
                }
                span { class: "tw:min-w-0 tw:overflow-hidden tw:text-ellipsis tw:whitespace-nowrap tw:text-sm tw:text-soft-foreground", "{label}" }
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
