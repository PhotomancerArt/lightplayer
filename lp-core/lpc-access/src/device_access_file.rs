//! The device store: root `/.lp/access.json`.

use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

use crate::access_file_error::AccessFileError;
use crate::open_to::OpenTo;
use crate::secret_entry::{SALT_BYTES, SecretEntry, SecretEntryV1, read_version, validate_secrets};

/// The device's own access settings, at the root of its filesystem.
///
/// It holds the device's secrets — browser keys, the account key, typed
/// passwords, each a [`SecretEntry`] of its [`crate::SecretKind`] — and two
/// settings:
///
/// - `bleEnabled` — whether the BLE link may start.
/// - `open` — what anyone nearby holds with no password ([`OpenTo`]):
///   nobody, play, or play and edit.
///
/// **Bluetooth on and open to anyone nearby, by default — for now.** A
/// device with no store is [`Self::fresh`]: BLE on, open at edit, no
/// secrets. Development and alpha testing want public access ("like WLED");
/// the default is one constant to change later. A store that fails to parse
/// reads as [`Self::locked`] instead (BLE off, open to nobody; the caller
/// logs the error): damage only ever takes access away.
///
/// **Two readers since over-the-air updates.** The engine (the server)
/// reads and writes the whole file; the split image's **core** reads
/// `secrets` and `open` for its update session's login and access (doors
/// #14). After an update or a rollback, a core may read a file a newer or
/// older engine wrote, so changes to `secrets` and `open` stay **additive**
/// — a core that cannot read the file reads it as [`Self::locked`] — and no
/// firmware migrates the file on an unconfirmed trial boot
/// (`lpa_server::access_store::may_migrate_device_store`).
///
/// Version 3 (this shape) made `open` an [`OpenTo`]. A version-2 file still
/// reads — `open: true` is play, `false` is nobody, so an existing board
/// keeps exactly the access it had — and so does a version-1 file (its
/// entries become `kind: password` with no time). Both are written back as
/// version 3.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeviceAccessFile {
    /// Format version; always [`DeviceAccessFile::VERSION`] when written.
    #[cfg_attr(feature = "schema-gen", schemars(range(min = 3, max = 3)))]
    pub version: u32,
    /// The device's secrets.
    pub secrets: Vec<SecretEntry>,
    /// Whether the BLE link may start at all.
    pub ble_enabled: bool,
    /// What an untrusted link holds without logging in.
    pub open: OpenTo,
}

/// The version-2 file shape, read and converted, never written.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DeviceAccessFileV2 {
    /// Checked by `read_version` before this parse.
    #[serde(rename = "version")]
    _version: u32,
    secrets: Vec<SecretEntry>,
    ble_enabled: bool,
    open: bool,
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

/// A v1/v2 `open` flag: it only ever granted play.
fn open_from_flag(open: bool) -> OpenTo {
    if open { OpenTo::Play } else { OpenTo::Nobody }
}

impl DeviceAccessFile {
    /// The format version this build writes.
    pub const VERSION: u32 = 3;

    /// Absolute path of the device store on the device filesystem.
    pub const PATH: &'static str = "/.lp/access.json";

    /// What anyone nearby holds on a device with no store: play and edit,
    /// for now. The one place the default lives.
    pub const FRESH_OPEN: OpenTo = OpenTo::Edit;

    /// The state a device without a store is in: BLE on, open to anyone
    /// nearby at [`Self::FRESH_OPEN`], no secrets.
    #[must_use]
    pub fn fresh() -> Self {
        Self {
            version: Self::VERSION,
            secrets: Vec::new(),
            ble_enabled: true,
            open: Self::FRESH_OPEN,
        }
    }

    /// The state a device with a damaged store is in: BLE off, open to
    /// nobody, no secrets.
    #[must_use]
    pub fn locked() -> Self {
        Self {
            ble_enabled: false,
            open: OpenTo::Nobody,
            ..Self::fresh()
        }
    }

