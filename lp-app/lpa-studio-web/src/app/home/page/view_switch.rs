//! The cards/list switch: two icon buttons in a segmented control (the
//! spike's `.vswitch`).
//!
//! View state, not a user verb (PD4): it changes only how the page draws
//! what core listed, so no `UiAction` backs it and it is not in the offer
//! tree. The page owns the signal and remembers it ([`HomeViewMode`]).

use dioxus::prelude::*;

use super::home_view_mode::HomeViewMode;
use crate::base::{StudioIcon, StudioIconName};

/// Cards and list, side by side; the current one is on.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn ViewSwitch(mode: HomeViewMode, on_select: EventHandler<HomeViewMode>) -> Element {
    rsx! {
        span {
            class: "tw:inline-flex tw:flex-none tw:overflow-hidden tw:rounded-md tw:border tw:border-border",
            role: "group",
            "aria-label": "View",
            for option in OPTIONS {
                button {
                    key: "{option.mode.stored()}",
                    class: option_class(option.mode == mode, option.first),
                    r#type: "button",
                    title: "{option.label}",
                    "aria-label": "{option.label}",
                    "aria-pressed": "{option.mode == mode}",
                    onclick: move |_| on_select.call(option.mode),
                    StudioIcon { name: option.icon, size: 14 }
                }
            }
        }
    }
}

/// One side of the switch.
#[derive(Clone, Copy)]
struct SwitchOption {
    mode: HomeViewMode,
    label: &'static str,
    icon: StudioIconName,
    first: bool,
}

const OPTIONS: [SwitchOption; 2] = [
    SwitchOption {
        mode: HomeViewMode::Cards,
        label: "Cards",
        icon: StudioIconName::ViewCards,
        first: true,
    },
    SwitchOption {
        mode: HomeViewMode::List,
        label: "List",
        icon: StudioIconName::ViewList,
        first: false,
    },
];

fn option_class(on: bool, first: bool) -> &'static str {
    match (on, first) {
        (true, true) => {
            "tw:grid tw:h-[26px] tw:w-[30px] tw:cursor-pointer tw:place-items-center tw:border-0 tw:bg-selection-bg tw:text-strong-foreground ux-focus-ring"
        }
        (true, false) => {
            "tw:grid tw:h-[26px] tw:w-[30px] tw:cursor-pointer tw:place-items-center tw:border-0 tw:border-l tw:border-border tw:bg-selection-bg tw:text-strong-foreground ux-focus-ring"
        }
        (false, true) => {
            "tw:grid tw:h-[26px] tw:w-[30px] tw:cursor-pointer tw:place-items-center tw:border-0 tw:bg-transparent tw:text-subtle-foreground tw:hover:text-strong-foreground ux-focus-ring"
        }
        (false, false) => {
            "tw:grid tw:h-[26px] tw:w-[30px] tw:cursor-pointer tw:place-items-center tw:border-0 tw:border-l tw:border-border tw:bg-transparent tw:text-subtle-foreground tw:hover:text-strong-foreground ux-focus-ring"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_switch_offers_cards_then_list() {
        let words: Vec<(&str, &str)> = OPTIONS
            .iter()
            .map(|option| (option.label, option.mode.stored()))
            .collect();
        assert_eq!(words, [("Cards", "cards"), ("List", "list")]);
        assert!(OPTIONS[0].first && !OPTIONS[1].first);
    }
}
