use dioxus::prelude::*;
use lpa_studio_core::core::status::UiStatusKind;
use lpa_studio_core::{
    UiBindingEndpoint, UiNodeDirtyState, UiNodeHeader, UiNodeSection, UiNodeTabBody, UiNodeView,
    UiProducedBindings,
};

use crate::app::node::{
    ConsumedAssets, ConsumedSlots, NodeChildren, NodeHeader, ProducedProducts, ProducedValues,
};
use crate::base::{IconMenuButton, IconMenuTone, PopoverPlacement, StudioIcon, StudioIconName};

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
                header { class: "tw:-mx-4 tw:-mt-4 tw:grid tw:min-h-[46px] tw:min-w-0 tw:grid-cols-[34px_38px_minmax(0,1fr)_auto] tw:items-stretch tw:overflow-hidden tw:rounded-t-md tw:border-b tw:border-border-muted",
                    button {
                        class: "tw:inline-flex tw:h-full tw:min-h-[46px] tw:w-[34px] tw:items-center tw:justify-center tw:border-0 tw:border-r tw:border-border-muted tw:bg-card-muted tw:p-0 tw:text-subtle-foreground tw:hover:bg-card-subtle",
                        r#type: "button",
                        aria_label: if collapsed() { "Expand node" } else { "Collapse node" },
                        title: if collapsed() { "Expand node" } else { "Collapse node" },
                        onclick: move |_| collapsed.set(!collapsed()),
                        StudioIcon {
                            name: if collapsed() { StudioIconName::Collapsed } else { StudioIconName::Expanded },
                            size: 14,
                        }
                    }
                    NodeStatusMenu { header: view.header.clone() }
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
                    onclick: move |_| on_select.call(index),
                    "{tab.label}"
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn NodeStatusMenu(header: UiNodeHeader) -> Element {
    let (tone, icon) = status_menu_tone(header.status.kind);
    let label = format!("{} status details", header.title);

    rsx! {
        span { class: "tw:flex tw:h-full tw:min-h-[46px] tw:w-[38px] tw:items-center tw:justify-center tw:border-r tw:border-border-muted tw:bg-card-muted",
            IconMenuButton {
                icon,
                icon_size: 14,
                label,
                title: format!("{} status details", header.title),
                tone,
                placement: PopoverPlacement::BottomStart,
                active: true,
                div { class: "tw:grid tw:gap-1",
                    span { class: "tw:text-[0.68rem] tw:font-bold tw:uppercase tw:text-heading", "node status" }
                    strong { class: "tw:text-sm tw:text-strong-foreground", "{header.title}" }
                    div { class: "tw:flex tw:flex-wrap tw:gap-2 tw:text-xs tw:text-subtle-foreground",
                        span { "{header.kind}" }
                        span { "{header.status.label}" }
                        if let Some(summary) = header.summary.as_ref() {
                            span { "{summary}" }
                        }
                    }
                    if let Some(source) = header.source.as_ref() {
                        code { class: "tw:font-mono tw:text-xs tw:text-muted-foreground tw:break-words", "{source}" }
                    }
                }
                if let Some(detail) = header.detail.as_ref() {
                    p { class: "tw:m-0 tw:rounded-xs tw:border tw:border-border-subtle tw:bg-page tw:p-2 tw:text-xs tw:leading-normal tw:text-muted-foreground tw:break-words", "{detail}" }
                }
            }
        }
    }
}

fn status_menu_tone(kind: UiStatusKind) -> (IconMenuTone, StudioIconName) {
    match kind {
        UiStatusKind::Neutral => (IconMenuTone::Neutral, StudioIconName::StatusIdle),
        UiStatusKind::Working => (IconMenuTone::Working, StudioIconName::StatusRunning),
        UiStatusKind::Good => (IconMenuTone::Good, StudioIconName::StatusRunning),
        UiStatusKind::Warning => (IconMenuTone::Warning, StudioIconName::StepAttention),
        UiStatusKind::Error => (IconMenuTone::Error, StudioIconName::StatusError),
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
    let has_bindings = bindings.has_any();
    let title = format!("{label} bindings");

    rsx! {
        IconMenuButton {
            icon: StudioIconName::BoundValue,
            icon_size: 12,
            label: title.clone(),
            title,
            tone: IconMenuTone::Accent,
            placement: PopoverPlacement::BottomStart,
            active: has_bindings,
            div { class: "tw:grid tw:gap-1",
                span { class: "tw:text-[0.68rem] tw:font-bold tw:uppercase tw:text-heading", "binding" }
                strong { class: "tw:text-sm tw:text-strong-foreground", "{label}" }
            }
            BindingEndpointSection {
                title: "bus target",
                empty: "not assigned",
                endpoints: bindings.bus_target.into_iter().collect(),
            }
            BindingEndpointSection {
                title: "target bindings",
                empty: "none",
                endpoints: bindings.target_bindings,
            }
            BindingEndpointSection {
                title: "consumed by",
                empty: "no consumers",
                endpoints: bindings.consumers,
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn BindingEndpointSection(
    title: &'static str,
    empty: &'static str,
    endpoints: Vec<UiBindingEndpoint>,
) -> Element {
    rsx! {
        div { class: "tw:grid tw:gap-1",
            span { class: "tw:text-[0.68rem] tw:font-bold tw:uppercase tw:text-subtle-foreground", "{title}" }
            if endpoints.is_empty() {
                p { class: "tw:m-0 tw:text-xs tw:text-subtle-foreground", "{empty}" }
            } else {
                div { class: "tw:grid tw:gap-1",
                    for endpoint in endpoints {
                        div { class: "tw:grid tw:min-w-0 tw:gap-0.5 tw:rounded-xs tw:border tw:border-border-subtle tw:bg-page tw:p-2",
                            code { class: "tw:font-mono tw:text-xs tw:text-muted-foreground tw:break-words", "{endpoint.label}" }
                            if let Some(detail) = endpoint.detail.as_ref() {
                                small { class: "tw:text-xs tw:text-subtle-foreground tw:break-words", "{detail}" }
                            }
                        }
                    }
                }
            }
        }
    }
}