    /// Parse and validate the file's bytes, version 1, 2 or 3.
    pub fn from_json(bytes: &[u8]) -> Result<Self, AccessFileError> {
        let file = match read_version(bytes)? {
            Self::VERSION => serde_json::from_slice::<Self>(bytes).map_err(malformed)?,
            2 => {
                let v2: DeviceAccessFileV2 = serde_json::from_slice(bytes).map_err(malformed)?;
                Self {
                    version: Self::VERSION,
                    secrets: v2.secrets,
                    ble_enabled: v2.ble_enabled,
                    open: open_from_flag(v2.open),
                }
            }
            1 => {
                let v1: DeviceAccessFileV1 = serde_json::from_slice(bytes).map_err(malformed)?;
                Self {
                    version: Self::VERSION,
                    secrets: v1.secrets.into_iter().map(SecretEntry::from).collect(),
                    ble_enabled: v1.ble_enabled,
                    open: open_from_flag(v1.open),
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
    /// [`crate::MAX_SECRETS_PER_FILE`] is refused and nothing changes, and
    /// so is an all-zero salt: the salt is a secure link's key id, and all
    /// zero is the anonymous key's ([`crate::key_lookup`]). Only adding is
    /// refused; a stored file is read as it is.
    pub fn upsert_secret(&mut self, entry: SecretEntry) -> Result<(), AccessFileError> {
        if entry.iterations == 0 {
            return Err(AccessFileError::ZeroIterations { label: entry.label });
        }
        if entry.salt == [0; SALT_BYTES] {
            return Err(AccessFileError::ZeroSalt { label: entry.label });
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

    /// A version-2 store exactly as the v2 writer produced it: one browser
    /// key (salt 7s, 1 iteration) and the play-only `open` flag.
    const V2_STORE: &str = "{\"version\":2,\"secrets\":[{\"label\":\"Yona's MacBook\",\
        \"kind\":\"browser\",\"tier\":\"edit\",\"salt\":\"BwcHBwcHBwcHBwcHBwcHBw==\",\
        \"iterations\":1,\"k\":\"pZ2gub4bSR+JKUDoo7R8TI1TQxtWxa3rc2kPufJ978E=\",\
        \"addedAt\":1790000000}],\"bleEnabled\":true,\"open\":true}";

    #[test]
    fn default_is_locked_and_fresh_is_open_with_bluetooth_on() {
        let locked = DeviceAccessFile::default();
        assert!(!locked.ble_enabled);
        assert_eq!(locked.open, OpenTo::Nobody);
        assert!(locked.secrets.is_empty());
        assert_eq!(locked.version, 3);

        let fresh = DeviceAccessFile::fresh();
        assert!(fresh.ble_enabled);
        assert_eq!(
            fresh.open,
            OpenTo::Edit,
            "a new board is open to anyone nearby, for now"
        );
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
            open: OpenTo::Play,
        };
        let json = file.to_json().unwrap();
        assert!(json.starts_with("{\"version\":3,"), "{json}");
        assert!(json.contains("\"bleEnabled\":true"), "{json}");
        assert!(json.contains("\"open\":\"play\""), "{json}");
        assert_eq!(DeviceAccessFile::from_json(json.as_bytes()).unwrap(), file);
    }

    #[test]
    fn a_v1_store_reads_as_v3_and_writes_back_as_v3() {
        let file = DeviceAccessFile::from_json(V1_STORE.as_bytes()).unwrap();
        assert_eq!(file.version, 3);
        assert!(file.ble_enabled);
        assert_eq!(file.open, OpenTo::Nobody);
        let expected =
            SecretEntry::from_password("account default", Tier::Edit, b"hunter2", [9u8; 16], 3);
        assert_eq!(file.secrets, vec![expected]);
        assert_eq!(file.secrets[0].kind, SecretKind::Password);
        assert_eq!(file.secrets[0].added_at, None);

        let json = file.to_json().unwrap();
        assert!(json.starts_with("{\"version\":3,"), "{json}");
        assert!(json.contains("\"kind\":\"password\""), "{json}");
    }

    /// An existing board keeps exactly the access it had: v2's `open: true`
    /// only ever granted play, and still does.
    #[test]
    fn a_v2_store_keeps_its_access_and_writes_back_as_v3() {
        for (flag, open) in [("true", OpenTo::Play), ("false", OpenTo::Nobody)] {
            let json = V2_STORE.replace("\"open\":true", &alloc::format!("\"open\":{flag}"));
            let file = DeviceAccessFile::from_json(json.as_bytes()).unwrap();
            assert_eq!(file.version, 3);
            assert_eq!(file.open, open);
            assert_eq!(file.secrets.len(), 1);
            assert_eq!(file.secrets[0].kind, SecretKind::Browser);
            assert_eq!(file.secrets[0].added_at, Some(1_790_000_000));
            let back = file.to_json().unwrap();
            assert!(back.starts_with("{\"version\":3,"), "{back}");
            assert_eq!(DeviceAccessFile::from_json(back.as_bytes()).unwrap(), file);
        }
    }

    #[test]
    fn a_v3_open_is_a_word_not_a_flag() {
        let json = br#"{"version":3,"secrets":[],"bleEnabled":true,"open":true}"#;
        assert!(matches!(
            DeviceAccessFile::from_json(json),
            Err(AccessFileError::Malformed(_))
        ));
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
            &b"{\"version\":3,\"secrets\":[],\"bleEnabled\":true}"[..],
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
            DeviceAccessFile::from_json(b"{\"version\":4,\"secrets\":[],\"new\":1}"),
            Err(AccessFileError::UnsupportedVersion(4))
        );
    }

    #[test]
    fn an_all_zero_salt_is_refused_on_add() {
        let mut file = DeviceAccessFile::fresh();
        let entry = SecretEntry::from_password("anon", Tier::Play, b"k", [0; 16], 1);
        assert_eq!(
            file.upsert_secret(entry),
            Err(AccessFileError::ZeroSalt {
                label: "anon".into()
            })
        );
        assert!(file.secrets.is_empty());
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
