use dioxus::prelude::*;
use lpa_studio_core::{
    UiNodeDirtyState, UiNodeSection, UiNodeTabBody, UiNodeView, UiProducedBindings,
};

use crate::app::node::{
    ConsumedAssets, ConsumedSlots, NodeChildren, NodeHeader, ProducedProducts, ProducedValues,
};
use crate::base::{StudioIcon, StudioIconName};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn NodePane(view: UiNodeView) -> Element {
    let mut active_tab = use_signal(|| 0_usize);
    let mut collapsed = use_signal(|| view.collapsed);
    let focused_class = if view.focused {
        "tw:border-accent-border"
    } else {
        "tw:border-border"
    };
    let active_index = active_tab().min(view.tabs.len().saturating_sub(1));
    let active_body = view.tabs.get(active_index).map(|tab| tab.body.clone());

    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-3",
            article { class: "tw:grid tw:min-w-0 tw:gap-3 tw:rounded-md tw:border {focused_class} tw:bg-card tw:p-4",
                div { class: "tw:grid tw:grid-cols-[auto_minmax(0,1fr)] tw:gap-2",
                    button {
                        class: "tw:mt-0.5 tw:inline-flex tw:h-7 tw:w-7 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-border-subtle tw:bg-card-muted tw:text-muted-foreground tw:hover:border-border-strong",
                        r#type: "button",
                        aria_label: if collapsed() { "Expand node" } else { "Collapse node" },
                        onclick: move |_| collapsed.set(!collapsed()),
                        StudioIcon {
                            name: if collapsed() { StudioIconName::Collapsed } else { StudioIconName::Expanded },
                            size: 15,
                        }
                    }
                    NodeHeader { header: view.header.clone() }
                }
                if !collapsed() {
                    if !view.issues.is_empty() {
                        ul { class: "tw:m-0 tw:grid tw:list-none tw:gap-1 tw:rounded-sm tw:border tw:border-status-error-border tw:bg-status-error-bg tw:p-3",
                            for issue in view.issues.clone() {
                                li { class: "tw:text-sm tw:text-status-error-foreground", "{issue}" }
                            }
                        }
                    }
                    if view.tabs.len() > 1 {
                        NodeTabs {
                            tabs: view.tabs.clone(),
                            active_index,
                            on_select: move |index| active_tab.set(index),
                        }
                    }
                    match active_body {
                        Some(UiNodeTabBody::Sections(sections)) => rsx! {
                            div { class: "tw:grid tw:min-w-0 tw:gap-4",
                                for section in sections {
                                    NodeSection { section }
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
                NodeChildren { items: view.children.clone() }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn NodeSection(section: UiNodeSection) -> Element {
    match section {
        UiNodeSection::ProducedProducts(products) => rsx! {
            ProducedProducts { products }
        },
        UiNodeSection::ProducedValues(values) => rsx! {
            ProducedValues { values }
        },
        UiNodeSection::ConsumedValues(slots) => rsx! {
            ConsumedSlots { slots }
        },
        UiNodeSection::ConsumedAssets(assets) => rsx! {
            ConsumedAssets { assets }
        },
        UiNodeSection::Children(children) => rsx! {
            NodeChildren { items: children }
        },
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
        div { class: "tw:flex tw:flex-wrap tw:gap-1 tw:border-b tw:border-border-muted", role: "tablist",
            for (index, tab) in tabs.into_iter().enumerate() {
                button {
                    class: if index == active_index {
                        "tw:border-b-2 tw:border-accent tw:bg-transparent tw:px-3 tw:pb-2 tw:pt-1 tw:text-xs tw:font-bold tw:text-strong-foreground"
                    } else {
                        "tw:border-b-2 tw:border-transparent tw:bg-transparent tw:px-3 tw:pb-2 tw:pt-1 tw:text-xs tw:font-bold tw:text-muted-foreground tw:hover:text-strong-foreground"
                    },
                    r#type: "button",
                    role: "tab",
                    aria_selected: "{index == active_index}",
                    onclick: move |_| on_select.call(index),
                    "{tab.label}"
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn DirtyMark(dirty: UiNodeDirtyState) -> Element {
    if !dirty.needs_attention() {
        return rsx! {};
    }

    let (label, class) = match dirty {
        UiNodeDirtyState::Clean => ("clean", ""),
        UiNodeDirtyState::Dirty => (
            "edited",
            "tw:rounded-xs tw:border tw:border-status-warning-border tw:bg-status-warning-bg tw:px-1.5 tw:py-0.5 tw:text-[0.65rem] tw:font-bold tw:uppercase tw:text-status-warning-foreground",
        ),
        UiNodeDirtyState::Saving => (
            "saving",
            "tw:rounded-xs tw:border tw:border-status-working-border tw:bg-status-working-bg tw:px-1.5 tw:py-0.5 tw:text-[0.65rem] tw:font-bold tw:uppercase tw:text-status-working-foreground",
        ),
        UiNodeDirtyState::Error => (
            "error",
            "tw:rounded-xs tw:border tw:border-status-error-border tw:bg-status-error-bg tw:px-1.5 tw:py-0.5 tw:text-[0.65rem] tw:font-bold tw:uppercase tw:text-status-error-foreground",
        ),
    };

    rsx! {
        span { class, "{label}" }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ProducedBindingMark(label: String, bindings: UiProducedBindings) -> Element {
    let class = if bindings.has_any() {
        "tw:inline-flex tw:h-5 tw:w-5 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-accent-border tw:bg-accent-bg tw:text-accent"
    } else {
        "tw:inline-flex tw:h-5 tw:w-5 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-border-subtle tw:bg-page tw:text-subtle-foreground"
    };
    let title = if bindings.has_any() {
        format!("{label} has bindings")
    } else {
        format!("{label} has no bindings")
    };

    rsx! {
        span { class, title,
            StudioIcon { name: StudioIconName::BoundValue, size: 12 }
        }
    }
}
