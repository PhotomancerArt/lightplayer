//! Connect a board when a USB or Bluetooth press came to nothing after the
//! pick: the section says so under its squares, in core's words
//! (`lpa_studio_core::UiChooserFailure`), until the next press.

use dioxus::prelude::*;
use lpa_studio_core::{
    BluetoothReach, ChooserTransport, UiChooserFailure, UiOfferTree, WifiAddressReach,
    add_device_offers, new_sim_offer,
};
use lpa_studio_web_story_macros::story;

use crate::app::home::connect_board::ConnectBoardSection;
use crate::core::OffersProvider;

#[story(
    description = "A Bluetooth board was picked in the browser's chooser and then ended the connection before it answered (an Android phone's first connection panicked the C6, 2026-10-17; it was one console warning and nothing on the page). The section says what happened and the way on, in red under the squares, until the next press of USB or Bluetooth."
)]
fn connect_board_bluetooth_dropped() -> Element {
    section_with(UiChooserFailure::new(
        ChooserTransport::Bluetooth,
        "GATT Server is disconnected. Cannot retrieve services.",
    ))
}

#[story(
    description = "A USB port picked in the chooser that would not open: almost always another program (a serial monitor, another tab) holding it."
)]
fn connect_board_usb_would_not_open() -> Element {
    section_with(UiChooserFailure::new(
        ChooserTransport::Usb,
        "Failed to open serial port.",
    ))
}

/// The Connect a board section alone, in Chrome on a computer, saying
/// `failure`.
fn section_with(failure: UiChooserFailure) -> Element {
    let mut offers = UiOfferTree::new();
    let wifi = WifiAddressReach {
        available: true,
        connecting: false,
    };
    for offer in add_device_offers(true, BluetoothReach::Ready, wifi) {
        offers.publish(offer);
    }
    offers.publish(new_sim_offer());
    rsx! {
        div { class: "tw:max-w-[360px] tw:p-3",
            OffersProvider { offers,
                ConnectBoardSection {
                    ble_reach: Some(BluetoothReach::Ready),
                    usb_available: true,
                    page_url: Some("https://lightplayer.app/".to_string()),
                    chooser_failure: Some(failure),
                    on_action: |_| {},
                }
            }
        }
    }
}
