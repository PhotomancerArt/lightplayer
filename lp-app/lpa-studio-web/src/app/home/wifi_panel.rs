//! The device's Wi‑Fi panel: the detail card the Connections group's
//! Wi‑Fi row opens (Wi‑Fi roadmap M5, plan WQ7).
//!
//! Functional, not designed (the Studio UX pass is later): what the board
//! says about its network, in core's words, then the verbs core publishes
//! at `devices/<board>/wifi/…`, each drawn from its offer. The panel never
//! builds an op: the form presses `wifi/set` with what was typed, the
//! switches press `wifi/enabled` and `wifi/cloud-relay` with their new state,
//! and Forget is the offer's own Lasting button (two clicks).
//!
//! The password field is a password input ([`OfferParamsForm`] draws any
//! secret parameter so), its text is cleared once pressed, and it is never
//! echoed — not into a notice, a title or a toast. The board answers whether
//! a password is set; it never answers the password.

use dioxus::prelude::*;
use lpa_studio_core::{OfferArgs, UiAction, UiDeviceWifi, UiOffer, WIFI_ABOUT, WIFI_ENABLED_PARAM};

use super::access_fields::{HELP_CLASS, Switch};
use crate::base::DetailSection;
use crate::core::action::ActionButtonVariant;
use crate::core::offer::offer_params_form::{OfferParamsForm, OfferPressButton};

/// The panel body (the popover's content, and the stories' subject).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn WifiPanel(
    wifi: UiDeviceWifi,
    /// The verbs core publishes under `devices/<board>/wifi/` (in the app,
    /// read from the offer tree; in a story, built by core directly).
    offers: Vec<UiOffer>,
    on_action: EventHandler<UiAction>,
    /// Stories only: what the form starts holding.
    #[props(default)]
    args_preview: Option<OfferArgs>,
    /// Stories only: Forget starts armed.
    #[props(default)]
    forget_armed_preview: bool,
) -> Element {
    let mut args = use_signal(|| args_preview.clone().unwrap_or_default());
    let verb = |name: &str| {
        offers
            .iter()
            .find(|offer| offer.path.last() == Some(name))
            .cloned()
    };
    let set = verb("set");
    let enabled = verb("enabled");
    let cloud_relay = verb("cloud-relay");
    let forget = verb("forget");
    let status_line = wifi.status_line();
    let relay_line = wifi.relay_line();
    let current = args.read().clone();
    rsx! {
        DetailSection { title: "Wi‑Fi".to_string(),
            div { class: "tw:grid tw:min-w-0 tw:gap-2 tw:pb-1",
                p { class: "tw:m-0 tw:text-xs tw:leading-snug tw:text-muted-foreground", "{WIFI_ABOUT}" }
                if let Some(line) = wifi.waiting_line() {
                    p { class: HELP_CLASS, "{line}" }
                }
                if let Some(line) = status_line {
                    p { class: "tw:m-0 tw:text-[13px] tw:font-semibold tw:leading-snug tw:text-strong-foreground", "{line}" }
                }
                if let Some(line) = relay_line {
                    p { class: HELP_CLASS, "{line}" }
                }
                if let Some(set) = set {
                    OfferParamsForm { offer: set.clone(), args }
                    div { class: "tw:flex tw:min-w-0 tw:justify-end",
                        OfferPressButton {
                            offer: set,
                            args: current,
                            variant: ActionButtonVariant::Outline,
                            on_action: move |action| {
                                on_action.call(action);
                                // The password leaves the form with the press.
                                args.set(OfferArgs::new());
                            },
                        }
                    }
                }
                if wifi.writing {
                    p { class: HELP_CLASS, "Writing to the device…" }
                }
                if let Some(error) = wifi.error.clone() {
                    p { class: "tw:m-0 tw:text-xs tw:leading-relaxed tw:text-status-error-foreground", "{error}" }
                }
            }
        }
        if enabled.is_some() || cloud_relay.is_some() || forget.is_some() {
            DetailSection {
                div { class: "tw:grid tw:min-w-0 tw:gap-1.5",
                    if let Some(offer) = enabled {
                        SwitchRow { offer, on_action }
                    }
                    if let Some(offer) = cloud_relay {
                        SwitchRow { detail: offer.summary().to_string(), offer, on_action }
                    }
                    if let Some(forget) = forget {
                        div { class: "tw:flex tw:min-w-0 tw:justify-end tw:pt-1",
                            OfferPressButton {
                                offer: forget,
                                args: OfferArgs::new(),
                                variant: ActionButtonVariant::Quiet,
                                armed_preview: forget_armed_preview,
                                on_action,
                            }
                        }
                    }
                }
            }
        }
    }
}

/// One switch offer as a row: its label (with `detail` under it, when
/// given — the offer's own summary) and a switch that presses the offer
/// with the new state.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn SwitchRow(
    offer: UiOffer,
    on_action: EventHandler<UiAction>,
    #[props(default)] detail: String,
) -> Element {
    let Some(param) = offer.params().first().cloned() else {
        return rsx! {};
    };
    let on = switch_state(&offer);
    let locked = !offer.is_enabled();
    let reason = match &offer.action.meta().enablement {
        lpa_studio_core::ActionEnablement::Disabled { reason } => reason.clone(),
        lpa_studio_core::ActionEnablement::Enabled => String::new(),
    };
    rsx! {
        div { class: "tw:flex tw:min-h-9 tw:min-w-0 tw:items-center tw:gap-2.5", title: "{reason}",
            div { class: "tw:grid tw:min-w-0 tw:flex-1 tw:gap-0.5",
                span { class: "tw:text-[13px] tw:font-semibold tw:text-strong-foreground",
                    "{capitalized(&param.label)}"
                }
                if !detail.is_empty() {
                    span { class: HELP_CLASS, "{detail}" }
                }
            }
            Switch {
                on,
                label: capitalized(&param.label),
                locked,
                on_toggle: move |next: bool| {
                    if let Ok(action) = offer.press(&OfferArgs::new().with(WIFI_ENABLED_PARAM, next.to_string())) {
                        on_action.call(action);
                    }
                },
            }
        }
    }
}

/// A switch offer's current state: its toggle parameter's value.
fn switch_state(offer: &UiOffer) -> bool {
    offer
        .params()
        .first()
        .and_then(|param| param.default_value())
        .is_some_and(|value| value == "true")
}

/// `label` with its first letter capitalized (core's labels are lower case:
/// "cloud relay" reads "Cloud relay").
fn capitalized(label: &str) -> String {
    let mut chars = label.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_label_reads_as_a_sentence() {
        assert_eq!(capitalized("join this network"), "Join this network");
        assert_eq!(capitalized("cloud relay"), "Cloud relay");
    }
}
