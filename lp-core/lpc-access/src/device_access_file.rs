//! The device store: root `/.lp/access.json`.

use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

use crate::access_file_error::AccessFileError;
use crate::secret_entry::{SALT_BYTES, SecretEntry, SecretEntryV1, read_version, validate_secrets};

/// The device's own access settings, at the root of its filesystem.
///
/// It holds the device's secrets — browser keys, the account key, typed
/// passwords, each a [`SecretEntry`] of its [`crate::SecretKind`] — and two
/// switches:
///
/// - `bleEnabled` — whether the BLE link may start.
/// - `open` — "open, no password" is explicit (D17): it grants **play**
///   to an untrusted link without login, and never edit.
///
/// **Bluetooth on, locked, by default.** A device with no store is
/// [`Self::fresh`]: BLE on, not open, no secrets — nobody can do anything
/// over the radio until a key is added over USB, but the radio is ready
/// for it. A store that fails to parse reads as [`Self::locked`] instead
/// (BLE off; the caller logs the error): damage only ever takes access
/// away.
///
/// Version 2 (this shape) added each entry's `kind` and `addedAt`. A
/// version-1 file still reads — its entries become `kind: password` with
/// no time — and is written back as version 2.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeviceAccessFile {
    /// Format version; always [`DeviceAccessFile::VERSION`] when written.
    #[cfg_attr(feature = "schema-gen", schemars(range(min = 2, max = 2)))]
    pub version: u32,
    /// The device's secrets.
    pub secrets: Vec<SecretEntry>,
    /// Whether the BLE link may start at all.
    pub ble_enabled: bool,
    /// Whether an untrusted link holds play without logging in.
    pub open: bool,
}

/// The version-1 file shape, read and converted, never written.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DeviceAccessFileV1 {
    /// Checked by `read_version` before this parse.
    #[serde(rename = "version")]
    _version: u32,
    secrets: Vec<SecretEntryV1>,
    ble_enabled: bool,
    open: bool,
}

impl DeviceAccessFile {
    /// The format version this build writes.
    pub const VERSION: u32 = 2;

    /// Absolute path of the device store on the device filesystem.
    pub const PATH: &'static str = "/.lp/access.json";

    /// The state a device without a store is in: BLE on, locked (not
    /// open), no secrets. The radio is up, and nothing gets past it until
    /// a key is added over USB.
    #[must_use]
    pub fn fresh() -> Self {
        Self {
            version: Self::VERSION,
            secrets: Vec::new(),
            ble_enabled: true,
            open: false,
        }
    }

    /// The state a device with a damaged store is in: BLE off, locked, no
    /// secrets.
    #[must_use]
    pub fn locked() -> Self {
        Self {
            ble_enabled: false,
            ..Self::fresh()
        }
    }

    /// Parse and validate the file's bytes, version 1 or 2.
    pub fn from_json(bytes: &[u8]) -> Result<Self, AccessFileError> {
        let file = match read_version(bytes)? {
            Self::VERSION => serde_json::from_slice::<Self>(bytes).map_err(malformed)?,
            1 => {
                let v1: DeviceAccessFileV1 = serde_json::from_slice(bytes).map_err(malformed)?;
                Self {
                    version: Self::VERSION,
                    secrets: v1.secrets.into_iter().map(SecretEntry::from).collect(),
                    ble_enabled: v1.ble_enabled,
                    open: v1.open,
                }
            }
            other => return Err(AccessFileError::UnsupportedVersion(other)),
        };
        validate_secrets(&file.secrets)?;
        Ok(file)
    }

    /// Serialize to the file's bytes, always at [`Self::VERSION`].
    pub fn to_json(&self) -> Result<String, AccessFileError> {
        let current = Self {
            version: Self::VERSION,
            ..self.clone()
        };
        serde_json::to_string(&current).map_err(malformed)
    }

