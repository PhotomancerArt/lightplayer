//! Why Connect a board's USB or Bluetooth press came to nothing after the
//! pick, in plain words.
//!
//! The browser's chooser can be answered (a board picked) and the connection
//! still fail: a Bluetooth board that drops the link before it answers, a
//! port another program holds. Before 2026-10-17 that reached the person as
//! nothing at all — a console warning, and the section as it was before the
//! press (`docs/defects/2026-10-17-an-android-phone-connecting-over-bluetooth-panics-the-c6.md`).
//! Now the section says it under its squares (P4, loud when bad), with the
//! way on (P7): press again, or connect it another way.
//!
//! A chooser closed with nothing picked is not a failure, and says nothing.

/// Which square's chooser failed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChooserTransport {
    /// Web Serial's port chooser.
    Usb,
    /// Web Bluetooth's device chooser.
    Bluetooth,
}

/// What Connect a board says under its squares when a connect failed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiChooserFailure {
    /// The square whose press failed.
    pub transport: ChooserTransport,
    /// The sentence, whole: what happened and what to do.
    pub words: String,
}

impl UiChooserFailure {
    /// The words for `transport`'s chooser failing with the browser's
    /// `error` (`GATT Server is disconnected. Cannot retrieve services.`,
    /// `Failed to open serial port.`).
    pub fn new(transport: ChooserTransport, error: &str) -> Self {
        let lower = error.to_ascii_lowercase();
        let words = match transport {
            ChooserTransport::Bluetooth if lower.contains("disconnected") => {
                BLUETOOTH_DROPPED_WORDS.to_string()
            }
            ChooserTransport::Bluetooth => format!(
                "Couldn't connect over Bluetooth ({}). Press Bluetooth to try again.",
                browser_words(error)
            ),
            ChooserTransport::Usb if lower.contains("failed to open") => USB_BUSY_WORDS.to_string(),
            ChooserTransport::Usb => format!(
                "Couldn't open the USB port ({}). Press USB to try again.",
                browser_words(error)
            ),
        };
        Self { transport, words }
    }
}

/// A Bluetooth board that ended the connection before it answered.
pub const BLUETOOTH_DROPPED_WORDS: &str = "The board ended the Bluetooth connection before it answered. Press Bluetooth to try again, or connect it by USB.";

/// A port the browser could not open: almost always another program (a
/// serial monitor, another tab) holding it.
pub const USB_BUSY_WORDS: &str =
    "Couldn't open the USB port. Close anything else using the board, then press USB again.";

/// The browser's own sentence, without its trailing full stop, so it reads
/// inside ours.
fn browser_words(error: &str) -> &str {
    error.trim().trim_end_matches('.')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dropped_bluetooth_connection_says_so_and_what_to_do() {
        let failure = UiChooserFailure::new(
            ChooserTransport::Bluetooth,
            "GATT Server is disconnected. Cannot retrieve services. (Re)connect first with `device.gatt.connect`.",
        );
        assert_eq!(failure.words, BLUETOOTH_DROPPED_WORDS);
        assert_eq!(failure.transport, ChooserTransport::Bluetooth);
    }

    #[test]
    fn another_bluetooth_failure_keeps_the_browsers_words() {
        let failure =
            UiChooserFailure::new(ChooserTransport::Bluetooth, "Connection attempt failed.");
        assert_eq!(
            failure.words,
            "Couldn't connect over Bluetooth (Connection attempt failed). Press Bluetooth to try again."
        );
    }

    #[test]
    fn a_port_that_would_not_open_names_the_likely_cause() {
        let failure = UiChooserFailure::new(ChooserTransport::Usb, "Failed to open serial port.");
        assert_eq!(failure.words, USB_BUSY_WORDS);
    }
}
