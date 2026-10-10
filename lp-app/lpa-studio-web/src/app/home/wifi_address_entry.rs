//! The Connect a board section's third way in, behind its Network square:
//! one address field and its Connect — core's `devices/connect-wifi-address`
//! offer, drawn with the generic offer form (the field is the offer's own
//! `address` parameter, so the app agent fills the same one).
//!
//! Plain on purpose (the network-transport plan's A1: no new surface): a
//! field, a button, and one line under them saying what the last connect
//! came to — "Connecting to 192.168.1.40…", or why it failed in core's
//! words. Where the page cannot reach the LAN, the offer is disabled with
//! core's reason and the button says it.

use dioxus::prelude::*;
use lpa_studio_core::{OfferArgs, UiAction, UiOffer, UiWifiConnect, WIFI_ADDRESS_PARAM};

use crate::core::{ActionButton, ActionButtonVariant, OfferParamsForm, resolved_args};

/// The field and its Connect, and what the last connect said.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn WifiAddressEntry(
    offer: UiOffer,
    /// The section's connect under way, or why it failed (core's).
    #[props(default)]
    connect: Option<UiWifiConnect>,
    /// Stories only: the field as typed.
    #[props(default)]
    typed: Option<String>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let args = use_signal(move || {
        let mut args = OfferArgs::new();
        if let Some(typed) = typed.clone() {
            args.insert(WIFI_ADDRESS_PARAM.to_string(), typed);
        }
        args
    });
    let current = args.read().clone();
    // An empty field is not a refusal worth printing: the field says
    // what it wants. A typed value that is not an address is.
    let empty = current
        .get(WIFI_ADDRESS_PARAM)
        .is_none_or(|value| value.trim().is_empty());
    // The press as the offer binds it, worded as the row's button: the
    // offer's own label ("Connect a board on Wi‑Fi") is for a reader with
    // no section around it (the app agent, ⌘K).
    let press = match offer.press(&resolved_args(&offer, &current)) {
        Ok(action) => action,
        Err(refused) => offer.action.clone().disabled(refused.to_string()),
    }
    .with_label("Connect");
    let line = connect.as_ref().map(connect_line);
    let failed = connect
        .as_ref()
        .is_some_and(|connect| connect.error.is_some());
    rsx! {
        div { class: "tw:grid tw:w-full tw:gap-1.5 tw:text-left",
            OfferParamsForm { offer: offer.clone(), args }
            div { class: "tw:flex tw:min-w-0 tw:justify-end",
                ActionButton {
                    action: press,
                    running: false,
                    variant: ActionButtonVariant::Outline,
                    reason_said_elsewhere: empty,
                    on_action,
                }
            }
            if let Some(line) = line {
                p {
                    class: if failed { FAILED_LINE_CLASS } else { CONNECTING_LINE_CLASS },
                    role: "status",
                    "{line}"
                }
            }
        }
    }
}

/// What a connect over Wi‑Fi says under its button: under way, or why it
/// failed (core's sentence, as it is).
pub(crate) fn connect_line(connect: &UiWifiConnect) -> String {
    match (&connect.error, connect.through_relay) {
        (Some(error), _) => error.clone(),
        (None, true) => format!("Connecting through {}\u{2026}", connect.host),
        (None, false) => format!("Connecting to {}\u{2026}", connect.host),
    }
}

/// The line while a connect runs.
pub(crate) const CONNECTING_LINE_CLASS: &str =
    "tw:m-0 tw:text-xs tw:leading-snug tw:text-dim-foreground";
/// The line once a connect failed.
pub(crate) const FAILED_LINE_CLASS: &str =
    "tw:m-0 tw:text-xs tw:leading-snug tw:text-status-error-foreground";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_line_says_where_it_is_connecting_or_why_it_failed() {
        let connecting = UiWifiConnect {
            host: "192.168.1.40".to_string(),
            through_relay: false,
            connecting: true,
            error: None,
            busy: false,
        };
        assert_eq!(
            connect_line(&connecting),
            "Connecting to 192.168.1.40\u{2026}"
        );
        let failed = UiWifiConnect {
            connecting: false,
            error: Some(lpa_studio_core::WIFI_BUSY_WORDS.to_string()),
            busy: true,
            ..connecting
        };
        assert_eq!(connect_line(&failed), lpa_studio_core::WIFI_BUSY_WORDS);
        let relay = UiWifiConnect {
            host: "lightplayer.app".to_string(),
            through_relay: true,
            connecting: true,
            error: None,
            busy: false,
        };
        assert_eq!(
            connect_line(&relay),
            "Connecting through lightplayer.app\u{2026}"
        );
    }
}
