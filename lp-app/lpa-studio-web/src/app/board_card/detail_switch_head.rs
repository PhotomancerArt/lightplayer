//! [`DetailSwitchHead`]: the first line of a radio row's details — the
//! row's icon and name, and the board's own on/off switch at the line's
//! end — drawn alike for Wi‑Fi, Bluetooth and lightplayer.app, as macOS
//! draws the top of its Wi‑Fi and Bluetooth menus
//! (`docs/ux/anatomy/board-card.md`, "Hushing a row").
//!
//! The switch is the shared `button[role=switch]`
//! ([`Switch`](crate::app::home::access_fields::Switch)): on wears the good
//! tone, and `aria-checked` is the state. A press is core's offer pressed
//! to the other state ([`UiDetailSwitch::flip`]), so the agent finds and
//! presses the same thing; its hover says what a flip does, in the offer's
//! own words (Bluetooth's restarts the board). Where this link may not flip it, the switch is
//! dimmed; where the offer refuses, its reason sits under the line
//! ("Bluetooth can only be turned off by USB…"), so "why can't I?" is
//! answered where it is asked. A switch only *waiting* on a change on its
//! way ([`UiDetailSwitch::waiting`]) is dimmed with the reason on its
//! hover instead: a line that came and went with every change made the
//! whole box jump (P3, Yona 10-25).

use dioxus::prelude::*;
use lpa_studio_core::{ActionEnablement, OfferPath, UiAction, UiDetailSwitch};

use super::card_action::card_press;
use crate::app::agent::AgentMark;
use crate::app::home::access_fields::{HELP_CLASS, Switch};
use crate::base::{StudioIcon, StudioIconName, action_icon_name};
use crate::core::offer::use_offer_at;

/// One switch line. See the module doc.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn DetailSwitchHead(
    switch: UiDetailSwitch,
    on_action: EventHandler<UiAction>,
) -> Element {
    let path = switch.flip.as_ref().map(|flip| flip.offer.clone());
    // Always one lookup, so the hooks keep their order; a switch with no
    // flip looks up nothing it could press.
    let offer = use_offer_at(path.clone().unwrap_or_else(OfferPath::devices))();
    let pressed = switch
        .flip
        .as_ref()
        .zip(offer.as_ref())
        .map(|(flip, offer)| card_press(offer, flip));
    let refused = match pressed.as_ref().map(|action| &action.meta().enablement) {
        Some(ActionEnablement::Disabled { reason }) => Some(reason.clone()),
        _ => None,
    };
    let locked = pressed.is_none() || refused.is_some();
    // A wait is said on hover, a standing refusal under the line.
    let (under, waited) = match refused {
        Some(reason) if switch.waiting => (None, Some(reason)),
        refused => (refused, None),
    };
    let icon = action_icon_name(Some(&switch.icon)).unwrap_or(StudioIconName::Info);
    // What a flip does, in the offer's own words ("Turn the board's
    // Bluetooth off… It restarts to apply it."), on hover.
    let title = waited
        .or_else(|| pressed.as_ref().map(|action| action.meta().summary.clone()))
        .unwrap_or_default();
    rsx! {
        div { class: HEAD_CLASS, "data-detail-switch": "{switch.icon}",
            div { class: LINE_CLASS,
                span { class: "tw:inline-flex tw:flex-none", aria_hidden: "true",
                    StudioIcon { name: icon, size: 15 }
                }
                h3 { class: LABEL_CLASS, "{switch.label}" }
                AgentMark { path,
                    span { class: "tw:inline-flex", title: "{title}",
                        Switch {
                            on: switch.on,
                            label: switch.label.clone(),
                            locked,
                            on_toggle: move |_| {
                                if let Some(action) = pressed.clone() {
                                    on_action.call(action);
                                }
                            },
                        }
                    }
                }
            }
            if let Some(reason) = under {
                p { class: HELP_CLASS, "data-detail-switch-refused": "true", "{reason}" }
            }
        }
    }
}

/// The line's box: the details' section padding, the divider below it
/// coming from the first section's own top border.
const HEAD_CLASS: &str = "tw:grid tw:min-w-0 tw:gap-1 tw:px-3 tw:py-2.5";

/// Icon, name, then the switch pushed to the end.
const LINE_CLASS: &str =
    "tw:flex tw:min-w-0 tw:items-center tw:gap-2 tw:text-status-neutral-foreground";

/// The row's name, as the Wi‑Fi panel's page head draws its title.
const LABEL_CLASS: &str =
    "tw:m-0 tw:min-w-0 tw:flex-1 tw:truncate tw:text-sm tw:font-bold tw:text-strong-foreground";

#[cfg(test)]
mod tests {
    use super::*;

    /// The name takes the line and pushes the switch to its end; it is
    /// the Wi‑Fi panel's title, strong and bold, cut at its end.
    #[test]
    fn the_name_fills_the_line_and_the_switch_sits_at_its_end() {
        assert!(LABEL_CLASS.contains("tw:flex-1"));
        assert!(LABEL_CLASS.contains("tw:truncate"));
        assert!(LABEL_CLASS.contains("tw:font-bold"));
        assert!(LINE_CLASS.contains("tw:items-center"));
    }
}
