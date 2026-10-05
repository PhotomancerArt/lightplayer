//! Why the network file, or a network a client asked to add, was refused.

use alloc::format;
use alloc::string::{String, ToString};
use core::fmt;

/// A network file that failed to parse, or a Wi-Fi network that breaks the
/// 802.11 / WPA2 rules ([`crate::WifiNetwork::validate`]).
///
/// **No variant carries a password's text** — at most its length — and
/// [`Self::Malformed`] keeps only where the parse failed, never serde's
/// message (which may quote a value). Every variant is safe to log and to
/// send to a client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkFileError {
    /// Not JSON of the expected shape (where the parse stopped).
    Malformed { line: usize, column: usize },
    /// A `version` this build does not speak.
    UnsupportedVersion(u32),
    /// An empty network name.
    SsidEmpty,
    /// A network name longer than 32 bytes (the 802.11 limit is in bytes).
    SsidTooLong { bytes: usize },
    /// A password between 1 and 7 characters (WPA2 needs 8, or none at
    /// all for an open network).
    PasswordTooShort { len: usize },
    /// A password longer than 64 characters.
    PasswordTooLong { len: usize },
    /// An 8–63 character password with a character outside printable ASCII.
    PasswordNotPrintable,
    /// A 64-character password that is not 64 hex digits (64 characters is
    /// how a raw WPA2 key is spelled, and only that).
    PasswordNotHexKey,
    /// A ninth network: a board keeps at most
    /// [`crate::NetworkFile::MAX_NETWORKS`].
    TooManyNetworks { max: usize },
    /// Two saved networks with the same name (a file written by hand; the
    /// board itself replaces a network added again).
    DuplicateSsid,
}

impl fmt::Display for NetworkFileError {
    /// A bare code, not a sentence — cheap on the device, which is the
    /// only thing that calls this `Display` (`lpa-server`'s
    /// `network_store` module: a reply's error text and its log line). A
    /// variant's payload (a byte count, a version number, …) does not
    /// ride along: nothing on the device needs it, and carrying it back
    /// out would cost back the formatting machinery this saved.
    /// [`Self::words`] has the full sentence, for callers that already
    /// hold the concrete value off-device.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Malformed { .. } => "malformed",
            Self::UnsupportedVersion(_) => "version",
            Self::SsidEmpty => "ssidEmpty",
            Self::SsidTooLong { .. } => "ssidTooLong",
            Self::PasswordTooShort { .. } => "passwordTooShort",
            Self::PasswordTooLong { .. } => "passwordTooLong",
            Self::PasswordNotPrintable => "passwordNotPrintable",
            Self::PasswordNotHexKey => "passwordNotHexKey",
            Self::TooManyNetworks { .. } => "tooManyNetworks",
            Self::DuplicateSsid => "duplicateSsid",
        })
    }
}

impl NetworkFileError {
    /// The rule in full words, exactly as `Display` used to say it before
    /// the Wi‑Fi flash-size cut (2026-10): the long sentences moved here,
    /// off the device's `Display` path. Call this only where the
    /// concrete `NetworkFileError` is already in hand with no wire round
    /// trip — Studio's own early validation (`lpa-studio-core`'s
    /// `app::network::wifi_offers::bind_add`, which runs the same
    /// [`crate::validate_ssid`] / [`crate::validate_password`] the device
    /// runs, before a request is ever sent) is the one caller. Nothing on
    /// the device calls it, so it costs the device nothing (dead-code
    /// elimination; `just fw-esp32c6-size-check` is the proof).
    #[must_use]
    pub fn words(&self) -> String {
        match self {
            Self::Malformed { line, column } => {
                format!("malformed network file (line {line}, column {column})")
            }
            Self::UnsupportedVersion(version) => {
                format!("unsupported network file version {version}")
            }
            Self::SsidEmpty => "the network name is empty".to_string(),
            Self::SsidTooLong { bytes } => {
                format!("the network name is {bytes} bytes; Wi-Fi allows 32")
            }
            Self::PasswordTooShort { len } => format!(
                "the password is {len} characters; Wi-Fi needs at least 8 (or none for an open network)"
            ),
            Self::PasswordTooLong { len } => format!(
                "the password is {len} characters; Wi-Fi allows 63 (or a 64-digit hex key)"
            ),
            Self::PasswordNotPrintable => "the password may only use printable ASCII characters (letters, digits, spaces and punctuation)".to_string(),
            Self::PasswordNotHexKey => "a 64-character password must be a raw key of 64 hex digits; passwords allow 63 characters".to_string(),
            Self::TooManyNetworks { max } => {
                format!("the board keeps at most {max} networks; forget one first")
            }
            Self::DuplicateSsid => "two saved networks have the same name".to_string(),
        }
    }

    /// The exact text `lpa-server`'s `network_store::network_add` puts
    /// before the rule's code in a refusal (`"cannot save the network: "`).
    const REFUSAL_PREFIX: &'static str = "cannot save the network: ";

