//! Wi‑Fi stories (Wi‑Fi roadmap M5; the network list, plan P7): the card's
//! Wi‑Fi row, and the three-page popover (the UX spike's 2B) in every
//! state a board can answer — today's firmware (it saves, it can't
//! connect), and M6's states (scan, connecting, connected, a refused
//! password, out of range) ahead of any firmware that produces them,
//! including the test that runs in a just-added network's row.
//!
//! Every one is also captured at the phone width (the story harness's `sm`
//! viewport). Made-up values only (`lp-walk-net` / `correct-horse-42`, the
//! spike's Starlink names).

use dioxus::prelude::*;
use lpa_studio_core::{
    DeviceId, HeardNetwork, LastAttempt, NetworkStatus, OfferArgs, OfferPath, SavedNetworkInfo,
    StationFailure, StationState, UiDeviceWifi, UiOffer, UiWifiTest, WIFI_NETWORK_PARAM,
    WIFI_PASSWORD_PARAM, WifiTestOutcome, WifiTestProgress, WifiTestStep,
};
use lpa_studio_web_story_macros::story;

use crate::app::home::ble_access_stories::{usb_access, usb_card};
use crate::app::home::device_offer_story_fixtures::StoryDeviceCard;
use crate::app::home::wifi_panel::{WifiPage, WifiPanel};

// --- 1 · The row ------------------------------------------------------------

#[story(
    description = "The device card's Connections group over USB, connected: under USB, Bluetooth and Access, the \"Wi‑Fi\" row shows green bars and the network's name (\"Starlink Truck ›\") and opens the Wi‑Fi popover."
)]
fn wifi_row_on_the_card() -> Element {
    rsx! {
        div { class: CARD_FRAME,
            StoryDeviceCard {
                card: usb_card(),
                projects: Vec::new(),
                examples: Vec::new(),
                open_uid: Some("dev000000daqf6dvvqz".to_string()),
                access: Some(usb_access(Some(true), false)),
                wifi: Some(connected_truck()),
                on_action: |_| {},
            }
        }
    }
}

// --- 2 · Page 1, Networks -----------------------------------------------------

#[story(
    description = "Networks, connected (M6): the connected network first — green bars, \"Starlink Truck\", \"Connected · 192.168.1.17\" — then the other saved ones (\"Not in range\", \"Wrong password\" in amber), then \"+ Connect to a network\". The Wi‑Fi switch sits in the header; the Cloud relay switch (\"Lets lightplayer.app reach this board through the cloud.\") at the foot."
)]
fn wifi_networks_connected() -> Element {
    panel(connected_truck(), None, None)
}

#[story(
    description = "Today's firmware (M5), two networks saved: \"Not connected — this firmware can't connect to Wi‑Fi yet. It will after an update.\", each row \"Saved\", no bars. Tap a row for its page."
)]
fn wifi_networks_saved_on_todays_firmware() -> Element {
    panel(
        board(
            StationState::Unsupported,
            vec![saved("lp-walk-net", None), saved("lp-back-office", None)],
        ),
        None,
        None,
    )
}

#[story(
    description = "Just saved on today's firmware: Save goes straight back to the list, and the new network's row says \"Saved · this firmware can't connect to Wi‑Fi yet. It will after an update.\" with Done."
)]
fn wifi_saved_row_on_todays_firmware() -> Element {
    let mut wifi = board(
        StationState::Unsupported,
        vec![saved("lp-walk-net", None), saved("lp-back-office", None)],
    );
    wifi.testing = Some("lp-back-office".to_string());
    panel(wifi, None, None)
}

#[story(
    description = "Just pressed Connect (M6), the test running in the new row: \"Looking for Starlink Apt\" done, \"Checking the password\" now, \"Getting an address\" and \"Reaching lightplayer.app\" to come."
)]
fn wifi_test_running_in_its_row() -> Element {
    let wifi = with_test(
        board(
            StationState::Connecting {
                ssid: "Starlink Apt".to_string(),
            },
            vec![saved("Starlink Home", None), saved("Starlink Apt", None)],
        ),
        UiWifiTest {
            ssid: "Starlink Apt".to_string(),
            relay_step: true,
            progress: WifiTestProgress::Running(WifiTestStep::CheckingPassword),
        },
    );
    panel_with_test(wifi)
}

