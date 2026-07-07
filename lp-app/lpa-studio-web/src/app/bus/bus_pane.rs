//! Bus pane body: channels with linked writer/reader sites.
//!
//! Every site is a navigation affordance — clicking dispatches the node's
//! focus action so the user lands on the node in the Project pane (roadmap
//! D7: the UI feels linked, no path hunting). Binding state wears the
//! violet status-bound family.

use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiBusChannelView, UiBusSiteView, UiBusView};

use crate::base::{StudioIcon, StudioIconName};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn BusPaneBody(view: UiBusView, on_action: EventHandler<UiAction>) -> Element {
    if view.channels.is_empty() {
        return rsx! {
            div { class: "tw:grid tw:gap-1 tw:text-sm tw:text-muted-foreground",
                p { class: "tw:m-0", "No bus channels yet." }
                p { class: "tw:m-0 tw:text-xs tw:leading-snug tw:text-subtle-foreground",
                    "The bus is the project's patch bay: nodes publish and consume "
                    "values on named channels. Bind a slot to "
                    code { class: "tw:font-mono", "bus#…" }
                    " and the channel appears here."
                }
            }
        };
    }

    rsx! {
        div { class: "tw:grid tw:gap-2",
            for channel in view.channels {
                BusChannelRow { channel, on_action }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn BusChannelRow(channel: UiBusChannelView, on_action: EventHandler<UiAction>) -> Element {
    let UiBusChannelView {
        name,
        kind,
        value,
        value_error,
        primary_visual,
        writers,
        readers,
    } = channel;
    let row_class = if primary_visual {
        "tw:grid tw:gap-1 tw:rounded-xs tw:border-l-2 tw:border-status-bound-foreground tw:bg-status-bound-bg tw:px-2 tw:py-1.5"
    } else {
        "tw:grid tw:gap-1 tw:rounded-xs tw:border-l-2 tw:border-status-bound-border tw:bg-transparent tw:px-2 tw:py-1.5"
    };
    let title = if primary_visual {
        format!("bus#{name} — the project's primary visual output")
    } else {
        format!("bus#{name}")
    };

    rsx! {
        div { class: row_class,
            div { class: "tw:flex tw:min-w-0 tw:items-baseline tw:gap-1.5",
                span { class: "tw:flex-none tw:text-status-bound-foreground",
                    StudioIcon {
                        name: StudioIconName::BoundValue,
                        size: 12,
                    }
                }
                code {
                    class: "tw:min-w-0 tw:truncate tw:font-mono tw:text-sm tw:font-bold tw:text-strong-foreground",
                    title,
                    "{name}"
                }
                if let Some(kind) = kind {
                    span { class: "tw:flex-none tw:text-[10px] tw:font-bold tw:uppercase tw:text-subtle-foreground", "{kind}" }
                }
                span { class: "tw:min-w-0 tw:flex-1" }
                if let Some(value) = value {
                    code { class: "tw:flex-none tw:font-mono tw:text-xs tw:text-muted-foreground", "{value}" }
                } else if let Some(error) = value_error {
                    span {
                        class: "tw:flex-none tw:max-w-32 tw:truncate tw:text-xs tw:text-status-error-foreground",
                        title: "{error}",
                        "unresolved"
                    }
                }
            }
            div { class: "tw:flex tw:min-w-0 tw:flex-wrap tw:items-center tw:gap-1",
                if writers.is_empty() {
                    span { class: "tw:text-[11px] tw:text-subtle-foreground", "no writer" }
                }
                for site in writers {
                    BusSiteChip { site, on_action }
                }
                span { class: "tw:flex-none tw:text-[11px] tw:font-bold tw:text-subtle-foreground", "\u{2192}" }
                if readers.is_empty() {
                    span { class: "tw:text-[11px] tw:text-subtle-foreground", "no readers" }
                }
                for site in readers {
                    BusSiteChip { site, on_action }
                }
            }
        }
    }
}

/// One clickable writer/reader site: node label (+ slot), violet outline,
/// dispatching the node's focus action.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn BusSiteChip(site: UiBusSiteView, on_action: EventHandler<UiAction>) -> Element {
    let UiBusSiteView {
        node_label,
        slot,
        default_origin,
        focus,
    } = site;
    let slot_suffix = slot.map(|slot| format!(".{slot}"));
    let mut tooltip = match &slot_suffix {
        Some(suffix) => format!("{node_label}{suffix}"),
        None => node_label.clone(),
    };
    if default_origin {
        tooltip.push_str(" — default binding");
    }
    if focus.is_some() {
        tooltip.push_str(" (click to focus)");
    }
    let class = "tw:inline-flex tw:min-w-0 tw:max-w-44 tw:cursor-pointer tw:appearance-none tw:items-center tw:gap-1 tw:rounded-xs tw:border tw:border-status-bound-border tw:bg-transparent tw:px-1.5 tw:py-0.5 tw:leading-none tw:text-status-bound-foreground tw:transition-colors tw:hover:border-status-bound-foreground";

    rsx! {
        button {
            class,
            r#type: "button",
            title: tooltip,
            disabled: focus.is_none(),
            onclick: move |event| {
                event.stop_propagation();
                if let Some(focus) = focus.clone() {
                    on_action.call(focus);
                }
            },
            span { class: "tw:min-w-0 tw:truncate tw:text-[11px] tw:font-semibold", "{node_label}" }
            if let Some(suffix) = slot_suffix {
                code { class: "tw:min-w-0 tw:truncate tw:font-mono tw:text-[10px] tw:text-muted-foreground", "{suffix}" }
            }
            if default_origin {
                span {
                    class: "tw:flex-none tw:text-[9px] tw:font-bold tw:uppercase tw:text-subtle-foreground",
                    title: "Materialized from default binding policy",
                    "def"
                }
            }
        }
    }
}
