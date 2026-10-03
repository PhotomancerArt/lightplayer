//! [`BluetoothReach`]: whether "via Bluetooth" can work in this browser —
//! a platform fact core knows, like whether there is Web Serial.
//!
//! The browser is asked by the web layer (`navigator.bluetooth` and
//! `getAvailability()` cannot be read from here); what it answered comes in
//! as a [`StudioCommand::BluetoothReach`] and the decision of what that
//! means — and the sentence that says so — lives here, so the offer tree
//! (`devices/connect-ble`) and the app agent read the same answer the add
//! slot draws.
//!
//! Never a generic "connect failed": each way Web Bluetooth can be missing
//! has its own sentence. The web adds the way forward under it (a link to
//! Bluefy, the Brave flag to copy, this page's address).
//!
//! `?ble=emu` polyfills `navigator.bluetooth`, so an emulated link is
//! `Ready` in any browser.
//!
//! [`StudioCommand::BluetoothReach`]: crate::app::studio::studio_command::StudioCommand::BluetoothReach

/// What this browser can do about Bluetooth.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum BluetoothReach {
    /// Not asked yet (the answer is async). The verb is disabled for the
    /// moment this takes.
    #[default]
    Checking,
    Ready,
    /// Web Bluetooth exists but `getAvailability()` said no.
    Off,
    /// Brave keeps it behind a flag.
    Brave,
    /// Every iPhone/iPad browser is WebKit, which ships none.
    Ios,
    Firefox,
    Safari,
    Unsupported,
}

impl BluetoothReach {
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

    /// The decision, apart from the browser so it is testable: a browser
    /// WITH Web Bluetooth is Ready unless it said Bluetooth is unavailable;
    /// one without it gets its family's sentence.
    pub fn from_probe(supported: bool, available: Option<bool>, family: Option<Self>) -> Self {
        match (supported, available) {
            (true, Some(false)) => Self::Off,
            (true, _) => Self::Ready,
            (false, _) => family.unwrap_or(Self::Unsupported),
        }
    }

    /// Whether the Bluetooth verb can be pressed.
    pub fn offers_verb(self) -> bool {
        matches!(self, Self::Ready)
    }

    /// Why the Bluetooth verb is disabled, in one sentence. `None` while
    /// ready (nothing to explain) or still checking (the answer is a moment
    /// away; see [`Self::disabled_reason`]).
    pub fn reason(self) -> Option<&'static str> {
        Some(match self {
            Self::Checking | Self::Ready => return None,
            Self::Off => {
                "Bluetooth is off or not permitted on this device. Turn it on, then reload this page."
            }
            Self::Brave => "Brave keeps Bluetooth behind a flag.",
            Self::Ios => "Bluetooth on iPhone and iPad needs the Bluefy browser.",
            Self::Firefox | Self::Safari | Self::Unsupported => "Bluetooth needs Chrome or Edge.",
        })
    }

    /// What a disabled `devices/connect-ble` says: [`Self::reason`], or,
    /// while the browser has not answered yet, that Studio is asking.
    /// `None` when the verb can be pressed.
    pub fn disabled_reason(self) -> Option<&'static str> {
        match self {
            Self::Checking => Some("Studio is still asking this browser about Bluetooth."),
            other => other.reason(),
        }
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
        use BluetoothReach as R;
        assert_eq!(R::from_probe(true, None, Some(R::Ios)), R::Ready);
        assert_eq!(R::from_probe(true, None, Some(R::Brave)), R::Ready);
        assert_eq!(R::from_probe(true, Some(true), None), R::Ready);
        assert_eq!(R::from_probe(true, Some(false), None), R::Off);
    }

    /// Every missing-Bluetooth case says something specific — never a
    /// generic "connect failed" — and only Ready offers the verb.
    #[test]
    fn each_missing_case_has_its_own_sentence() {
        use BluetoothReach as R;
        assert_eq!(R::from_probe(false, None, Some(R::Brave)), R::Brave);
        assert_eq!(R::from_probe(false, None, None), R::Unsupported);
        assert_eq!(R::for_browser("brave"), Some(R::Brave));
        assert_eq!(R::for_browser("other"), None);

        assert!(R::Ios.reason().unwrap().contains("Bluefy"));
        assert!(R::Off.reason().unwrap().contains("off or not permitted"));
        for reach in [R::Firefox, R::Safari, R::Unsupported] {
            assert_eq!(reach.reason(), Some("Bluetooth needs Chrome or Edge."));
        }
        for reach in [
            R::Off,
            R::Brave,
            R::Ios,
            R::Firefox,
            R::Safari,
            R::Unsupported,
        ] {
            let reason = reach.reason().expect("a sentence");
            assert!(!reason.to_lowercase().contains("failed"), "{reason}");
            assert!(!reach.offers_verb());
            assert_eq!(reach.disabled_reason(), Some(reason));
        }
        assert!(R::Ready.offers_verb());
        assert_eq!(R::Ready.disabled_reason(), None);
        assert!(!R::Checking.offers_verb());
        assert_eq!(R::Checking.reason(), None);
        assert!(
            R::Checking.disabled_reason().is_some(),
            "disabled, and says why"
        );
        assert_eq!(
            R::default(),
            R::Checking,
            "nothing is known before the browser answers"
        );
    }
}
