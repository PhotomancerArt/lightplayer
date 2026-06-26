use dioxus::prelude::*;
use lpa_studio_core::core::status::UiStatusKind;
use lpa_studio_core::{UiAction, UiNodeSection, UiNodeTabBody, UiNodeView, UiSlotRecord};

use crate::app::node::{
    NodeChildren, NodeHeader, ProducedProducts, ProducedValues, SlotRecordEditor,
};
use crate::base::{StudioIcon, StudioIconName};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn NodePane(
    view: UiNodeView,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let mut active_tab = use_signal(|| 0_usize);
    let mut collapsed = use_signal(|| view.collapsed);
    let focus_action = view.action.clone();
    let focus_handler = on_action;
    let focused_class = if view.focused {
        "tw:border-accent-border"
    } else {
        "tw:border-border"
    };
    let article_class = format!(
        "tw:grid tw:min-w-0 tw:overflow-hidden tw:rounded-md tw:border {focused_class} tw:bg-card tw:p-4"
    );
    let active_index = active_tab().min(view.tabs.len().saturating_sub(1));
    let active_body = view.tabs.get(active_index).map(|tab| tab.body.clone());
    let header_class = node_header_class(view.header.status.kind, collapsed());

    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-3",
            article {
                class: "{article_class}",
                onclick: move |_| {
                    if let (Some(action), Some(handler)) = (focus_action.clone(), focus_handler) {
                        handler.call(action);
                    }
                },
                header { class: "{header_class}",
                    button {
                        class: "tw:inline-flex tw:h-full tw:min-h-[46px] tw:w-[34px] tw:items-center tw:justify-center tw:border-0 tw:border-r tw:border-border-muted tw:bg-transparent tw:p-0 tw:text-subtle-foreground tw:hover:bg-card-subtle/60",
                        r#type: "button",
                        aria_label: if collapsed() { "Expand node" } else { "Collapse node" },
                        title: if collapsed() { "Expand node" } else { "Collapse node" },
                        onclick: move |event| {
                            event.stop_propagation();
                            collapsed.set(!collapsed());
                        },
                        StudioIcon {
                            name: if collapsed() { StudioIconName::Collapsed } else { StudioIconName::Expanded },
                            size: 14,
                        }
                    }
                    NodeHeader { header: view.header.clone() }
                    if view.tabs.len() > 1 {
                        NodeTabs {
                            tabs: view.tabs.clone(),
                            active_index,
                            on_select: move |index| active_tab.set(index),
                        }
                    }
                }
                if !collapsed() {
                    if !view.issues.is_empty() {
                        ul { class: "tw:m-0 tw:grid tw:list-none tw:gap-1 tw:rounded-sm tw:border tw:border-status-error-border tw:bg-status-error-bg tw:p-3",
                            for issue in view.issues.clone() {
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
                }
            }
            if !collapsed() && !view.children.is_empty() {
                NodeChildren {
                    items: view.children.clone(),
                    on_action,
                }
            }
        }
    }
}

fn node_header_class(kind: UiStatusKind, collapsed: bool) -> String {
    let shape_class = if collapsed {
        "tw:-mb-4 tw:rounded-md"
    } else {
        "tw:rounded-t-md tw:border-b tw:border-border-muted"
    };
    let status_class = match kind {
        UiStatusKind::Neutral => {
            "tw:bg-[linear-gradient(90deg,var(--studio-status-neutral-bg),transparent_62%)]"
        }
        UiStatusKind::Working => {
            "tw:bg-[linear-gradient(90deg,var(--studio-status-working-bg),transparent_62%)]"
        }
        UiStatusKind::Good => {
            "tw:bg-[linear-gradient(90deg,var(--studio-status-good-bg),transparent_62%)]"
        }
        UiStatusKind::Warning => {
            "tw:bg-[linear-gradient(90deg,var(--studio-status-warning-bg),transparent_62%)]"
        }
        UiStatusKind::Error => {
            "tw:bg-[linear-gradient(90deg,var(--studio-status-error-bg),transparent_66%)]"
        }
    };

    format!(
        "tw:-mx-4 tw:-mt-4 tw:grid tw:min-h-[46px] tw:min-w-0 tw:grid-cols-[34px_minmax(0,1fr)_auto] tw:items-stretch tw:overflow-hidden {shape_class} tw:bg-card-subtle {status_class}"
    )
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn NodeSection(section: UiNodeSection, #[props(default = false)] first: bool) -> Element {
    match section {
        UiNodeSection::ProducedProducts(products) => rsx! {
            section { class: section_class("tw:bg-card tw:p-0", first),
                ProducedProducts { products }
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
                }
            }
        },
        UiNodeSection::AssetSlots(assets) => rsx! {
            section { class: section_class("tw:bg-card tw:p-0", first),
                SlotRecordEditor {
                    record: UiSlotRecord::new(assets),
                }
            }
        },
        UiNodeSection::Children(children) => rsx! {
            section { class: section_class("tw:bg-card tw:px-4 tw:py-4", first),
                NodeChildren { items: children, on_action: None }
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
