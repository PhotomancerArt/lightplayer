//! Whether "via Bluetooth" can work in this browser, and what the add slot
//! says under the disabled button when it cannot (M5 S4, AC7; G3 copy).
//!
//! Never a generic "connect failed": each way Web Bluetooth can be missing
//! has its own sentence and its own way forward, and every one of them is
//! reachable at phone width because it sits in the add slot itself.
//!
//! | reach | what the slot says |
//! |---|---|
//! | `Ready` | the button, enabled |
//! | `Off` | `getAvailability()` answered false: turn it on, reload |
//! | `Brave` | Brave keeps it behind a flag — the flag's address as select-and-copy text, because a `brave://` link cannot be opened from a page |
//! | `Ios` | iPhone/iPad browsers have none: a link to Bluefy on the App Store, then this page's address to open there |
//! | `Firefox`, `Safari`, `Unsupported` | needs Chrome or Edge: this page's address to open there |
//!
//! `?ble=emu` polyfills `navigator.bluetooth`, so an emulated link is
//! `Ready` in any browser.

use dioxus::prelude::*;

use crate::app::home::reach_note::{BLUEFY_APP_STORE_URL, ReachCopy, ReachLink, ReachNote};

/// Where the Brave flag lives. Shown as text to select and copy — a page
/// cannot open a `brave://` URL, so a link would be a dead end.
pub const BRAVE_BLUETOOTH_FLAG: &str = "brave://flags/#brave-web-bluetooth-api";

/// What this browser can do about Bluetooth.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BleReach {
    /// Not asked yet (the answer is async). The button is disabled with
    /// nothing under it for the moment this takes.
    Checking,
    Ready,
    /// Web Bluetooth exists but `getAvailability()` said no.
    Off,
    Brave,
    Ios,
    Firefox,
    Safari,
    Unsupported,
}

impl BleReach {
    /// Whether the Bluetooth button is enabled.
    pub fn offers_verb(self) -> bool {
        matches!(self, Self::Ready)
    }

    /// What the slot says under the disabled Bluetooth button, and the way
    /// forward. `None` while ready (nothing to explain) or still checking
    /// (the answer is a moment away).
    pub fn note(self) -> Option<ReachNote> {
        Some(match self {
            Self::Checking | Self::Ready => return None,
            Self::Off => ReachNote::reason(
                "Bluetooth is off or not permitted on this device. Turn it on, then reload this page.",
            ),
            Self::Brave => ReachNote {
                reason: "Brave keeps Bluetooth behind a flag.",
                link: None,
                copy_lead: Some("Turn it on, then restart Brave:"),
                copy: Some(ReachCopy::Text(BRAVE_BLUETOOTH_FLAG)),
            },
            // Every iOS browser is WebKit, which ships no Web Bluetooth —
            // Chrome on iPhone included. Bluefy brings its own.
            Self::Ios => ReachNote {
                reason: "Bluetooth on iPhone and iPad needs the Bluefy browser.",
                link: Some(ReachLink {
                    label: "Get Bluefy on the App Store",
                    href: BLUEFY_APP_STORE_URL,
                }),
                copy_lead: Some("Then open this page in Bluefy:"),
                copy: Some(ReachCopy::ThisPage),
            },
            Self::Firefox | Self::Safari | Self::Unsupported => ReachNote {
                reason: "Bluetooth needs Chrome or Edge.",
                link: None,
                copy_lead: Some("Open this page there:"),
                copy: Some(ReachCopy::ThisPage),
            },
        })
    }
}

/// Ask the browser once, per mounted slot. The host build (tests, stories
/// with no override) has no Bluetooth and answers `Unsupported`.
pub fn use_ble_reach() -> Signal<BleReach> {
    let mut reach = use_signal(|| BleReach::Checking);
    use_future(move || async move {
        reach.set(ask_browser().await);
    });
    reach
}

