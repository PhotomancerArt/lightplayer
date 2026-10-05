//! Why the network file, or a network a client asked to add, was refused.

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
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed { line, column } => write!(
                f,
                "malformed network file (line {line}, column {column})"
            ),
            Self::UnsupportedVersion(version) => {
                write!(f, "unsupported network file version {version}")
            }
            Self::SsidEmpty => f.write_str("the network name is empty"),
            Self::SsidTooLong { bytes } => write!(
                f,
                "the network name is {bytes} bytes; Wi-Fi allows 32"
            ),
            Self::PasswordTooShort { len } => write!(
                f,
                "the password is {len} characters; Wi-Fi needs at least 8 (or none for an open network)"
            ),
            Self::PasswordTooLong { len } => write!(
                f,
                "the password is {len} characters; Wi-Fi allows 63 (or a 64-digit hex key)"
            ),
            Self::PasswordNotPrintable => f.write_str(
                "the password may only use printable ASCII characters (letters, digits, spaces and punctuation)",
            ),
            Self::PasswordNotHexKey => f.write_str(
                "a 64-character password must be a raw key of 64 hex digits; passwords allow 63 characters",
            ),
            Self::TooManyNetworks { max } => write!(
                f,
                "the board keeps at most {max} networks; forget one first"
            ),
            Self::DuplicateSsid => f.write_str("two saved networks have the same name"),
        }
    }
}