    /// Add `entry`, or replace the entry with the same salt (a holder uses
    /// one salt everywhere, so the same salt is the same holder — this is
    /// how a rename re-labels). A new entry past
    /// [`crate::MAX_SECRETS_PER_FILE`] is refused and nothing changes.
    pub fn upsert_secret(&mut self, entry: SecretEntry) -> Result<(), AccessFileError> {
        if entry.iterations == 0 {
            return Err(AccessFileError::ZeroIterations { label: entry.label });
        }
        if let Some(existing) = self.secrets.iter_mut().find(|s| s.salt == entry.salt) {
            *existing = entry;
            return Ok(());
        }
        if self.secrets.len() >= crate::MAX_SECRETS_PER_FILE {
            return Err(AccessFileError::TooManySecrets(self.secrets.len() + 1));
        }
        self.secrets.push(entry);
        Ok(())
    }

    /// Drop the entry with `salt`. Returns whether there was one; a salt
    /// that is not there is not an error.
    pub fn remove_secret(&mut self, salt: &[u8; SALT_BYTES]) -> bool {
        let before = self.secrets.len();
        self.secrets.retain(|s| &s.salt != salt);
        self.secrets.len() != before
    }
}

impl Default for DeviceAccessFile {
    /// [`Self::locked`]: the default value is the safe state. A MISSING
    /// store is [`Self::fresh`]; that is the reader's decision, not this
    /// type's.
    fn default() -> Self {
        Self::locked()
    }
}

