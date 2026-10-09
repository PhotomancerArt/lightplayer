//! One of the Connect a board section's three square buttons.
//!
//! A square is one offer drawn as an icon over one word. It builds no
//! action: the section hands it the offer from the tree, and a press tells
//! the section, which forwards the tree's own action (USB, Bluetooth) or
//! opens the address row (Network). It honours the offer's enablement the
//! way every offer button does: disabled, with core's reason as its tooltip.

use dioxus::prelude::*;
use lpa_studio_core::{ActionEnablement, UiOffer};

use crate::base::{StudioIcon, action_icon_name};

/// A square button, about 84 × 72 px: the offer's own icon over `word`.
/// Its visible text is exactly the word, so a walk can find it by that.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn ConnectSquare(
    offer: UiOffer,
    /// The one word under the icon ("USB", "Bluetooth", "Network").
    word: &'static str,
    /// The square's panel is open (Network, while its address row shows).
    #[props(default)]
    pressed: bool,
    on_press: EventHandler<()>,
) -> Element {
    let reason = disabled_reason(&offer);
    let disabled = reason.is_some();
    // The tooltip is core's reason where there is one, else what the offer
    // does in full ("Connect a board via USB").
    let title = reason.unwrap_or_else(|| offer.label().to_string());
    let icon = action_icon_name(offer.action.meta().icon.as_deref());
    rsx! {
        button {
            class: square_class(pressed, disabled),
            r#type: "button",
            disabled,
            title: "{title}",
            "aria-expanded": pressed.then_some("true"),
            onclick: move |_| on_press.call(()),
            if let Some(icon) = icon {
                span { class: "tw:inline-flex tw:text-heading", aria_hidden: "true",
                    StudioIcon { name: icon, size: 20 }
                }
            }
            span { "{word}" }
        }
    }
}

/// Why the offer cannot be pressed now (core's sentence), or `None` when it
/// can. The square is disabled exactly when this is `Some`.
///
/// An offer that takes a value is "disabled" in core until the value is
/// there ("choose a board's address"). That is not a reason to refuse the
/// square: the square only opens the entry that asks for the value. So an
/// offer that is merely a value short counts as pressable; only one that is
/// disabled for its own sake (the page cannot reach the LAN, an address is
/// being reached) does not.
pub(crate) fn disabled_reason(offer: &UiOffer) -> Option<String> {
    match &offer.action.meta().enablement {
        ActionEnablement::Enabled => None,
        ActionEnablement::Disabled { .. } if offer.awaits_a_value() => None,
        ActionEnablement::Disabled { reason } => Some(reason.clone()),
    }
}

/// The square's classes: the same size whether it is resting, hovered,
/// open or disabled, so nothing around it moves.
fn square_class(pressed: bool, disabled: bool) -> &'static str {
    match (disabled, pressed) {
        (true, _) => {
            "tw:grid tw:h-[72px] tw:w-[84px] tw:flex-none tw:cursor-not-allowed tw:place-items-center tw:content-center tw:gap-1.5 tw:rounded-md tw:border tw:border-border-muted tw:bg-card tw:text-[11.5px] tw:font-semibold tw:text-dim-foreground tw:opacity-60"
        }
        (false, true) => {
            "tw:grid tw:h-[72px] tw:w-[84px] tw:flex-none tw:cursor-pointer tw:place-items-center tw:content-center tw:gap-1.5 tw:rounded-md tw:border tw:border-selection-border tw:bg-selection-bg tw:text-[11.5px] tw:font-semibold tw:text-strong-foreground ux-focus-ring"
        }
        (false, false) => {
            "tw:grid tw:h-[72px] tw:w-[84px] tw:flex-none tw:cursor-pointer tw:place-items-center tw:content-center tw:gap-1.5 tw:rounded-md tw:border tw:border-border tw:bg-card tw:text-[11.5px] tw:font-semibold tw:text-strong-foreground tw:transition-[border-color,box-shadow] tw:hover:border-border-strong tw:hover:shadow-[0_2px_20px_rgba(140,170,255,0.14)] ux-focus-ring"
        }
    }
}

#[cfg(test)]
mod tests {
    use lpa_studio_core::{BluetoothReach, WifiAddressReach, add_device_offers};

    use super::*;

    fn offers(usb: bool, ble: BluetoothReach, wifi: WifiAddressReach) -> Vec<UiOffer> {
        add_device_offers(usb, ble, wifi)
    }

    fn reach(available: bool, connecting: bool) -> WifiAddressReach {
        WifiAddressReach {
            available,
            connecting,
        }
    }

    #[test]
    fn the_three_offers_wear_the_icons_the_squares_draw() {
        let [usb, ble, wifi] = offers(true, BluetoothReach::Ready, reach(true, false))
            .try_into()
            .expect("three transports");
        let icon = |offer: &UiOffer| action_icon_name(offer.action.meta().icon.as_deref());
        assert_eq!(usb.path.to_string(), "devices/connect-usb");
        assert_eq!(icon(&usb), Some(crate::base::StudioIconName::Usb));
        assert_eq!(ble.path.to_string(), "devices/connect-ble");
        assert_eq!(icon(&ble), Some(crate::base::StudioIconName::Bluetooth));
        assert_eq!(wifi.path.to_string(), "devices/connect-wifi-address");
        assert_eq!(icon(&wifi), Some(crate::base::StudioIconName::Wifi));
    }

    #[test]
    fn a_square_is_disabled_exactly_when_its_offer_is_and_says_why() {
        let [usb, ble, _] = offers(false, BluetoothReach::Ready, reach(true, false))
            .try_into()
            .expect("three transports");
        assert_eq!(
            disabled_reason(&usb).as_deref(),
            Some(lpa_studio_core::USB_NEEDS_WEB_SERIAL)
        );
        assert_eq!(disabled_reason(&ble), None);
    }

    /// An empty address field is not a disabled offer: the field says what it
    /// wants. Only a page that cannot reach the LAN, or an address being
    /// reached, disables the Network square.
    #[test]
    fn network_is_disabled_only_when_its_offer_is() {
        let wifi = |reach| {
            offers(true, BluetoothReach::Ready, reach)
                .into_iter()
                .nth(2)
                .expect("the Wi-Fi address offer")
        };
        assert_eq!(disabled_reason(&wifi(reach(true, false))), None);
        assert_eq!(
            disabled_reason(&wifi(reach(false, false))).as_deref(),
            Some(lpa_studio_core::WIFI_NEEDS_WEBSOCKET)
        );
        assert_eq!(
            disabled_reason(&wifi(reach(true, true))).as_deref(),
            Some(lpa_studio_core::WIFI_CONNECTING)
        );
    }
}
