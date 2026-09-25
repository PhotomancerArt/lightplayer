//! The last access list each device answered, cached for the panel, and
//! the changes the panel asks for.
//!
//! The device is the source of truth: "Who has access" is read from the
//! board (`AccessList`, P1) on every USB connect and every edit-tier
//! Bluetooth unlock. This cache only lets the panel render while the device
//! is away, and remembers "restart to apply" across a reload. It is keyed by
//! the board's base MAC (`mac:…`), the one identity it keeps from first hello
//! to last, else its uid, and stored by the web edge under
//! `lp.access.device-lists.v1`. It holds no key: a listing never carries one.

use std::collections::BTreeMap;

use lpc_access::{SALT_BYTES, Tier};
use serde::{Deserialize, Serialize};

use super::device_access_ops::AccessListing;

/// Every device's last listing, by base MAC (`mac:…`), else uid.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DeviceAccessRecords {
    /// Format version of this document; always 1.
    pub version: u32,
    pub devices: BTreeMap<String, DeviceAccessRecord>,
}

/// One device's list, as it last answered.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceAccessRecord {
    pub listing: AccessListing,
    /// When it answered, epoch seconds.
    pub listed_at: f64,
    /// The stored Bluetooth switch moved since the device last booted: the
    /// board reads it once at boot, so the panel says "restart to apply"
    /// until a restart is seen.
    #[serde(default)]
    pub restart_pending: bool,
}

/// A change the access panel asks for.
#[derive(Clone, PartialEq, Eq)]
pub enum DeviceAccessChange {
    /// Remove one entry (the trash can), by its salt.
    Remove { salt: [u8; SALT_BYTES] },
    /// Switch "Anyone nearby can play".
    SetOpen(bool),
    /// Switch Bluetooth. Applies at the next boot, so over USB Studio
    /// restarts the device; over Bluetooth, turning it off is refused
    /// (it would cut the link it came over — "turn off by USB").
    SetBluetooth(bool),
    /// Add a device password (a shared one): PBKDF2 at the default cost, a
    /// fresh random salt, kind `password`.
    AddPassword {
        label: String,
        tier: Tier,
        password: String,
    },
}

/// A password to install in the open project's sidecar, before its key is
/// derived.
#[derive(Clone, PartialEq, Eq)]
pub struct NewSecret {
    pub label: String,
    pub tier: Tier,
    pub password: String,
}

impl core::fmt::Debug for NewSecret {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("NewSecret")
            .field("label", &self.label)
            .field("tier", &self.tier)
            .field("password", &"<redacted>")
            .finish()
    }
}

impl core::fmt::Debug for DeviceAccessChange {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Remove { .. } => f.write_str("Remove"),
            Self::SetOpen(open) => f.debug_tuple("SetOpen").field(open).finish(),
            Self::SetBluetooth(on) => f.debug_tuple("SetBluetooth").field(on).finish(),
            Self::AddPassword { label, tier, .. } => f
                .debug_struct("AddPassword")
                .field("label", label)
                .field("tier", tier)
                .field("password", &"<redacted>")
                .finish(),
        }
    }
}

impl DeviceAccessRecords {
    pub fn from_json(json: &str) -> Self {
        serde_json::from_str::<Self>(json).unwrap_or_default()
    }

    pub fn to_json(&self) -> String {
        let mut doc = self.clone();
        doc.version = 1;
        serde_json::to_string(&doc).unwrap_or_default()
    }

    pub fn get(&self, key: &str) -> Option<&DeviceAccessRecord> {
        self.devices.get(key)
    }

    /// Store a listing the device just answered.
    pub fn record(&mut self, key: &str, listing: AccessListing, now_secs: f64) {
        let restart_pending = self.devices.get(key).is_some_and(|before| {
            before.restart_pending || before.listing.ble_enabled != listing.ble_enabled
        });
        self.devices.insert(
            key.to_string(),
            DeviceAccessRecord {
                listing,
                listed_at: now_secs,
                restart_pending,
            },
        );
    }

    /// The device booted since: its Bluetooth is now what is stored.
    pub fn note_restarted(&mut self, key: &str) -> bool {
        match self.devices.get_mut(key) {
            Some(record) if record.restart_pending => {
                record.restart_pending = false;
                true
            }
            _ => false,
        }
    }

    pub fn forget(&mut self, key: &str) {
        self.devices.remove(key);
    }
}

/// Validate a device password before it is derived and sent.
pub fn check_new_password(label: &str, password: &str) -> Result<(), String> {
    if label.trim().is_empty() {
        return Err("give the password a name".to_string());
    }
    if password.is_empty() {
        return Err("type a password".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listing(ble_enabled: bool) -> AccessListing {
        AccessListing {
            ble_enabled,
            open: false,
            entries: Vec::new(),
        }
    }

    #[test]
    fn switching_bluetooth_asks_for_a_restart_until_one_is_seen() {
        let mut records = DeviceAccessRecords::default();
        records.record("dev_1", listing(true), 1.0);
        assert!(
            !records.get("dev_1").unwrap().restart_pending,
            "a first listing is what the device runs"
        );
        records.record("dev_1", listing(false), 2.0);
        assert!(records.get("dev_1").unwrap().restart_pending);
        records.record("dev_1", listing(false), 3.0);
        assert!(records.get("dev_1").unwrap().restart_pending, "still");
        assert!(records.note_restarted("dev_1"));
        assert!(!records.get("dev_1").unwrap().restart_pending);
        let back = DeviceAccessRecords::from_json(&records.to_json());
        assert_eq!(back.devices, records.devices);
    }

    #[test]
    fn a_password_needs_a_name_and_a_password() {
        assert!(check_new_password("friends", "x").is_ok());
        assert!(check_new_password(" ", "x").is_err());
        assert!(check_new_password("friends", "").is_err());
    }
}
