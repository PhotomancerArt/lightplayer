//! What this browser wrote to each piece's device store (`/.lp/access.json`).
//!
//! The device store is **write-only** by design: no link, at any tier, can
//! read it back (M3's fs gate). So Studio keeps its own record of the file it
//! last wrote to each device — keyed by the device's registry key (its uid,
//! or `mac:…` before it has one) — and every change rewrites the WHOLE file
//! from that record. The panel lists what is here, and says plainly that the
//! device may hold others written from another browser; saving from here
//! replaces them, which is the recovery path (USB is trusted).
//!
//! Stored in `localStorage` (`lp.ble.device-access.v1`) by the web edge. It
//! carries each secret's derived key `K` — login-equivalent, like the
//! remembered passwords beside it, and accepted for the same reason (PQ8).

use std::collections::BTreeMap;

use lpc_access::{DeviceAccessFile, MAX_SECRETS_PER_FILE, SALT_BYTES, SecretEntry, Tier};
use serde::{Deserialize, Serialize};

/// Every device's record, by registry key.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DeviceAccessRecords {
    /// Format version of this document; always 1.
    pub version: u32,
    pub devices: BTreeMap<String, DeviceAccessRecord>,
}

/// One device's store, as this browser last wrote it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceAccessRecord {
    /// The whole file, exactly as written.
    pub store: DeviceAccessFile,
    /// When it was written, epoch seconds.
    pub written_at: f64,
    /// Whether this write changed `bleEnabled` since the device last
    /// booted: the board reads that switch once at boot, so the panel says
    /// "restart to apply" until a restart is seen.
    #[serde(default)]
    pub restart_pending: bool,
}

/// A change to one device's store, before it is derived and written.
#[derive(Clone, PartialEq, Eq)]
pub enum DeviceAccessChange {
    /// Turn Bluetooth on: locked with a password (its label and tier), or
    /// open (play, no password).
    Enable {
        secret: Option<NewSecret>,
        open: bool,
    },
    /// Turn Bluetooth off. The secrets stay, for the next time.
    Disable,
    /// Add (or replace, by label) one password.
    Add(NewSecret),
    /// Remove one password, by label.
    Revoke { label: String },
    /// Switch "open, no password (play only)".
    SetOpen(bool),
    /// Rewrite the device from this browser's record as it stands, replacing
    /// anything another browser added.
    ReplaceAll,
}

/// A password to install, before its key is derived.
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
            Self::Enable { secret, open } => f
                .debug_struct("Enable")
                .field("secret", secret)
                .field("open", open)
                .finish(),
            Self::Disable => f.write_str("Disable"),
            Self::Add(secret) => f.debug_tuple("Add").field(secret).finish(),
            Self::Revoke { label } => f.debug_struct("Revoke").field("label", label).finish(),
            Self::SetOpen(open) => f.debug_tuple("SetOpen").field(open).finish(),
            Self::ReplaceAll => f.write_str("ReplaceAll"),
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

    /// Store a write that succeeded.
    pub fn record(&mut self, key: &str, store: DeviceAccessFile, now_secs: f64) {
        let before = self.devices.get(key).map(|record| record.store.ble_enabled);
        let restart_pending = match before {
            // The switch moved: the board keeps the old value until it boots.
            Some(before) => {
                before != store.ble_enabled
                    || self.devices.get(key).is_some_and(|r| r.restart_pending)
            }
            // A first write that turns Bluetooth on needs a restart too.
            None => store.ble_enabled,
        };
        self.devices.insert(
            key.to_string(),
            DeviceAccessRecord {
                store,
                written_at: now_secs,
                restart_pending,
            },
        );
    }

    /// The device booted since: its `bleEnabled` is now what was written.
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

