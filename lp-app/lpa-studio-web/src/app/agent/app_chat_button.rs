//! [`AppChatButton`]: the site chrome's door to the app chat, beside the
//! AI settings trigger. It toggles the drawer (web-local chrome, like a
//! popover's open flag), wears the pressed look while the drawer is open,
//! and carries a dot while the drawer is closed and something is waiting
//! on the user — a card to click — or the assistant is still working.

use dioxus::prelude::*;

use crate::base::{StudioIcon, StudioIconName};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn AppChatButton(
    open: Signal<bool>,
    /// A card is waiting for the user's click.
    #[props(default = false)]
    pending_card: bool,
    /// A run is in flight.
    #[props(default = false)]
    busy: bool,
) -> Element {
    let mut open = open;
    let is_open = open();
    let dot = (!is_open).then_some(if pending_card {
        Some(PENDING_DOT_CLASS)
    } else if busy {
        Some(BUSY_DOT_CLASS)
    } else {
        None
    });
    let title = match (is_open, pending_card) {
        (true, _) => "Close the assistant",
        (false, true) => "Assistant — a card is waiting for your click",
        (false, false) => "Assistant — tell it what you want your lights to do",
    };
    rsx! {
        button {
            class: if is_open { TRIGGER_OPEN_CLASS } else { TRIGGER_CLASS },
            r#type: "button",
            title,
            aria_label: "Assistant",
            aria_pressed: "{is_open}",
            onclick: move |_| open.set(!is_open),
            StudioIcon { name: StudioIconName::Agent, size: 15 }
            if let Some(Some(dot)) = dot {
                span { class: dot }
            }
        }
    }
}

const TRIGGER_CLASS: &str = "tw:relative tw:inline-flex tw:h-7 tw:w-7 tw:flex-none tw:cursor-pointer tw:items-center tw:justify-center tw:rounded-full tw:border tw:border-border-strong tw:bg-card tw:p-0 tw:text-strong-foreground tw:transition tw:duration-300 tw:hover:bg-card-raised";

const TRIGGER_OPEN_CLASS: &str = "tw:relative tw:inline-flex tw:h-7 tw:w-7 tw:flex-none tw:cursor-pointer tw:items-center tw:justify-center tw:rounded-full tw:border tw:border-strong-foreground tw:bg-card-raised tw:p-0 tw:text-strong-foreground tw:transition tw:duration-300";

const PENDING_DOT_CLASS: &str = "tw:absolute tw:-right-0.5 tw:-top-0.5 tw:h-2 tw:w-2 tw:rounded-full tw:bg-status-warning-foreground";

const BUSY_DOT_CLASS: &str = "tw:absolute tw:-right-0.5 tw:-top-0.5 tw:h-2 tw:w-2 tw:animate-pulse tw:rounded-full tw:bg-status-working-foreground";