    /// Turn a device's refusal into words a user can read: the device
    /// sends `"cannot save the network: <code>"` (cheap — see
    /// [`Self::fmt`]), and this is the one place that reads a code back
    /// out, for a caller that only has that text, with no concrete
    /// `NetworkFileError` and no wire round trip of its own to re-run.
    /// Studio (`device_network_ops::refusal`) and `lp-cli` (its wifi error
    /// output) are the two callers; **never the device**, which keeps
    /// sending the cheap code — a user-visible string with a bare code in
    /// it is a bug in whichever of those two skipped this call, not a
    /// reason to make the device spell sentences again.
    ///
    /// Text that isn't shaped like a refusal, or a code this build does
    /// not recognize (a newer device than this one), is never shown
    /// verbatim: the unshaped case passes through (it is already a full
    /// sentence — an fs error, a transport error, …), and an unrecognized
    /// code gets a plain, honest fallback instead of leaking raw text the
    /// user cannot act on.
    #[must_use]
    pub fn reword_refusal(text: &str) -> String {
        match text.strip_prefix(Self::REFUSAL_PREFIX) {
            Some(code) => format!(
                "{}{}",
                Self::REFUSAL_PREFIX,
                Self::words_for_code(code).unwrap_or_else(|| {
                    "the board refused it for a reason it did not explain".to_string()
                })
            ),
            None => text.to_string(),
        }
    }

    /// [`Self::reword_refusal`]'s code table. Exact for a code whose rule
    /// carries no number (`ssidEmpty`, `passwordNotPrintable`,
    /// `passwordNotHexKey`, `duplicateSsid`) or whose number is a build
    /// constant the caller already knows (`tooManyNetworks`,
    /// [`crate::NetworkFile::MAX_NETWORKS`]); a generic phrasing, the
    /// specific count dropped, for a code whose number was the caller's
    /// own data (`ssidTooLong`, `passwordTooShort`, `passwordTooLong`) —
    /// those never reach this far in practice, because Studio's own early
    /// validation already holds the concrete value and words it with
    /// [`Self::words`] before a request carrying one is ever sent; this
    /// is the fallback for a caller with no such check of its own
    /// (`lp-cli`), or a race between two clients.
    fn words_for_code(code: &str) -> Option<String> {
        Some(match code {
            "ssidEmpty" => "the network name is empty".to_string(),
            "ssidTooLong" => "the network name is too long; Wi-Fi allows 32 bytes".to_string(),
            "passwordTooShort" => "the password is too short; Wi-Fi needs at least 8 characters (or none for an open network)".to_string(),
            "passwordTooLong" => "the password is too long; Wi-Fi allows 63 characters (or a 64-digit hex key)".to_string(),
            "passwordNotPrintable" => "the password may only use printable ASCII characters (letters, digits, spaces and punctuation)".to_string(),
            "passwordNotHexKey" => "a 64-character password must be a raw key of 64 hex digits; passwords allow 63 characters".to_string(),
            "tooManyNetworks" => format!(
                "the board keeps at most {} networks; forget one first",
                crate::NetworkFile::MAX_NETWORKS
            ),
            "duplicateSsid" => "two saved networks have the same name".to_string(),
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_devices_bare_code_is_reworded_exactly_for_too_many_networks() {
        // The one code whose number is a build constant both ends
        // already know: the reworded text matches `words()` exactly.
        let device_text = format!("cannot save the network: {}", NetworkFileError::TooManyNetworks { max: 8 });
        assert_eq!(device_text, "cannot save the network: tooManyNetworks");
        assert_eq!(
            NetworkFileError::reword_refusal(&device_text),
            format!(
                "cannot save the network: {}",
                NetworkFileError::TooManyNetworks { max: 8 }.words()
            )
        );
    }

    #[test]
    fn every_add_rule_codes_bare_display_reads_back_to_words_with_no_raw_code() {
        for error in [
            NetworkFileError::SsidEmpty,
            NetworkFileError::SsidTooLong { bytes: 40 },
            NetworkFileError::PasswordTooShort { len: 5 },
            NetworkFileError::PasswordTooLong { len: 65 },
            NetworkFileError::PasswordNotPrintable,
            NetworkFileError::PasswordNotHexKey,
            NetworkFileError::TooManyNetworks { max: 8 },
            NetworkFileError::DuplicateSsid,
        ] {
            let code = error.to_string();
            let device_text = format!("cannot save the network: {code}");
            let reworded = NetworkFileError::reword_refusal(&device_text);
            assert!(!reworded.contains(&code), "{reworded} still shows {code}");
            assert_ne!(reworded, device_text, "{code} was not reworded at all");
        }
    }

    #[test]
    fn an_unrecognized_code_falls_back_to_a_plain_sentence_not_the_raw_code() {
        let reworded = NetworkFileError::reword_refusal("cannot save the network: aFutureCode");
        assert!(!reworded.contains("aFutureCode"), "{reworded}");
        assert!(reworded.contains("cannot save the network"), "{reworded}");
    }

    #[test]
    fn text_with_no_refusal_prefix_passes_through_unchanged() {
        let message = "cannot save the network settings: disk full";
        assert_eq!(NetworkFileError::reword_refusal(message), message);
        let message = "the device did not answer: timed out";
        assert_eq!(NetworkFileError::reword_refusal(message), message);
    }
}