#[story(
    description = "The in-row test, connected: every step ticked, \"Getting an address · 10.0.0.23\", then \"Connected · good signal · 10.0.0.23\" with Done."
)]
fn wifi_test_connected() -> Element {
    let wifi = board(
        StationState::Connected {
            ssid: "Starlink Apt".to_string(),
            ip: "10.0.0.23".to_string(),
            rssi: -57,
        },
        vec![
            saved("Starlink Home", None),
            saved("Starlink Apt", Some(LastAttempt::Connected)),
        ],
    );
    panel(testing(wifi, "Starlink Apt"), None, None)
}

#[story(
    description = "The in-row test, wrong password: \"Checking the password\" crossed in red, then \"Wrong password · it's saved, but won't connect until the password is changed.\" with Remove (two clicks) and Change password."
)]
fn wifi_test_wrong_password() -> Element {
    let wifi = board(
        StationState::Failed {
            ssid: "Starlink Apt".to_string(),
            reason: StationFailure::WrongPassword,
        },
        vec![
            saved("Starlink Home", None),
            saved("Starlink Apt", Some(LastAttempt::WrongPassword)),
        ],
    );
    panel(testing(wifi, "Starlink Apt"), None, None)
}

#[story(
    description = "The in-row test, out of range: \"Looking for Starlink Truck\" crossed, then \"Not in range · it's saved and connects when it's in range. The board only sees 2.4 GHz networks.\" with Done."
)]
fn wifi_test_not_in_range() -> Element {
    let wifi = board(
        StationState::Failed {
            ssid: "Starlink Truck".to_string(),
            reason: StationFailure::NotFound,
        },
        vec![
            saved("Starlink Home", None),
            saved("Starlink Truck", Some(LastAttempt::NotFound)),
        ],
    );
    panel(testing(wifi, "Starlink Truck"), None, None)
}

#[story(
    description = "The in-row test, connected but lightplayer.app didn't answer (the relay step, M7): \"Connected, no internet · lightplayer.app didn't answer (10.20.4.118). The network may need a sign-in page.\""
)]
fn wifi_test_no_internet() -> Element {
    let wifi = with_test(
        board(
            StationState::Connected {
                ssid: "Ritual Coffee Guest".to_string(),
                ip: "10.20.4.118".to_string(),
                rssi: -58,
            },
            vec![saved("Ritual Coffee Guest", Some(LastAttempt::Connected))],
        ),
        UiWifiTest {
            ssid: "Ritual Coffee Guest".to_string(),
            relay_step: true,
            progress: WifiTestProgress::Done(WifiTestOutcome::NoInternet {
                ip: "10.20.4.118".to_string(),
            }),
        },
    );
    panel_with_test(wifi)
}

#[story(
    description = "Saved networks, none in range (M6): \"Not connected.\", each row \"Not in range\", dim bars."
)]
fn wifi_networks_none_in_range() -> Element {
    let mut wifi = board(
        StationState::NotConnected,
        vec![
            saved("Starlink Home", None),
            saved("Starlink Truck", None),
            saved("Starlink Apt", None),
        ],
    );
    wifi.heard = Some(cafe());
    panel(wifi, None, None)
}

#[story(
    description = "Wi‑Fi and the cloud relay switched off: \"Wi‑Fi is off.\", the rows \"Saved\", both switches off."
)]
fn wifi_off_and_relay_off() -> Element {
    let mut wifi = board(
        StationState::Off,
        vec![saved("Starlink Home", None), saved("Starlink Truck", None)],
    );
    if let Some(status) = wifi.status.as_mut() {
        status.wifi = false;
        status.cloud_relay = false;
    }
    panel(wifi, None, None)
}

// --- 3 · Page 2, Connect to a network ------------------------------------------

#[story(
    description = "Nothing saved, a board that can scan (M6): the popover opens straight on Connect — \"Not connected. Pick the board's network:\", Nearby (strongest first, a lock on the ones that ask for a password, \"· open\" on the rest), \"Other network…\" for a hidden or out-of-range name, and the Cloud relay switch."
)]
fn wifi_nothing_saved_opens_on_connect() -> Element {
    let mut wifi = board(StationState::NotConnected, Vec::new());
    wifi.heard = Some(home());
    panel(wifi, None, None)
}

#[story(
    description = "Nothing saved, today's firmware: it opens on the connect page as \"Add a network by name\" — \"Not set up. This firmware can save a network but can't connect yet — it will after an update.\", \"This firmware can't list networks. Type the name.\", the name and password, and Save."
)]
fn wifi_nothing_saved_on_todays_firmware() -> Element {
    panel(board(StationState::Unsupported, Vec::new()), None, None)
}