async fn ask_browser() -> BleReach {
    #[cfg(target_arch = "wasm32")]
    {
        use lpa_link::providers::browser_ble::{BleBrowser, availability};

        let found = availability().await;
        let family = match found.browser {
            BleBrowser::Brave => "brave",
            BleBrowser::Ios => "ios",
            BleBrowser::Firefox => "firefox",
            BleBrowser::Safari => "safari",
            BleBrowser::Other => "other",
        };
        reach_from(
            found.supported,
            found.available,
            BleReach::for_browser(family),
        )
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        reach_from(false, None, BleReach::for_browser("host"))
    }
}

impl BleReach {
    /// The sentence family for a browser WITHOUT Web Bluetooth, by the key
    /// `browser_ble.js`'s `availability()` reports; `None` for any other.
    pub fn for_browser(key: &str) -> Option<Self> {
        match key {
            "brave" => Some(Self::Brave),
            "ios" => Some(Self::Ios),
            "firefox" => Some(Self::Firefox),
            "safari" => Some(Self::Safari),
            _ => None,
        }
    }
}

/// The decision, apart from the browser so it is testable: a browser WITH
/// Web Bluetooth is Ready unless it said Bluetooth is unavailable; one
/// without it gets its family's sentence.
pub fn reach_from(supported: bool, available: Option<bool>, family: Option<BleReach>) -> BleReach {
    match (supported, available) {
        (true, Some(false)) => BleReach::Off,
        (true, _) => BleReach::Ready,
        (false, _) => family.unwrap_or(BleReach::Unsupported),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bluefy on an iPhone HAS Web Bluetooth (and so does any browser under
    /// the `?ble=emu` polyfill): a supported browser is Ready whatever
    /// family it looks like, and only an explicit "unavailable" turns it
    /// into the off/not-permitted sentence.
    #[test]
    fn a_browser_with_web_bluetooth_is_ready_unless_it_says_otherwise() {
        assert_eq!(reach_from(true, None, Some(BleReach::Ios)), BleReach::Ready);
        assert_eq!(
            reach_from(true, None, Some(BleReach::Brave)),
            BleReach::Ready
        );
        assert_eq!(reach_from(true, Some(true), None), BleReach::Ready);
        assert_eq!(reach_from(true, Some(false), None), BleReach::Off);
    }

    /// Every missing-Bluetooth case says something specific and gives a
    /// way to continue — never a generic "connect failed".
    #[test]
    fn each_missing_case_has_its_own_way_forward() {
        assert_eq!(
            reach_from(false, None, Some(BleReach::Brave)),
            BleReach::Brave
        );
        assert_eq!(reach_from(false, None, None), BleReach::Unsupported);

        let brave = BleReach::Brave.note().expect("Brave explains itself");
        assert_eq!(brave.copy, Some(ReachCopy::Text(BRAVE_BLUETOOTH_FLAG)));

        let ios = BleReach::Ios.note().unwrap();
        assert!(ios.reason.contains("Bluefy"));
        assert_eq!(ios.link.map(|link| link.href), Some(BLUEFY_APP_STORE_URL));
        assert_eq!(
            ios.copy,
            Some(ReachCopy::ThisPage),
            "then open THIS page there"
        );

        for reach in [BleReach::Firefox, BleReach::Safari, BleReach::Unsupported] {
            let note = reach.note().unwrap();
            assert_eq!(note.reason, "Bluetooth needs Chrome or Edge.");
            assert_eq!(note.copy, Some(ReachCopy::ThisPage));
        }
        assert!(
            BleReach::Off
                .note()
                .unwrap()
                .reason
                .contains("off or not permitted")
        );
        for reach in [
            BleReach::Off,
            BleReach::Brave,
            BleReach::Ios,
            BleReach::Firefox,
            BleReach::Safari,
            BleReach::Unsupported,
        ] {
            let note = reach.note().expect("a sentence");
            assert!(
                !note.reason.to_lowercase().contains("failed"),
                "{}",
                note.reason
            );
            assert!(!reach.offers_verb());
        }
        assert!(BleReach::Ready.offers_verb());
        assert!(!BleReach::Checking.offers_verb());
        assert_eq!(BleReach::Checking.note(), None);
    }
}