/// Apply `change` to the store this browser last wrote (or a locked one),
/// deriving a key for any new password with `derive`. Pure apart from the
/// derivation, so every rule is testable on the host.
pub fn apply_access_change(
    current: Option<&DeviceAccessFile>,
    change: &DeviceAccessChange,
    salt: [u8; SALT_BYTES],
    iterations: u32,
) -> Result<DeviceAccessFile, String> {
    let mut store = current.cloned().unwrap_or_else(DeviceAccessFile::locked);
    let install = |store: &mut DeviceAccessFile, secret: &NewSecret| -> Result<(), String> {
        let label = secret.label.trim();
        if label.is_empty() {
            return Err("give the password a name".to_string());
        }
        if secret.password.is_empty() {
            return Err("type a password".to_string());
        }
        store.secrets.retain(|entry| entry.label != label);
        if store.secrets.len() >= MAX_SECRETS_PER_FILE {
            return Err(format!(
                "a piece holds at most {MAX_SECRETS_PER_FILE} passwords — remove one first"
            ));
        }
        store.secrets.push(SecretEntry::from_password(
            label,
            secret.tier,
            secret.password.as_bytes(),
            salt,
            iterations,
        ));
        Ok(())
    };
    match change {
        DeviceAccessChange::Enable { secret, open } => {
            store.ble_enabled = true;
            store.open = *open;
            match secret {
                Some(secret) => install(&mut store, secret)?,
                // "Open, no password" is an explicit choice (D17): it grants
                // play and never edit. A locked device with no password at
                // all would be one nobody could reach over Bluetooth.
                None if !*open && store.secrets.is_empty() => {
                    return Err("choose a password, or open it without one".to_string());
                }
                None => {}
            }
        }
        DeviceAccessChange::Disable => store.ble_enabled = false,
        DeviceAccessChange::Add(secret) => install(&mut store, secret)?,
        DeviceAccessChange::Revoke { label } => {
            store.secrets.retain(|entry| &entry.label != label);
        }
        DeviceAccessChange::SetOpen(open) => store.open = *open,
        DeviceAccessChange::ReplaceAll => {}
    }
    Ok(store)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secret(label: &str, tier: Tier, password: &str) -> NewSecret {
        NewSecret {
            label: label.to_string(),
            tier,
            password: password.to_string(),
        }
    }

    #[test]
    fn enabling_locks_with_the_password_given() {
        let store = apply_access_change(
            None,
            &DeviceAccessChange::Enable {
                secret: Some(secret("default", Tier::Edit, "hunter2")),
                open: false,
            },
            [3; 16],
            2,
        )
        .unwrap();
        assert!(store.ble_enabled);
        assert!(!store.open);
        assert_eq!(store.secrets.len(), 1);
        assert_eq!(store.secrets[0].label, "default");
        // The key is derived here, and the board can verify it.
        assert_eq!(
            store.secrets[0].k,
            lpc_access::derive_login_key(b"hunter2", &[3; 16], 2)
        );
    }

    #[test]
    fn open_needs_no_password_but_locked_needs_one() {
        let open = apply_access_change(
            None,
            &DeviceAccessChange::Enable {
                secret: None,
                open: true,
            },
            [0; 16],
            1,
        )
        .unwrap();
        assert!(open.open && open.ble_enabled && open.secrets.is_empty());
        assert!(
            apply_access_change(
                None,
                &DeviceAccessChange::Enable {
                    secret: None,
                    open: false
                },
                [0; 16],
                1
            )
            .is_err()
        );
    }

    #[test]
    fn add_replaces_by_label_and_revoke_removes() {
        let one = apply_access_change(
            None,
            &DeviceAccessChange::Add(secret("camp", Tier::Play, "a")),
            [1; 16],
            1,
        )
        .unwrap();
        let two = apply_access_change(
            Some(&one),
            &DeviceAccessChange::Add(secret("camp", Tier::Edit, "b")),
            [2; 16],
            1,
        )
        .unwrap();
        assert_eq!(two.secrets.len(), 1);
        assert_eq!(two.secrets[0].tier, Tier::Edit);
        let none = apply_access_change(
            Some(&two),
            &DeviceAccessChange::Revoke {
                label: "camp".to_string(),
            },
            [0; 16],
            1,
        )
        .unwrap();
        assert!(none.secrets.is_empty());
    }

    #[test]
    fn turning_bluetooth_on_or_off_asks_for_a_restart_until_one_is_seen() {
        let mut records = DeviceAccessRecords::default();
        let mut store = DeviceAccessFile::locked();
        store.ble_enabled = true;
        records.record("dev_1", store.clone(), 1.0);
        assert!(records.get("dev_1").unwrap().restart_pending);
        assert!(records.note_restarted("dev_1"));
        assert!(!records.get("dev_1").unwrap().restart_pending);
        // A secrets-only change needs no restart: the board reads them live.
        records.record("dev_1", store.clone(), 2.0);
        assert!(!records.get("dev_1").unwrap().restart_pending);
        store.ble_enabled = false;
        records.record("dev_1", store, 3.0);
        assert!(records.get("dev_1").unwrap().restart_pending);
        let back = DeviceAccessRecords::from_json(&records.to_json());
        assert_eq!(back.devices, records.devices);
    }
}
