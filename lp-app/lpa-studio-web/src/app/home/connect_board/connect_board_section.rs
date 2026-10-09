//! "Connect a board": the home page's section for the three ways a board
//! comes in, and the quiet detour that starts one here.
//!
//! ```text
//!   Connect a board
//!   [ USB ] [ Bluetooth ] [ Network ]   No board? Try an example ↓
//!   (the way forward under a transport this browser cannot drive)
//!   start a board here ▾
//! ```
//!
//! It looks the same to a newcomer and to someone with a dozen boards; a
//! newcomer's adds one hint line after the squares, and the returning
//! user's simply is not first on the page.
//!
//! # What is core's, what is the web's
//!
//! The buttons are offers: `devices/connect-usb`, `devices/connect-ble`,
//! `devices/connect-wifi-address` and `devices/new-sim`. USB and Bluetooth
//! press the tree's own action; Network only opens the address row, whose
//! field and Connect are [`WifiAddressEntry`] pressing
//! `devices/connect-wifi-address`. A transport this browser cannot drive is
//! DISABLED with core's reason (the square's tooltip), and the way forward
//! — a link out, an address to copy — is the web's, drawn quietly under the
//! row for that transport only, in full and never folded.

use dioxus::prelude::*;
use lpa_studio_core::{OfferPath, UiAction, UiHomeSection, UiOffer, UiWifiConnect};

use crate::app::home::ble_reach::{BluetoothReach, use_ble_reach};
use crate::app::home::connect_board::connect_square::ConnectSquare;
use crate::app::home::connect_board::transport_offer::{
    AddSlotNotes, ReachNoteLines, add_slot_notes,
};
use crate::app::home::reach_note::{ReachNote, this_page_url};
use crate::app::home::section_title_class;
use crate::app::home::target_pick_popover::TargetPickPopover;
use crate::app::home::wifi_address_entry::WifiAddressEntry;
use crate::core::use_offer_at;

/// The id of the examples section the welcome hint scrolls to. The page's
/// first example group carries it.
pub(crate) const HOME_EXAMPLES_ID: &str = "home-examples";

/// The id of this section's root: the walks scope their clicks to it.
pub(crate) const CONNECT_BOARD_ID: &str = "home-connect-board";

/// The words on the three squares. A walk clicks them by these, exactly, inside
/// `#home-connect-board`.
const USB_WORD: &str = "USB";
const BLUETOOTH_WORD: &str = "Bluetooth";
const NETWORK_WORD: &str = "Network";

/// The newcomer's hint, before and after its link.
const WELCOME_LEAD: &str = "No board?";
const WELCOME_LINK: &str = "Try an example \u{2193}";