fn malformed(error: serde_json::Error) -> AccessFileError {
    AccessFileError::Malformed(alloc::format!("{error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secret_kind::SecretKind;
    use crate::tier::Tier;
    use alloc::vec;

    /// A version-1 store exactly as the v1 writer produced it: the account
    /// default from `round_trips` below (`hunter2`, salt 9s, 3 iterations).
    const V1_STORE: &str = "{\"version\":1,\"secrets\":[{\"label\":\"account default\",\
        \"tier\":\"edit\",\"salt\":\"CQkJCQkJCQkJCQkJCQkJCQ==\",\"iterations\":3,\
        \"k\":\"pZ2gub4bSR+JKUDoo7R8TI1TQxtWxa3rc2kPufJ978E=\"}],\
        \"bleEnabled\":true,\"open\":false}";

    #[test]
    fn default_is_locked_and_fresh_has_bluetooth_on() {
        let locked = DeviceAccessFile::default();
        assert!(!locked.ble_enabled);
        assert!(!locked.open);
        assert!(locked.secrets.is_empty());
        assert_eq!(locked.version, 2);

        let fresh = DeviceAccessFile::fresh();
        assert!(fresh.ble_enabled);
        assert!(!fresh.open);
        assert!(fresh.secrets.is_empty());
    }

    #[test]
    fn round_trips() {
        let file = DeviceAccessFile {
            version: DeviceAccessFile::VERSION,
            secrets: vec![
                SecretEntry::from_password("account default", Tier::Edit, b"hunter2", [9u8; 16], 3),
                SecretEntry::from_password("Yona's MacBook", Tier::Edit, b"key", [7u8; 16], 1)
                    .with_kind(SecretKind::Browser)
                    .with_added_at(1_790_000_000),
            ],
            ble_enabled: true,
            open: false,
        };
        let json = file.to_json().unwrap();
        assert!(json.starts_with("{\"version\":2,"), "{json}");
        assert!(json.contains("\"bleEnabled\":true"), "{json}");
        assert!(json.contains("\"open\":false"), "{json}");
        assert_eq!(DeviceAccessFile::from_json(json.as_bytes()).unwrap(), file);
    }

    #[test]
    fn a_v1_store_reads_as_v2_and_writes_back_as_v2() {
        let file = DeviceAccessFile::from_json(V1_STORE.as_bytes()).unwrap();
        assert_eq!(file.version, 2);
        assert!(file.ble_enabled);
        assert!(!file.open);
        let expected =
            SecretEntry::from_password("account default", Tier::Edit, b"hunter2", [9u8; 16], 3);
        assert_eq!(file.secrets, vec![expected]);
        assert_eq!(file.secrets[0].kind, SecretKind::Password);
        assert_eq!(file.secrets[0].added_at, None);

        let json = file.to_json().unwrap();
        assert!(json.starts_with("{\"version\":2,"), "{json}");
        assert!(json.contains("\"kind\":\"password\""), "{json}");
    }

    #[test]
    fn a_v1_store_with_a_v2_field_is_malformed() {
        let json = V1_STORE.replace(
            "\"tier\":\"edit\"",
            "\"kind\":\"browser\",\"tier\":\"edit\"",
        );
        assert!(matches!(
            DeviceAccessFile::from_json(json.as_bytes()),
            Err(AccessFileError::Malformed(_))
        ));
    }

    #[test]
    fn every_field_is_required() {
        // A store that forgot `open` must not silently read as locked-or-not:
        // it is refused, and the caller treats a refused store as locked.
        for json in [
            &b"{\"version\":2,\"secrets\":[],\"bleEnabled\":true}"[..],
            &b"{\"version\":1,\"secrets\":[],\"bleEnabled\":true}"[..],
        ] {
            assert!(matches!(
                DeviceAccessFile::from_json(json),
                Err(AccessFileError::Malformed(_))
            ));
        }
    }

    #[test]
    fn other_versions_are_refused_as_versions() {
        assert_eq!(
            DeviceAccessFile::from_json(b"{\"version\":0}"),
            Err(AccessFileError::UnsupportedVersion(0))
        );
        assert_eq!(
            DeviceAccessFile::from_json(b"{\"version\":3,\"secrets\":[],\"new\":1}"),
            Err(AccessFileError::UnsupportedVersion(3))
        );
    }

    #[test]
    fn upsert_adds_replaces_by_salt_and_caps() {
        let mut file = DeviceAccessFile::fresh();
        let key = |label: &str, salt: u8| {
            SecretEntry::from_password(label, Tier::Edit, b"k", [salt; 16], 1)
                .with_kind(SecretKind::Browser)
        };
        file.upsert_secret(key("Chrome on Mac", 1)).unwrap();
        file.upsert_secret(key("Yona's MacBook", 1)).unwrap();
        assert_eq!(file.secrets.len(), 1);
        assert_eq!(file.secrets[0].label, "Yona's MacBook");

        for salt in 2..=16 {
            file.upsert_secret(key("more", salt)).unwrap();
        }
        assert_eq!(file.secrets.len(), crate::MAX_SECRETS_PER_FILE);
        assert_eq!(
            file.upsert_secret(key("one too many", 17)),
            Err(AccessFileError::TooManySecrets(17))
        );
        assert_eq!(file.secrets.len(), crate::MAX_SECRETS_PER_FILE);
        // Replacing is still allowed at the cap.
        file.upsert_secret(key("renamed", 16)).unwrap();
        assert_eq!(file.secrets[15].label, "renamed");
    }

    #[test]
    fn upsert_refuses_zero_iterations() {
        let mut entry = SecretEntry::from_password("x", Tier::Play, b"pw", [1; 16], 1);
        entry.iterations = 0;
        let mut file = DeviceAccessFile::fresh();
        assert!(matches!(
            file.upsert_secret(entry),
            Err(AccessFileError::ZeroIterations { .. })
        ));
        assert!(file.secrets.is_empty());
    }

    #[test]
    fn remove_drops_by_salt_and_ignores_a_missing_one() {
        let mut file = DeviceAccessFile::fresh();
        file.upsert_secret(SecretEntry::from_password(
            "a",
            Tier::Play,
            b"a",
            [1; 16],
            1,
        ))
        .unwrap();
        file.upsert_secret(SecretEntry::from_password(
            "b",
            Tier::Play,
            b"b",
            [2; 16],
            1,
        ))
        .unwrap();
        assert!(file.remove_secret(&[1; 16]));
        assert!(!file.remove_secret(&[1; 16]));
        assert_eq!(file.secrets.len(), 1);
        assert_eq!(file.secrets[0].label, "b");
    }
}
