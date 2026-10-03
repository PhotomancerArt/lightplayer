//! The last access list each device answered, cached for the panel, and
//! the changes the panel asks for.
//!
//! The device is the source of truth: "Who has access" is read from the
//! board (`AccessList`, P1) on every USB connect and every edit-tier
//! Bluetooth unlock. This cache only lets the panel render while the device
//! is away, and remembers "restart to apply" across a reload. It is keyed by
//! the board's base MAC (`mac:…`), the one identity it keeps from first hello
//! to last, else its uid, and stored by the web edge under
//! `lp.access.device-lists.v1`. A listing never carries a key; what the
//! record adds is the passwords THIS browser set from the panel
//! ([`SetHere`]), so the panel can show them again — a board keeps only a
//! derived key, so a password set anywhere else cannot be shown. They sit in
//! localStorage beside the remembered passwords, under the same threat
//! model.
//!
//! A cache, not a user's data: a document this build cannot read (an older
//! listing shape) reads as empty, and the next connect lists again.

use std::collections::BTreeMap;

use lpc_access::{SALT_BYTES, Tier};
use lpc_wire::server::AccessEntryInfo;
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
    /// Passwords this browser set on the device, while they are still on it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub set_here: Vec<SetHere>,
}

/// A password this browser set from the panel, by the entry it became.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetHere {
    #[serde(with = "lpc_access::base64_bytes")]
    pub salt: [u8; SALT_BYTES],
    pub password: String,
}

/// The password is for showing, never for a log line.
impl core::fmt::Debug for SetHere {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SetHere")
            .field("password", &"<redacted>")
            .finish()
    }
}

impl DeviceAccessRecord {
    /// The password this browser set for `entry`, if it did.
    pub fn password_for(&self, entry: &AccessEntryInfo) -> Option<&str> {
        self.set_here
            .iter()
            .find(|set| set.salt == entry.salt)
            .map(|set| set.password.as_str())
    }
}

/// A change the access panel asks for.
#[derive(Clone, PartialEq, Eq)]
pub enum DeviceAccessChange {
    /// Remove entries (a group's trash can), by their salts.
    Remove { salts: Vec<[u8; SALT_BYTES]> },
    /// Switch Bluetooth. Applies at the next boot, so over USB Studio
    /// restarts the device; over Bluetooth, turning it off is refused
    /// (it would cut the link it came over — "turn off by USB").
    SetBluetooth(bool),
    /// A "Who nearby can…" line: `tier` (Play, or Author = edit) takes this
    /// password, or with `None` anyone nearby can do it
    /// ([`super::two_passwords::plan_password`]).
    SetPassword {
        tier: Tier,
        password: Option<String>,
    },
}

impl core::fmt::Debug for DeviceAccessChange {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Remove { salts } => f
                .debug_struct("Remove")
                .field("count", &salts.len())
                .finish(),
            Self::SetBluetooth(on) => f.debug_tuple("SetBluetooth").field(on).finish(),
            Self::SetPassword { tier, password } => f
                .debug_struct("SetPassword")
                .field("tier", tier)
                .field("password", &password.as_ref().map(|_| "<redacted>"))
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

    /// Store a listing the device just answered, with `set` — a password
    /// this browser just set on it — remembered. A remembered password whose
    /// entry has left the device is forgotten.
    pub fn record(
        &mut self,
        key: &str,
        listing: AccessListing,
        now_secs: f64,
        set: Option<SetHere>,
    ) {
        let before = self.devices.remove(key);
        let restart_pending = before.as_ref().is_some_and(|before| {
            before.restart_pending || before.listing.ble_enabled != listing.ble_enabled
        });
        let mut set_here = before.map(|before| before.set_here).unwrap_or_default();
        set_here.extend(set);
        set_here.retain(|set| listing.entries.iter().any(|entry| entry.salt == set.salt));
        self.devices.insert(
            key.to_string(),
            DeviceAccessRecord {
                listing,
                listed_at: now_secs,
                restart_pending,
                set_here,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn listing(ble_enabled: bool) -> AccessListing {
        AccessListing {
            ble_enabled,
            ..AccessListing::default()
        }
    }

    #[test]
    fn switching_bluetooth_asks_for_a_restart_until_one_is_seen() {
        let mut records = DeviceAccessRecords::default();
        records.record("dev_1", listing(true), 1.0, None);
        assert!(
            !records.get("dev_1").unwrap().restart_pending,
            "a first listing is what the device runs"
        );
        records.record("dev_1", listing(false), 2.0, None);
        assert!(records.get("dev_1").unwrap().restart_pending);
        records.record("dev_1", listing(false), 3.0, None);
        assert!(records.get("dev_1").unwrap().restart_pending, "still");
        assert!(records.note_restarted("dev_1"));
        assert!(!records.get("dev_1").unwrap().restart_pending);
        let back = DeviceAccessRecords::from_json(&records.to_json());
        assert_eq!(back.devices, records.devices);
    }

    #[test]
    fn a_password_set_here_is_kept_while_its_entry_is_on_the_device() {
        let entry = AccessEntryInfo::from(&lpc_access::SecretEntry::from_password(
            "Play password",
            Tier::Play,
            b"x",
            [4; 16],
            1,
        ));
        let mut with = listing(true);
        with.entries.push(entry.clone());
        let mut records = DeviceAccessRecords::default();
        let set = SetHere {
            salt: [4; 16],
            password: "camp-glow-17".to_string(),
        };
        records.record("dev_1", with.clone(), 1.0, Some(set));
        assert_eq!(
            records.get("dev_1").unwrap().password_for(&entry),
            Some("camp-glow-17")
        );
        let back = DeviceAccessRecords::from_json(&records.to_json());
        assert_eq!(back.devices, records.devices);
        records.record("dev_1", with, 2.0, None);
        assert!(records.get("dev_1").unwrap().password_for(&entry).is_some());
        records.record("dev_1", listing(true), 3.0, None);
        assert!(
            records.get("dev_1").unwrap().set_here.is_empty(),
            "the entry left"
        );
        assert!(!format!("{:?}", records.get("dev_1")).contains("camp"));
    }
}