/// The Connect a board section.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn ConnectBoardSection(
    /// Whether this browser can reach a USB port (Web Serial, or the
    /// `?emu=` shim that polyfills it). Where it cannot — iPhone, Bluefy,
    /// Firefox, Safari — the USB square is disabled with core's reason and
    /// the way forward under the row.
    #[props(default = true)]
    usb_available: bool,
    /// Whether this build reaches any port at all. Without a transport
    /// the section says so instead of showing squares that can only fail.
    #[props(default = true)]
    transport_available: bool,
    /// The Network row's connect under way, or why it failed (core's).
    #[props(default)]
    wifi_connect: Option<UiWifiConnect>,
    /// A first visit: add the one hint line after the squares.
    #[props(default)]
    welcome: bool,
    /// Stories only: pin what the Bluetooth half says. Real surfaces ask
    /// the browser (`use_ble_reach`).
    #[props(default = None)]
    ble_reach: Option<BluetoothReach>,
    /// Stories only: the address the copy lines show, pinned so a capture
    /// does not print the story server's own URL.
    #[props(default = None)]
    page_url: Option<String>,
    /// Stories only: the Network field as typed.
    #[props(default)]
    wifi_typed: Option<String>,
    /// Stories only: mount the "start a board here" menu open (a capture
    /// cannot click).
    #[props(default)]
    pick_open: bool,
    /// Stories only: mount with the Network row already open.
    #[props(default)]
    network_open: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    let asked = use_ble_reach();
    let ble = ble_reach.unwrap_or_else(|| asked());
    let page_url = page_url.unwrap_or_else(this_page_url);
    let usb = use_offer_at(OfferPath::devices().child("connect-usb"))();
    let ble_offer = use_offer_at(OfferPath::devices().child("connect-ble"))();
    let wifi_offer = use_offer_at(OfferPath::devices().child("connect-wifi-address"))();
    let mut row_open = use_signal(move || network_open);

    let heading = UiHomeSection::ConnectBoard.label();
    let body = connect_body(transport_available);
    let notes = shown_notes(
        usb.as_ref(),
        ble_offer.as_ref(),
        add_slot_notes(usb_available, ble),
    );
    let hint = welcome_hint(welcome);

    rsx! {
        section { id: CONNECT_BOARD_ID, class: "tw:grid tw:gap-3",
            header { class: "tw:flex tw:items-baseline tw:justify-between tw:gap-3",
                h2 { class: section_title_class(), "{heading}" }
            }
            if body == ConnectBody::NoTransport {
                UnavailableNote {}
            }
            if body == ConnectBody::Squares {
                div { class: "tw:flex tw:flex-wrap tw:items-start tw:gap-2.5",
                    if let Some(usb) = usb {
                        ConnectSquare {
                            word: USB_WORD,
                            on_press: {
                                let action = usb.action.clone();
                                move |_| on_action.call(action.clone())
                            },
                            offer: usb,
                        }
                    }
                    if let Some(ble) = ble_offer {
                        ConnectSquare {
                            word: BLUETOOTH_WORD,
                            on_press: {
                                let action = ble.action.clone();
                                move |_| on_action.call(action.clone())
                            },
                            offer: ble,
                        }
                    }
                    if let Some(wifi) = wifi_offer {
                        // Network only opens its row; the row's Connect
                        // presses `devices/connect-wifi-address`.
                        ConnectSquare {
                            word: NETWORK_WORD,
                            pressed: row_open(),
                            on_press: move |_| {
                                let open = row_open();
                                row_open.set(!open);
                            },
                            offer: wifi.clone(),
                        }
                        if row_open() {
                            div { class: "tw:min-w-60 tw:flex-1 tw:basis-60 tw:max-w-sm",
                                WifiAddressEntry {
                                    offer: wifi,
                                    connect: wifi_connect,
                                    typed: wifi_typed,
                                    on_action,
                                }
                            }
                        }
                    }
                    if let Some(hint) = hint {
                        p { class: "tw:m-0 tw:flex tw:flex-wrap tw:items-baseline tw:gap-1.5 tw:self-center tw:text-xs tw:text-subtle-foreground",
                            span { "{hint.lead}" }
                            button {
                                class: "tw:cursor-pointer tw:border-0 tw:bg-transparent tw:p-0 tw:text-xs tw:font-semibold tw:text-muted-foreground tw:underline tw:underline-offset-2 tw:hover:text-strong-foreground ux-focus-ring",
                                r#type: "button",
                                onclick: move |_| scroll_to_examples(),
                                "{hint.link}"
                            }
                        }
                    }
                }
                for (key , note) in [("usb", notes.usb), ("ble", notes.ble)] {
                    if let Some(note) = note {
                        div { key: "{key}", class: "tw:grid tw:max-w-sm tw:gap-1 tw:text-left",
                            p { class: "tw:m-0 tw:text-xs tw:leading-snug tw:text-dim-foreground",
                                "{note.reason}"
                            }
                            ReachNoteLines { note, page_url: page_url.clone(), start: true }
                        }
                    }
                }
                div { class: "tw:justify-self-start",
                    TargetPickPopover { initially_open: pick_open, on_action }
                }
            }
        }
    }
}

/// What the section's body is, decided apart from the component so it is
/// testable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ConnectBody {
    /// The three squares, the way forward under them, and "start a board
    /// here".
    Squares,
    /// This build or browser reaches no port at all.
    NoTransport,
}

pub(crate) fn connect_body(transport_available: bool) -> ConnectBody {
    match transport_available {
        true => ConnectBody::Squares,
        false => ConnectBody::NoTransport,
    }
}

/// The way forward under each transport, for the transports that are
/// disabled now. An enabled transport has nothing to explain, whatever the
/// browser says about its siblings.
pub(crate) fn shown_notes(
    usb: Option<&UiOffer>,
    ble: Option<&UiOffer>,
    notes: AddSlotNotes,
) -> AddSlotNotes {
    let when_disabled = |offer: Option<&UiOffer>, note: Option<ReachNote>| {
        note.filter(|_| offer.is_some_and(|offer| !offer.is_enabled()))
    };
    AddSlotNotes {
        usb: when_disabled(usb, notes.usb),
        ble: when_disabled(ble, notes.ble),
    }
}

/// The newcomer's hint line; `None` for everyone else.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct WelcomeHint {
    pub lead: &'static str,
    pub link: &'static str,
}

pub(crate) fn welcome_hint(welcome: bool) -> Option<WelcomeHint> {
    welcome.then_some(WelcomeHint {
        lead: WELCOME_LEAD,
        link: WELCOME_LINK,
    })
}

/// Scroll the examples section into view: the welcome hint's whole job. Web
/// only — not a route change and not an action, so it is nowhere in the offer
/// tree. A missing target (the examples are filtered off the page) is a
/// silent no-scroll.
#[cfg(target_arch = "wasm32")]
fn scroll_to_examples() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(target) = window
        .document()
        .and_then(|document| document.get_element_by_id(HOME_EXAMPLES_ID))
    else {
        return;
    };
    let reduced = window
        .match_media("(prefers-reduced-motion: reduce)")
        .ok()
        .flatten()
        .is_some_and(|query| query.matches());
    let options = web_sys::ScrollIntoViewOptions::new();
    options.set_behavior(match reduced {
        true => web_sys::ScrollBehavior::Auto,
        false => web_sys::ScrollBehavior::Smooth,
    });
    options.set_block(web_sys::ScrollLogicalPosition::Start);
    target.scroll_into_view_with_scroll_into_view_options(&options);
}

