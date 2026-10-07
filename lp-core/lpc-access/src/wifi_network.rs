//! One saved Wi-Fi network, and the rules a network must meet to be saved.

use alloc::string::String;
use core::fmt;
use serde::{Deserialize, Serialize};

use crate::network_file_error::NetworkFileError;

/// The most bytes an 802.11 network name may hold.
pub const SSID_MAX_BYTES: usize = 32;

/// The fewest characters a WPA2 passphrase may hold.
pub const PASSWORD_MIN_LEN: usize = 8;

/// The most characters a WPA2 passphrase may hold.
pub const PASSWORD_MAX_LEN: usize = 63;

/// The length of a raw WPA2 key spelled in hex.
pub const HEX_KEY_LEN: usize = 64;

/// One saved Wi-Fi network, an entry of [`crate::NetworkFile::networks`].
///
/// `Debug` is written by hand and **never prints the password** — logs on
/// the server and the firmware format with `{:?}`.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct WifiNetwork {
    /// The network name: 1–32 bytes of UTF-8.
    pub ssid: String,
    /// `""` for an open network; else 8–63 printable ASCII characters, or a
    /// raw key of exactly 64 hex digits. Never leaves the board.
    pub password: String,
    /// The network does not broadcast its name: the station asks for it
    /// by name. Omitted when false.
    #[serde(default, skip_serializing_if = "is_false")]
    pub hidden: bool,
}

impl WifiNetwork {
    /// Whether this network has a password (an open network has none).
    #[must_use]
    pub fn has_password(&self) -> bool {
        !self.password.is_empty()
    }

    /// Check the name and the password against the 802.11 / WPA2 rules
    /// ([`validate_ssid`], [`validate_password`]).
    pub fn validate(&self) -> Result<(), NetworkFileError> {
        validate_ssid(&self.ssid)?;
        validate_password(&self.password)
    }
}

impl fmt::Debug for WifiNetwork {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WifiNetwork")
            .field("ssid", &self.ssid)
            .field(
                "password",
                &if self.has_password() {
                    "<set>"
                } else {
                    "<none>"
                },
            )
            .field("hidden", &self.hidden)
            .finish()
    }
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// A network name is 1–32 bytes of UTF-8 (the 802.11 limit is in bytes,
/// not characters).
pub fn validate_ssid(ssid: &str) -> Result<(), NetworkFileError> {
    if ssid.is_empty() {
        return Err(NetworkFileError::SsidEmpty);
    }
    if ssid.len() > SSID_MAX_BYTES {
        return Err(NetworkFileError::SsidTooLong { bytes: ssid.len() });
    }
    Ok(())
}

/// A password is `""` (an open network), 8–63 printable ASCII characters
/// (`0x20..=0x7E`), or exactly 64 ASCII hex digits (a raw WPA2 key). The
/// error names a length or a rule, never the text.
pub fn validate_password(password: &str) -> Result<(), NetworkFileError> {
    let len = password.len();
    if len == 0 {
        return Ok(());
    }
    if len == HEX_KEY_LEN {
        return if password.bytes().all(|b| b.is_ascii_hexdigit()) {
            Ok(())
        } else {
            Err(NetworkFileError::PasswordNotHexKey)
        };
    }
    if !password.bytes().all(|b| (0x20..=0x7E).contains(&b)) {
        return Err(NetworkFileError::PasswordNotPrintable);
    }
    if len < PASSWORD_MIN_LEN {
        return Err(NetworkFileError::PasswordTooShort { len });
    }
    if len > PASSWORD_MAX_LEN {
        return Err(NetworkFileError::PasswordTooLong { len });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;
    use alloc::string::ToString;

    fn network(ssid: &str, password: &str) -> WifiNetwork {
        WifiNetwork {
            ssid: ssid.to_string(),
            password: password.to_string(),
            hidden: false,
        }
    }

    #[test]
    fn ssid_length_is_counted_in_bytes() {
        assert_eq!(validate_ssid(""), Err(NetworkFileError::SsidEmpty));
        assert_eq!(validate_ssid("a"), Ok(()));
        assert_eq!(validate_ssid(&"a".repeat(32)), Ok(()));
        assert_eq!(
            validate_ssid(&"a".repeat(33)),
            Err(NetworkFileError::SsidTooLong { bytes: 33 })
        );
        // 10 four-byte characters = 40 bytes, though only 10 chars.
        assert_eq!(
            validate_ssid(&"🌈".repeat(10)),
            Err(NetworkFileError::SsidTooLong { bytes: 40 })
        );
        // 8 four-byte characters = exactly 32 bytes.
        assert_eq!(validate_ssid(&"🌈".repeat(8)), Ok(()));
        // 31 ASCII + one two-byte character = 33 bytes.
        let edge = format!("{}é", "a".repeat(31));
        assert_eq!(
            validate_ssid(&edge),
            Err(NetworkFileError::SsidTooLong { bytes: 33 })
        );
    }

    #[test]
    fn password_rules_at_their_edges() {
        assert_eq!(validate_password(""), Ok(()), "open network");
        assert_eq!(
            validate_password("1234567"),
            Err(NetworkFileError::PasswordTooShort { len: 7 })
        );
        assert_eq!(validate_password("12345678"), Ok(()));
        assert_eq!(validate_password(&"x".repeat(63)), Ok(()));
        assert_eq!(
            validate_password(&"x".repeat(64)),
            Err(NetworkFileError::PasswordNotHexKey)
        );
        assert_eq!(validate_password(&"aF09".repeat(16)), Ok(()), "64 hex");
        assert_eq!(
            validate_password(&"x".repeat(65)),
            Err(NetworkFileError::PasswordTooLong { len: 65 })
        );
        assert_eq!(validate_password("with space ~!"), Ok(()));
        assert_eq!(
            validate_password("pässwörd-long"),
            Err(NetworkFileError::PasswordNotPrintable)
        );
        assert_eq!(
            validate_password("tab\tinside"),
            Err(NetworkFileError::PasswordNotPrintable)
        );
    }

    #[test]
    fn validate_checks_both_fields() {
        assert_eq!(
            network("lp-walk-net", "correct-horse-42").validate(),
            Ok(())
        );
        assert_eq!(
            network("", "correct-horse-42").validate(),
            Err(NetworkFileError::SsidEmpty)
        );
        assert_eq!(
            network("lp-walk-net", "short").validate(),
            Err(NetworkFileError::PasswordTooShort { len: 5 })
        );
    }

    #[test]
    fn errors_never_quote_the_password() {
        for password in ["shortpw", "pässwörd-long", &"q".repeat(64), &"q".repeat(70)] {
            let error = validate_password(password).unwrap_err();
            let shown = format!("{error} {error:?}");
            assert!(!shown.contains(password), "{shown}");
        }
    }

    #[test]
    fn debug_never_prints_the_password() {
        let shown = format!("{:?}", network("lp-walk-net", "correct-horse-42"));
        assert!(!shown.contains("correct-horse-42"), "{shown}");
        assert!(shown.contains("lp-walk-net"), "{shown}");
        assert!(shown.contains("<set>"), "{shown}");
        let open = format!("{:?}", network("cafe", ""));
        assert!(open.contains("<none>"), "{open}");
    }
}