#[story(
    description = "Connect to a network from the list (M6): back to Networks at the top left, Nearby without the network already saved."
)]
fn wifi_connect_page() -> Element {
    let mut wifi = board(
        StationState::NotConnected,
        vec![saved("Starlink Home", None)],
    );
    wifi.heard = Some(home());
    panel(wifi, Some(WifiPage::Connect), None)
}

// --- 4 · Page 3, the network ----------------------------------------------------

#[story(
    description = "A picked network's page: its name read-only, the password as dots (Show/Hide; never echoed, cleared once pressed), and Connect."
)]
fn wifi_network_form() -> Element {
    let mut wifi = board(StationState::NotConnected, Vec::new());
    wifi.heard = Some(home());
    panel(
        wifi,
        Some(WifiPage::Form {
            ssid: Some("NETGEAR42".to_string()),
            changing: false,
        }),
        Some(
            OfferArgs::new()
                .with(WIFI_NETWORK_PARAM, "NETGEAR42")
                .with(WIFI_PASSWORD_PARAM, "correct-horse-42"),
        ),
    )
}

#[story(
    description = "Other network…: a typed name (hidden, or not in range) and its password (\"empty if open\")."
)]
fn wifi_other_network_form() -> Element {
    let mut wifi = board(
        StationState::NotConnected,
        vec![saved("Starlink Home", None)],
    );
    wifi.heard = Some(home());
    panel(
        wifi,
        Some(WifiPage::Form {
            ssid: None,
            changing: false,
        }),
        Some(OfferArgs::new().with(WIFI_NETWORK_PARAM, "Starlink Truck")),
    )
}

// --- 5 · A saved network's page --------------------------------------------------

#[story(
    description = "A saved network's page (M6): \"In range · strong signal\", \"Password: saved on the board. It can't be shown.\", Change password, and Forget."
)]
fn wifi_saved_network_page() -> Element {
    let mut wifi = connected_truck();
    if let Some(heard) = wifi.heard.as_mut() {
        heard.push(heard_network("Starlink Home", -48, true));
    }
    panel(
        wifi,
        Some(WifiPage::Network("Starlink Home".to_string())),
        None,
    )
}

#[story(
    description = "A saved network's page with Forget armed (the first click): it is Lasting — the board forgets the network and its password, and only the user can type it again."
)]
fn wifi_saved_network_forget_armed() -> Element {
    let wifi = board(
        StationState::Unsupported,
        vec![saved("lp-walk-net", None), saved("lp-back-office", None)],
    );
    let offers = offers(&wifi);
    rsx! {
        div { class: PANEL_FRAME,
            WifiPanel {
                wifi,
                offers,
                on_action: |_| {},
                on_network: |_| {},
                page_preview: Some(WifiPage::Network("lp-walk-net".to_string())),
                forget_armed_preview: true,
            }
        }
    }
}

#[story(
    description = "A saved network whose password was refused: \"Wrong password — it won't connect until it's changed.\" in amber, Change password leading."
)]
fn wifi_saved_network_wrong_password() -> Element {
    panel(
        connected_truck(),
        Some(WifiPage::Network("Starlink Apt".to_string())),
        None,
    )
}

// --- 6 · Waiting and refusals ------------------------------------------------------

#[story(
    description = "Reading: the popover opened before the board answered — \"Reading…\" and no verbs yet."
)]
fn wifi_reading() -> Element {
    let mut reading = board(StationState::Unsupported, Vec::new());
    reading.status = None;
    reading.reading = true;
    panel(reading, None, None)
}

#[story(
    description = "A Bluetooth link unlocked for play only: the board is not asked, and the popover says what it needs — \"Needs Author access — unlock with an author password.\" No verbs."
)]
fn wifi_needs_author() -> Element {
    panel(UiDeviceWifi::new(DeviceId(7), false), None, None)
}

#[story(
    description = "A ninth network refused by the board, in its words under the list: \"cannot save the network: the board keeps at most 8 networks; forget one first\" — it names the rule, never a password."
)]
fn wifi_refused() -> Element {
    let names: Vec<String> = (1..=8).map(|n| format!("lp-net-{n}")).collect();
    let mut refused = board(
        StationState::Unsupported,
        names.iter().map(|ssid| saved(ssid, None)).collect(),
    );
    refused.error = Some(
        "cannot save the network: the board keeps at most 8 networks; forget one first".to_string(),
    );
    panel(refused, None, None)
}