/// Host builds (tests, stories' static captures) have nothing to scroll.
#[cfg(not(target_arch = "wasm32"))]
fn scroll_to_examples() {
    let _ = HOME_EXAMPLES_ID;
}

/// The headline when no transport is available.
pub(crate) const NO_TRANSPORT_HEADLINE: &str = "This browser can't talk to USB devices";

/// No transport: this build (or this browser) cannot reach a USB port at
/// all.
///
/// Said out loud, because a section with no way to connect and no
/// explanation reads as "you have no way in" — a different and wrong
/// claim.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn UnavailableNote() -> Element {
    rsx! {
        div { class: "tw:grid tw:gap-2 tw:rounded-md tw:border tw:border-dashed tw:border-border tw:px-4 tw:py-5",
            p { class: "tw:m-0 tw:text-sm tw:font-semibold tw:text-strong-foreground",
                "{NO_TRANSPORT_HEADLINE}"
            }
            p { class: "tw:m-0 tw:max-w-prose tw:text-xs tw:leading-relaxed tw:text-subtle-foreground",
                "Studio reaches boards over Web Serial, which Chrome, Edge and \
                 other Chromium browsers support. A sim runs anywhere."
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use lpa_studio_core::{WifiAddressReach, add_device_offers};

    use super::*;
    use crate::app::home::ble_reach::BluetoothReach;
    use crate::app::home::reach_note::USB_UNAVAILABLE;

    /// A host build (or a Firefox) has no transport, and the section says
    /// that rather than showing squares that can only fail.
    #[test]
    fn no_transport_says_so_instead_of_showing_squares() {
        assert_eq!(connect_body(false), ConnectBody::NoTransport);
        assert_eq!(connect_body(true), ConnectBody::Squares);
        assert_eq!(
            NO_TRANSPORT_HEADLINE,
            "This browser can't talk to USB devices"
        );
    }

    /// The way forward shows only under a transport that is disabled.
    #[test]
    fn the_way_forward_shows_only_for_a_disabled_transport() {
        let reach = WifiAddressReach {
            available: true,
            connecting: false,
        };
        let offers = |usb: bool, ble: BluetoothReach| add_device_offers(usb, ble, reach);

        // Bluefy: USB disabled (notes: its address), Bluetooth live.
        let [usb, ble, _] = offers(false, BluetoothReach::Ready)
            .try_into()
            .expect("three transports");
        let shown = shown_notes(
            Some(&usb),
            Some(&ble),
            add_slot_notes(false, BluetoothReach::Ready),
        );
        assert_eq!(shown.usb, Some(USB_UNAVAILABLE));
        assert_eq!(shown.ble, None);

        // Chrome: nothing is disabled, nothing is explained.
        let [usb, ble, _] = offers(true, BluetoothReach::Ready)
            .try_into()
            .expect("three transports");
        let shown = shown_notes(
            Some(&usb),
            Some(&ble),
            add_slot_notes(true, BluetoothReach::Ready),
        );
        assert_eq!(shown.usb, None);
        assert_eq!(shown.ble, None);

        // A note for a transport whose offer the tree does not hold has
        // no button to sit under.
        let shown = shown_notes(None, None, add_slot_notes(false, BluetoothReach::Ios));
        assert_eq!(shown.usb, None);
        assert_eq!(shown.ble, None);

        // Brave: Bluetooth disabled, USB live.
        let [usb, ble, _] = offers(true, BluetoothReach::Brave)
            .try_into()
            .expect("three transports");
        let shown = shown_notes(
            Some(&usb),
            Some(&ble),
            add_slot_notes(true, BluetoothReach::Brave),
        );
        assert_eq!(shown.usb, None);
        assert!(shown.ble.is_some());
    }

    /// The hint is the newcomer's alone.
    #[test]
    fn the_welcome_hint_shows_only_for_a_first_visit() {
        assert_eq!(welcome_hint(false), None);
        let hint = welcome_hint(true).expect("a first visit gets the hint");
        assert_eq!(
            format!("{} {}", hint.lead, hint.link),
            "No board? Try an example \u{2193}"
        );
    }

    /// Three squares and the quiet detour under them: the words a walk and a
    /// person read.
    #[test]
    fn the_section_reads_usb_bluetooth_network_and_start_a_board_here() {
        assert_eq!(
            [USB_WORD, BLUETOOTH_WORD, NETWORK_WORD],
            ["USB", "Bluetooth", "Network"]
        );
        assert_eq!(
            crate::app::home::target_pick_popover::SLOT_VERB_LABEL,
            "start a board here"
        );
    }

    #[test]
    fn the_sections_ids_are_the_ones_the_walks_and_the_hint_use() {
        assert_eq!(CONNECT_BOARD_ID, "home-connect-board");
        assert_eq!(HOME_EXAMPLES_ID, "home-examples");
        assert_eq!(UiHomeSection::ConnectBoard.label(), "Connect a board");
    }
}
