//! Asking the browser whether "via Bluetooth" can work here, and what the
//! add slot says under the disabled button when it cannot (M5 S4, AC7; G3
//! copy).
//!
//! The answer itself — [`BluetoothReach`], the decision from what the
//! browser said, and the one-sentence reason for each case — is core's
//! (`lpa_studio_core::app::devices::bluetooth_reach`), so the offer tree's
//! `devices/connect-ble` and the app agent read the same thing this slot
//! draws. What stays here is the probe (only the page can ask
//! `navigator.bluetooth`) and the way forward under the reason:
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
pub use lpa_studio_core::BluetoothReach;

use crate::app::home::reach_note::{BLUEFY_APP_STORE_URL, ReachCopy, ReachLink, ReachNote};

/// Where the Brave flag lives. Shown as text to select and copy — a page
/// cannot open a `brave://` URL, so a link would be a dead end.
pub const BRAVE_BLUETOOTH_FLAG: &str = "brave://flags/#brave-web-bluetooth-api";

/// What the slot says under the disabled Bluetooth button: core's reason,
/// and the way forward. `None` while ready (nothing to explain) or still
/// checking (the answer is a moment away).
pub fn ble_reach_note(reach: BluetoothReach) -> Option<ReachNote> {
    let reason = reach.reason()?;
    Some(match reach {
        BluetoothReach::Brave => ReachNote {
            reason,
            link: None,
            copy_lead: Some("Turn it on, then restart Brave:"),
            copy: Some(ReachCopy::Text(BRAVE_BLUETOOTH_FLAG)),
        },
        // Every iOS browser is WebKit, which ships no Web Bluetooth —
        // Chrome on iPhone included. Bluefy brings its own.
        BluetoothReach::Ios => ReachNote {
            reason,
            link: Some(ReachLink {
                label: "Get Bluefy on the App Store",
                href: BLUEFY_APP_STORE_URL,
            }),
            copy_lead: Some("Then open this page in Bluefy:"),
            copy: Some(ReachCopy::ThisPage),
        },
        BluetoothReach::Firefox | BluetoothReach::Safari | BluetoothReach::Unsupported => {
            ReachNote {
                reason,
                link: None,
                copy_lead: Some("Open this page there:"),
                copy: Some(ReachCopy::ThisPage),
            }
        }
        BluetoothReach::Off | BluetoothReach::Checking | BluetoothReach::Ready => {
            ReachNote::reason(reason)
        }
    })
}

/// Ask the browser once, per mounted slot. The host build (tests, stories
/// with no override) has no Bluetooth and answers `Unsupported`.
pub fn use_ble_reach() -> Signal<BluetoothReach> {
    let mut reach = use_signal(|| BluetoothReach::Checking);
    use_future(move || async move {
        reach.set(ask_browser().await);
    });
    reach
}

/// What this browser says about Bluetooth, decided by core. The shell also
/// reports it into core once at startup (`StudioCommand::BluetoothReach`),
/// which is what `devices/connect-ble` reads.
pub async fn ask_browser() -> BluetoothReach {
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
        BluetoothReach::from_probe(
            found.supported,
            found.available,
            BluetoothReach::for_browser(family),
        )
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        BluetoothReach::from_probe(false, None, BluetoothReach::for_browser("host"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every missing-Bluetooth case gives a way to continue — never a
    /// generic "connect failed" — under core's sentence.
    #[test]
    fn each_missing_case_has_its_own_way_forward() {
        let brave = ble_reach_note(BluetoothReach::Brave).expect("Brave explains itself");
        assert_eq!(brave.copy, Some(ReachCopy::Text(BRAVE_BLUETOOTH_FLAG)));

        let ios = ble_reach_note(BluetoothReach::Ios).unwrap();
        assert!(ios.reason.contains("Bluefy"));
        assert_eq!(ios.link.map(|link| link.href), Some(BLUEFY_APP_STORE_URL));
        assert_eq!(
            ios.copy,
            Some(ReachCopy::ThisPage),
            "then open THIS page there"
        );

        for reach in [
            BluetoothReach::Firefox,
            BluetoothReach::Safari,
            BluetoothReach::Unsupported,
        ] {
            let note = ble_reach_note(reach).unwrap();
            assert_eq!(note.reason, "Bluetooth needs Chrome or Edge.");
            assert_eq!(note.copy, Some(ReachCopy::ThisPage));
        }
        let off = ble_reach_note(BluetoothReach::Off).unwrap();
        assert!(off.reason.contains("off or not permitted"));
        assert_eq!(off.copy, None, "the sentence is the way forward");
        for reach in [
            BluetoothReach::Off,
            BluetoothReach::Brave,
            BluetoothReach::Ios,
            BluetoothReach::Firefox,
            BluetoothReach::Safari,
            BluetoothReach::Unsupported,
        ] {
            let note = ble_reach_note(reach).expect("a sentence");
            assert_eq!(Some(note.reason), reach.reason(), "core's own words");
        }
        assert_eq!(ble_reach_note(BluetoothReach::Checking), None);
        assert_eq!(ble_reach_note(BluetoothReach::Ready), None);
    }
}