// --- fixtures ---------------------------------------------------------------

/// The popover in its frame, with the verbs core would publish.
fn panel(wifi: UiDeviceWifi, page: Option<WifiPage>, args: Option<OfferArgs>) -> Element {
    let offers = offers(&wifi);
    rsx! {
        div { class: PANEL_FRAME,
            WifiPanel {
                wifi,
                offers,
                on_action: |_| {},
                on_network: |_| {},
                page_preview: page,
                args_preview: args,
            }
        }
    }
}

/// Core's Wi‑Fi offers for `wifi`, at the story board's prefix.
fn offers(wifi: &UiDeviceWifi) -> Vec<UiOffer> {
    let prefix = OfferPath::parse("devices/mac-a0f26287b48c").expect("a board prefix");
    lpa_studio_core::app::network::wifi_offers(&prefix, wifi)
}

fn board(station: StationState, networks: Vec<SavedNetworkInfo>) -> UiDeviceWifi {
    UiDeviceWifi {
        status: Some(NetworkStatus {
            wifi: true,
            cloud_relay: true,
            networks,
            station,
        }),
        ..UiDeviceWifi::new(DeviceId(7), true)
    }
}

/// The truck: three saved, on the truck's network, the apartment's
/// password refused last time, the house out of range.
fn connected_truck() -> UiDeviceWifi {
    let mut wifi = board(
        StationState::Connected {
            ssid: "Starlink Truck".to_string(),
            ip: "192.168.1.17".to_string(),
            rssi: -41,
        },
        vec![
            saved("Starlink Home", Some(LastAttempt::NotFound)),
            saved("Starlink Truck", Some(LastAttempt::Connected)),
            saved("Starlink Apt", Some(LastAttempt::WrongPassword)),
        ],
    );
    wifi.heard = Some(vec![
        heard_network("Starlink Truck", -41, true),
        heard_network("Pixel_7310", -66, true),
        heard_network("xfinitywifi", -86, false),
    ]);
    wifi
}

/// `wifi` with `ssid` under test in its row.
fn testing(mut wifi: UiDeviceWifi, ssid: &str) -> UiDeviceWifi {
    wifi.testing = Some(ssid.to_string());
    wifi
}

/// `wifi` with `test` under way. A board reports only `connecting` while it
/// tries (the steps inside it are M6's), so a story that shows a later
/// step, or the relay step (M7), hands the panel the test it draws.
fn with_test(wifi: UiDeviceWifi, test: UiWifiTest) -> (UiDeviceWifi, UiWifiTest) {
    (testing(wifi, &test.ssid), test)
}

/// The panel drawing `test` in its row.
fn panel_with_test((wifi, test): (UiDeviceWifi, UiWifiTest)) -> Element {
    let offers = offers(&wifi);
    rsx! {
        div { class: PANEL_FRAME,
            WifiPanel {
                wifi,
                offers,
                on_action: |_| {},
                on_network: |_| {},
                test_preview: Some(test),
            }
        }
    }
}

fn saved(ssid: &str, last: Option<LastAttempt>) -> SavedNetworkInfo {
    SavedNetworkInfo {
        ssid: ssid.to_string(),
        has_password: true,
        hidden: false,
        last,
    }
}

fn heard_network(ssid: &str, rssi: i8, secure: bool) -> HeardNetwork {
    HeardNetwork {
        ssid: ssid.to_string(),
        rssi,
        secure,
    }
}

/// What the board hears at the house.
fn home() -> Vec<HeardNetwork> {
    vec![
        heard_network("Starlink Home", -48, true),
        heard_network("NETGEAR42", -71, true),
        heard_network("xfinitywifi", -77, false),
        heard_network("HP-Print-3F-Officejet Pro 9015", -80, true),
    ]
}

/// What the board hears at a café.
fn cafe() -> Vec<HeardNetwork> {
    vec![
        heard_network("Ritual Coffee Guest", -58, false),
        heard_network("Ritual Staff", -62, true),
    ]
}

/// The detail card's own frame (the access panel stories' width).
const PANEL_FRAME: &str = "tw:m-3 tw:grid tw:w-[min(320px,calc(100vw-24px))] tw:gap-0 tw:overflow-hidden tw:rounded-md tw:text-sm tw:text-muted-foreground ux-glass-panel";

/// One card, at most the roster column's width.
const CARD_FRAME: &str = "tw:grid tw:max-w-[420px] tw:p-3";
