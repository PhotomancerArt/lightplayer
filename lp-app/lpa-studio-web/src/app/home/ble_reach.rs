//! Whether "Add over Bluetooth" can work in this browser, and what the add
//! slot says when it cannot (M5 S4, AC7).
//!
//! Never a generic "connect failed": each way Web Bluetooth can be missing
//! has its own sentence and its own way forward, and every one of them is
//! reachable at phone width because it sits in the add slot itself.
//!
//! | reach | what the slot says |
//! |---|---|
//! | `Ready` | the verb |
//! | `Off` | `getAvailability()` answered false: Bluetooth is off or not permitted here |
//! | `Brave` | Brave keeps it behind a flag — the flag's address as select-and-copy text, because a `brave://` link cannot be opened from a page |
//! | `Ios` | iPhone/iPad browsers have none: open lightplayer.app in Bluefy |
//! | `Firefox`, `Safari`, `Unsupported` | not supported here: use Chrome |

use dioxus::prelude::*;

/// Where the Brave flag lives. Shown as text to select and copy — a page
/// cannot open a `brave://` URL, so a link would be a dead end.
pub const BRAVE_BLUETOOTH_FLAG: &str = "brave://flags/#brave-web-bluetooth-api";

/// What this browser can do about Bluetooth.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BleReach {
    /// Not asked yet (the answer is async). The slot draws nothing for it.
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

/// The slot's sentence for a reach that has no verb.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BleReachNote {
    pub text: &'static str,
    /// Text to select and copy (the Brave flag), when there is one.
    pub copy: Option<&'static str>,
}

impl BleReach {
    /// Whether the slot offers "Add over Bluetooth".
    pub fn offers_verb(self) -> bool {
        matches!(self, Self::Ready)
    }

    /// What the slot says instead of the verb.
    pub fn note(self) -> Option<BleReachNote> {
        let text = match self {
            Self::Checking | Self::Ready => return None,
            Self::Off => "Bluetooth is off or not permitted on this device.",
            Self::Brave => {
                return Some(BleReachNote {
                    text: "Brave keeps Bluetooth behind a flag. Turn it on, then restart Brave:",
                    copy: Some(BRAVE_BLUETOOTH_FLAG),
                });
            }
            Self::Ios => {
                "Bluetooth on iPhone and iPad needs the Bluefy browser: open lightplayer.app in Bluefy."
            }
            Self::Firefox => {
                "Firefox doesn't support Web Bluetooth. Use Chrome to add a piece over Bluetooth."
            }
            Self::Safari => {
                "Safari doesn't support Web Bluetooth. Use Chrome to add a piece over Bluetooth."
            }
            Self::Unsupported => "This browser doesn't support Web Bluetooth. Chrome and Edge do.",
        };
        Some(BleReachNote { text, copy: None })
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

    /// Bluefy on an iPhone HAS Web Bluetooth: a supported browser is Ready
    /// whatever family it looks like, and only an explicit "unavailable"
    /// turns it into the off/not-permitted sentence.
    #[test]
    fn a_browser_with_web_bluetooth_is_ready_unless_it_says_otherwise() {
        assert_eq!(reach_from(true, None, Some(BleReach::Ios)), BleReach::Ready);
        assert_eq!(reach_from(true, Some(true), None), BleReach::Ready);
        assert_eq!(reach_from(true, Some(false), None), BleReach::Off);
    }

    /// Every missing-Bluetooth case says something specific, and never a
    /// generic "connect failed".
    #[test]
    fn each_missing_case_has_its_own_way_forward() {
        assert_eq!(
            reach_from(false, None, Some(BleReach::Brave)),
            BleReach::Brave
        );
        assert_eq!(reach_from(false, None, None), BleReach::Unsupported);

        let brave = BleReach::Brave.note().expect("Brave explains itself");
        assert_eq!(brave.copy, Some(BRAVE_BLUETOOTH_FLAG));
        assert!(BleReach::Ios.note().unwrap().text.contains("Bluefy"));
        assert!(BleReach::Firefox.note().unwrap().text.contains("Chrome"));
        assert!(
            BleReach::Off
                .note()
                .unwrap()
                .text
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
                !note.text.to_lowercase().contains("failed"),
                "{}",
                note.text
            );
            assert!(!reach.offers_verb());
        }
        assert!(BleReach::Ready.offers_verb());
        assert_eq!(BleReach::Checking.note(), None);
    }
}
