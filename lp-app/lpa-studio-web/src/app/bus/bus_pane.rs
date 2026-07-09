//! Bus pane body: one [`SlotPane`] per channel.
//!
//! Each channel wears the shared slot-pane language (roadmap D10 — one
//! binding treatment everywhere): violet `Bound` frame, channel name in the
//! title bar with kind/PRIMARY badges, and the live value as the centered
//! hero. The main display stays tight — just the values; the wiring
//! (writers → readers, merge semantics, default origins) lives in the
//! detail popup (`UiBusChannelView::visible_aspects`), where every site row
//! is a clickable focus affordance (D7: the UI feels linked; D11: no dead
//! ends).

use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiBusChannelView, UiBusView};

use crate::app::node::{SlotPane, SlotPaneTreatment};

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
                    code { class: "tw:font-mono", "bus:…" }
                    " and the channel appears here."
                }
            }
        };
    }

    rsx! {
        div { class: "tw:grid tw:gap-2",
            for channel in view.channels {
                BusChannelPane { channel, on_action }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn BusChannelPane(
    channel: UiBusChannelView,
    on_action: EventHandler<UiAction>,
    #[props(default = false)] initially_open: bool,
) -> Element {
    let aspects = channel.visible_aspects();
    let UiBusChannelView {
        name,
        kind,
        value,
        value_error,
        primary_visual,
        ..
    } = channel;

    rsx! {
        SlotPane {
            title: name,
            aspects,
            initially_open,
            treatment: SlotPaneTreatment::Bound,
            on_action,
            badges: rsx! {
                if primary_visual {
                    span {
                        class: "tw:flex-none tw:rounded-xs tw:bg-status-bound-bg tw:px-1 tw:text-[9px] tw:font-bold tw:uppercase tw:leading-snug tw:text-status-bound-foreground",
                        title: "The project's primary visual output",
                        "primary"
                    }
                }
                if let Some(kind) = kind {
                    span { class: "tw:flex-none tw:text-[10px] tw:font-bold tw:uppercase tw:text-subtle-foreground", "{kind}" }
                }
            },
            if let Some(value) = value {
                code { class: "tw:min-w-0 tw:break-all tw:text-center tw:font-mono tw:text-sm tw:font-bold tw:text-strong-foreground", "{value}" }
            } else if let Some(error) = value_error {
                span {
                    class: "tw:min-w-0 tw:truncate tw:text-xs tw:text-status-error-foreground",
                    title: "{error}",
                    "unresolved"
                }
            } else {
                span { class: "tw:text-xs tw:text-subtle-foreground", "\u{2014}" }
            }
        }
    }
}
