//! The device store: root `/.lp/access.json`.

use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

use crate::access_file_error::AccessFileError;
use crate::secret_entry::{SecretEntry, read_version, validate_secrets};

/// The device's own access settings, at the root of its filesystem.
///
/// It holds the device-only secrets, the account default (as one more
/// secret entry, labelled by Studio), and two switches:
///
/// - `bleEnabled` — BLE is off until this is set over USB (PQ2).
/// - `open` — "open, no password" is explicit (D17): it grants **play**
///   to an untrusted link without login, and never edit.
///
/// **Locked by default.** A missing file reads as [`Self::locked`]: BLE
/// disabled, not open, no secrets. A file that fails to parse reads the
/// same way (the caller logs the error): damage only ever takes access
/// away. Enabling is an edit-tier fs write, made by Studio over USB.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeviceAccessFile {
    /// Format version; always [`DeviceAccessFile::VERSION`].
    #[cfg_attr(feature = "schema-gen", schemars(range(min = 1, max = 1)))]
    pub version: u32,
    /// Device-only secrets and the account default.
    pub secrets: Vec<SecretEntry>,
    /// Whether the BLE link may start at all.
    pub ble_enabled: bool,
    /// Whether an untrusted link holds play without logging in.
    pub open: bool,
}

impl DeviceAccessFile {
    /// The format version this build reads and writes.
    pub const VERSION: u32 = 1;

    /// Absolute path of the device store on the device filesystem.
    pub const PATH: &'static str = "/.lp/access.json";

    /// The state a device without a store is in: BLE off, locked, no
    /// secrets.
    #[must_use]
    pub fn locked() -> Self {
        Self {
            version: Self::VERSION,
            secrets: Vec::new(),
            ble_enabled: false,
            open: false,
        }
    }

    /// Parse and validate the file's bytes.
    pub fn from_json(bytes: &[u8]) -> Result<Self, AccessFileError> {
        let version = read_version(bytes)?;
        if version != Self::VERSION {
            return Err(AccessFileError::UnsupportedVersion(version));
        }
        let file: Self = serde_json::from_slice(bytes)
            .map_err(|error| AccessFileError::Malformed(alloc::format!("{error}")))?;
        validate_secrets(&file.secrets)?;
        Ok(file)
    }

    /// Serialize to the file's bytes.
    pub fn to_json(&self) -> Result<String, AccessFileError> {
        serde_json::to_string(self)
            .map_err(|error| AccessFileError::Malformed(alloc::format!("{error}")))
    }
}

impl Default for DeviceAccessFile {
    /// [`Self::locked`]: the default is the safe state.
    fn default() -> Self {
        Self::locked()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tier::Tier;
    use alloc::vec;

    #[test]
    fn default_is_locked() {
        let locked = DeviceAccessFile::default();
        assert!(!locked.ble_enabled);
        assert!(!locked.open);
        assert!(locked.secrets.is_empty());
        assert_eq!(locked.version, 1);
    }

    #[test]
    fn round_trips() {
        let file = DeviceAccessFile {
            version: DeviceAccessFile::VERSION,
            secrets: vec![SecretEntry::from_password(
                "account default",
                Tier::Edit,
                b"hunter2",
                [9u8; 16],
                3,
            )],
            ble_enabled: true,
            open: false,
        };
        let json = file.to_json().unwrap();
        assert!(json.contains("\"bleEnabled\":true"), "{json}");
        assert!(json.contains("\"open\":false"), "{json}");
        assert_eq!(DeviceAccessFile::from_json(json.as_bytes()).unwrap(), file);
    }

    #[test]
    fn every_field_is_required() {
        // A store that forgot `open` must not silently read as locked-or-not:
        // it is refused, and the caller treats a refused store as locked.
        let json = b"{\"version\":1,\"secrets\":[],\"bleEnabled\":true}";
        assert!(matches!(
            DeviceAccessFile::from_json(json),
            Err(AccessFileError::Malformed(_))
        ));
    }

    #[test]
    fn other_versions_are_refused_as_versions() {
        assert_eq!(
            DeviceAccessFile::from_json(b"{\"version\":0}"),
            Err(AccessFileError::UnsupportedVersion(0))
        );
    }
}
