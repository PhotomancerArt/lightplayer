//! One transport's offer as a button, and the way forward under it where
//! this browser cannot drive it (the Unlock page draws it; the Connect a
//! board section draws the same note lines under its row).
//!
//! A transport this browser cannot drive is DISABLED with core's reason
//! rather than hidden (G3, 2026-09-24), and the way to go on from there —
//! a link to Bluefy, the Brave flag, this page's address to open where it
//! works — is the web's, because it is about this browser. The text to copy
//! is shown in full and never folded behind a click: it is meant to be
//! selected.

use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiOffer};

use crate::app::home::ble_reach::{BluetoothReach, ble_reach_note};
use crate::app::home::reach_note::{ReachCopy, ReachNote, USB_UNAVAILABLE};
use crate::core::ActionButton;

/// One transport's offer as its button and, when it is disabled, the way
/// forward under it: the reason (the offer's own, core's sentence), an
/// optional link out, and the text to select and copy — shown in full,
/// never folded. The way forward is the web's: it is about this browser.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn TransportOffer(
    offer: UiOffer,
    /// What the button reads: only the path ("via USB"), because the
    /// page around it already says the goal. The offer's own label says
    /// both, for a reader with no page around it (the app agent, ⌘K).
    path_word: &'static str,
    note: Option<ReachNote>,
    page_url: String,
    on_action: EventHandler<UiAction>,
) -> Element {
    // The way forward belongs under a button that cannot be pressed.
    let note = note.filter(|_| !offer.is_enabled());
    let action = offer.action.with_label(path_word);
    rsx! {
        div { class: "tw:grid tw:w-full tw:gap-1",
            ActionButton { action, running: false, on_action }
            if let Some(note) = note {
                ReachNoteLines { note, page_url }
            }
        }
    }
}

/// The way forward a [`ReachNote`] gives: a link out, the line that
/// introduces the text, and the text to select and copy. The reason is not
/// drawn here — the disabled button (or the square's tooltip) carries it.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn ReachNoteLines(
    note: ReachNote,
    /// This page's address, for a note whose copy line is the page itself.
    page_url: String,
    /// Align the link and the copy line to the start of the row they sit
    /// under; the default centres them under a centred button.
    #[props(default)]
    start: bool,
) -> Element {
    let align = match start {
        true => "tw:justify-self-start",
        false => "tw:justify-self-center",
    };
    let copy = note.copy.map(|copy| match copy {
        ReachCopy::Text(text) => text.to_string(),
        ReachCopy::ThisPage => page_url.clone(),
    });
    rsx! {
        if let Some(link) = note.link {
            a {
                class: "{align} tw:text-xs tw:font-semibold tw:text-strong-foreground tw:underline tw:underline-offset-2",
                href: link.href,
                target: "_blank",
                rel: "noopener",
                "{link.label}"
            }
        }
        if let Some(lead) = note.copy_lead.filter(|_| copy.is_some()) {
            p { class: "tw:m-0 tw:text-xs tw:leading-snug tw:text-dim-foreground",
                "{lead}"
            }
        }
        if let Some(copy) = copy {
            code { class: "{align} tw:select-all tw:[overflow-wrap:anywhere] tw:rounded-sm tw:bg-card-muted tw:px-1.5 tw:py-0.5 tw:font-mono tw:text-[11px] tw:text-strong-foreground",
                "{copy}"
            }
        }
    }
}

/// What the Connect a board section says under each transport where this
/// browser cannot drive it: the way forward (a link out, an address to
/// copy), beside the reason core gives the disabled offer. Decided apart
/// from the component so it is testable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct AddSlotNotes {
    /// Under USB.
    pub usb: Option<ReachNote>,
    /// Under Bluetooth.
    pub ble: Option<ReachNote>,
}

