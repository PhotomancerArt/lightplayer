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
}