pub(crate) fn add_slot_notes(usb_available: bool, ble: BluetoothReach) -> AddSlotNotes {
    let ble_note = ble_reach_note(ble);
    let mut usb_note = (!usb_available).then_some(USB_UNAVAILABLE);
    // Firefox, desktop Safari: BOTH paths send you to Chrome or Edge, with
    // the same "open this page there" — say the address once, under the
    // lower button, rather than twice in a row.
    if let (Some(usb), Some(ble)) = (usb_note.as_mut(), ble_note)
        && usb.copy == ble.copy
        && usb.copy_lead == ble.copy_lead
    {
        usb.copy_lead = None;
        usb.copy = None;
    }
    AddSlotNotes {
        usb: usb_note,
        ble: ble_note,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// G3: both transports are ALWAYS offered. Where this browser cannot
    /// drive one, core disables its offer with the reason, and the web adds
    /// a way to go on under it — never hidden, never a verb that can only
    /// fail.
    #[test]
    fn a_transport_this_browser_cannot_drive_is_disabled_with_a_way_forward() {
        let reasons = |usb: bool, reach: BluetoothReach| {
            lpa_studio_core::add_device_offers(usb, reach, wifi_reach())
                .into_iter()
                // The two choosers; the Wi‑Fi address entry has its own
                // test in core (it waits for its field, not a browser).
                .take(2)
                .map(|offer| match &offer.action.meta().enablement {
                    lpa_studio_core::ActionEnablement::Enabled => None,
                    lpa_studio_core::ActionEnablement::Disabled { reason } => Some(reason.clone()),
                })
                .collect::<Vec<_>>()
        };

        // Chrome/Edge on a computer: both live, nothing to explain.
        assert_eq!(reasons(true, BluetoothReach::Ready), [None, None]);
        assert_eq!(
            add_slot_notes(true, BluetoothReach::Ready),
            AddSlotNotes {
                usb: None,
                ble: None
            }
        );

        // Bluefy: no Web Serial. USB disabled with core's reason, and the
        // page gives its address to open where it works; Bluetooth live.
        assert_eq!(
            reasons(false, BluetoothReach::Ready),
            [
                Some(lpa_studio_core::USB_NEEDS_WEB_SERIAL.to_string()),
                None
            ]
        );
        let bluefy = add_slot_notes(false, BluetoothReach::Ready);
        assert_eq!(bluefy.usb, Some(USB_UNAVAILABLE));
        assert_eq!(
            USB_UNAVAILABLE.reason,
            lpa_studio_core::USB_NEEDS_WEB_SERIAL
        );
        assert_eq!(USB_UNAVAILABLE.copy, Some(ReachCopy::ThisPage));
        assert_eq!(bluefy.ble, None);

        // iPhone Safari / Chrome on iOS: both disabled; Bluetooth points to
        // Bluefy on the App Store and then this page, USB keeps its own.
        assert!(
            reasons(false, BluetoothReach::Ios)
                .iter()
                .all(Option::is_some)
        );
        let ios = add_slot_notes(false, BluetoothReach::Ios);
        let ble_note = ios.ble.expect("the Bluefy path");
        assert_eq!(
            ble_note.link.map(|link| link.href),
            Some(crate::app::home::reach_note::BLUEFY_APP_STORE_URL)
        );
        assert_eq!(ios.usb, Some(USB_UNAVAILABLE));

        // Brave on a computer: USB live, Bluetooth disabled with the flag.
        let brave = add_slot_notes(true, BluetoothReach::Brave);
        assert_eq!(brave.usb, None);
        assert_eq!(
            brave.ble.and_then(|note| note.copy),
            Some(ReachCopy::Text(
                crate::app::home::ble_reach::BRAVE_BLUETOOTH_FLAG
            ))
        );

        // Firefox / desktop Safari: both go to Chrome or Edge — the page's
        // address is said ONCE (under Bluetooth), not twice in a row.
        let firefox = add_slot_notes(false, BluetoothReach::Firefox);
        let usb_note = firefox.usb.expect("USB still says why");
        assert_eq!(usb_note.copy, None, "the address is not repeated");
        assert_eq!(
            firefox.ble.and_then(|note| note.copy),
            Some(ReachCopy::ThisPage)
        );

        // The answer still on its way: disabled, nothing to add under it.
        assert!(reasons(true, BluetoothReach::Checking)[1].is_some());
        assert_eq!(add_slot_notes(true, BluetoothReach::Checking).ble, None);
    }

    fn wifi_reach() -> lpa_studio_core::WifiAddressReach {
        lpa_studio_core::WifiAddressReach {
            available: true,
            connecting: false,
        }
    }
}
