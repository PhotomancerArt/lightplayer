//! [`DeviceRecord`] ⇄ [`RegisteredDevice`]: the model's persisted snapshot in
//! the store the studio already has.
//!
//! The device registry (`/registry.json`) survived the teardown intact, and
//! it is where remembered boards live — so the rebuilt model's records go
//! there rather than into a second file that would disagree with it. The
//! format grows the way it grew twice before: additive `#[serde(default)]`
//! fields, no version bump, legacy rows still parse (pinned by
//! `device_registry`'s own tests).
//!
//! | `DeviceRecord` | `RegisteredDevice` |
//! |---|---|
//! | `device` (`DeviceId`) | `device_id` (additive) |
//! | `identity.uid` | `uid` — the row KEY |
//! | `identity.mac` | `hardware_id` as `efuse:aa:bb:…` ([`HardwareId`]'s canonical string) |
//! | `identity.name` (provisioned) | `device_name` (additive) |
//! | `name` (the user's) | `name` — the registry's user-facing truth |
//! | `autoconnect` | `autoconnect` (additive) |
//! | `last_seen` (millis) | `last_seen_at` (epoch seconds) |
//! | `board_id` (learned, never cleared) | `board_id` (additive — the SAME field a provisioning-flow board pick also writes) |
//! | `chip` (learned, never cleared) | `chip` (additive) |
//!
//! **`identity.endpoint` is deliberately NOT persisted.** A browser Web
//! Serial endpoint id is minted per page load from a counter
//! (`…-port-1`, `…-port-2`), so it is not stable across a refresh; a
//! persisted one would route a granted port to whichever record happened to
//! be written first and show the wrong name until the hello corrected it.
//! Without it a granted port arrives as a pending link, identifies, and
//! MERGES into its record by uid or MAC — the model's own join, which is
//! revisable and journaled.
//!
//! **A record with no uid and no MAC is not persisted.** The row key is the
//! uid, and an anonymous board has nothing to key on; inventing one would put
//! an un-rejoinable row on disk. The only way to reach that state today is
//! adopting a still-anonymous link, which is Setup's gesture — round 2.

use lpa_devices::identity::{DeviceId, DeviceUid, IdentityChain, MacAddress};
use lpa_devices::record::DeviceRecord;
use lpa_devices::time::Millis;

use crate::app::places::{HardwareId, RegisteredDevice};

/// Transport label recorded for rows this model writes. One transport this
/// round; the registry column is display-only.
const USB_TRANSPORT: &str = "USB";

/// The registry key for a record: its uid, else its MAC. `None` when the
/// device is anonymous (see the module doc).
pub fn registry_key(identity: &IdentityChain) -> Option<String> {
    if let Some(uid) = &identity.uid {
        return Some(uid.0.clone());
    }
    identity.mac.as_ref().map(|mac| format!("mac:{}", mac.0))
}

/// One model record as a registry row.
///
/// `None` for an anonymous record: there is no honest key for it.
pub fn registry_row_from_record(record: &DeviceRecord) -> Option<RegisteredDevice> {
    let uid = registry_key(&record.identity)?;
    Some(RegisteredDevice {
        uid,
        name: record.name.clone().unwrap_or_default(),
        transport: USB_TRANSPORT.to_string(),
        last_seen_at: record
            .last_seen
            .map(|last| last.0 as f64 / 1_000.0)
            .unwrap_or_default(),
        association: None,
        board_id: record.board_id.clone(),
        chip: record.chip.clone(),
        hardware_id: record
            .identity
            .mac
            .as_ref()
            .and_then(|mac| HardwareId::from_base_mac(&mac.0))
            .map(|id| id.to_string()),
        previous_uids: Vec::new(),
        device_id: Some(record.device.0),
        device_name: record.identity.name.clone(),
        autoconnect: record.autoconnect,
    })
}

/// One registry row as a model record, for `Roster::load_records` at boot.
///
/// `fallback_device_id` is used when the row predates the model (no
/// `device_id`): the caller hands out ids so the roster's minting never
/// collides with a rehydrated one.
pub fn record_from_registry_row(row: &RegisteredDevice, fallback_device_id: u64) -> DeviceRecord {
    let mac = row
        .hardware_id
        .as_deref()
        .and_then(|origin| origin.strip_prefix("efuse:"))
        .and_then(HardwareId::from_base_mac)
        .map(|id| id.to_string())
        // `HardwareId`'s Display re-emits `efuse:<mac>`; the chain wants the
        // bare, normalized address.
        .and_then(|origin| origin.strip_prefix("efuse:").map(str::to_string))
        .or_else(|| row.uid.strip_prefix("mac:").map(str::to_string))
        .map(MacAddress);
    let uid = (!row.uid.starts_with("mac:")).then(|| DeviceUid(row.uid.clone()));

    DeviceRecord {
        device: DeviceId(row.device_id.unwrap_or(fallback_device_id)),
        identity: IdentityChain {
            // Not persisted: see the module doc.
            endpoint: None,
            mac,
            uid,
            name: row.device_name.clone(),
        },
        name: (!row.name.is_empty()).then(|| row.name.clone()),
        autoconnect: row.autoconnect,
        last_seen: (row.last_seen_at > 0.0)
            .then(|| Millis((row.last_seen_at * 1_000.0).round().max(0.0) as u64)),
        // Device-card-v2 plan P2: the model's own board_id/chip now share
        // the row's `board_id`/`chip` fields with the older
        // provisioning-flow board pick, so identity survives a full app
        // restart, not just a reopen — a row a session never re-learns
        // still carries what an earlier one wrote.
        board_id: row.board_id.clone(),
        chip: row.chip.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record() -> DeviceRecord {
        DeviceRecord {
            device: DeviceId(7),
            identity: IdentityChain {
                endpoint: Some(lpa_devices::identity::EndpointKey("usb-1".to_string())),
                mac: Some(MacAddress("60:55:f9:0a:0b:0c".to_string())),
                uid: Some(DeviceUid("dev000000daqf6dvvqz".to_string())),
                name: Some("Bench board".to_string()),
            },
            name: Some("Kitchen".to_string()),
            autoconnect: true,
            last_seen: Some(Millis(1_500)),
            board_id: Some("seeed/xiao-esp32-c6".to_string()),
            chip: Some("esp32c6".to_string()),
        }
    }

    #[test]
    fn a_record_round_trips_through_a_registry_row() {
        let row = registry_row_from_record(&record()).expect("an identified record persists");

        let back = record_from_registry_row(&row, 999);

        let original = record();
        assert_eq!(back.device, original.device, "the model handle survives");
        assert_eq!(back.identity.uid, original.identity.uid);
        assert_eq!(back.identity.mac, original.identity.mac);
        assert_eq!(back.identity.name, original.identity.name);
        assert_eq!(back.name, original.name);
        assert!(back.autoconnect);
        assert_eq!(back.last_seen, original.last_seen);
        assert_eq!(
            back.board_id, original.board_id,
            "the learned board id survives a full app restart"
        );
        assert_eq!(
            back.chip, original.chip,
            "the learned chip survives a full app restart"
        );
        assert_eq!(
            back.identity.endpoint, None,
            "the endpoint is a per-page fingerprint, never persisted"
        );
    }

    /// A board with silicon but no provisioned uid still earns a row, keyed
    /// on the binding it does have.
    #[test]
    fn a_mac_only_record_keys_on_its_mac() {
        let mut record = record();
        record.identity.uid = None;
        record.identity.name = None;

        let row = registry_row_from_record(&record).expect("a MAC is a key");
        assert_eq!(row.uid, "mac:60:55:f9:0a:0b:0c");

        let back = record_from_registry_row(&row, 1);
        assert_eq!(back.identity.uid, None, "the key was never a uid");
        assert_eq!(
            back.identity.mac,
            Some(MacAddress("60:55:f9:0a:0b:0c".to_string()))
        );
    }

    #[test]
    fn an_anonymous_record_has_no_honest_key_and_is_not_persisted() {
        let record = DeviceRecord::new(DeviceId(3), IdentityChain::default());

        assert_eq!(registry_row_from_record(&record), None);
    }

    /// A row written before the model existed loads as a record with the
    /// caller's id — the registry's whole point is that a user's name
    /// outlives every rewrite of the machinery around it.
    #[test]
    fn a_legacy_row_loads_with_the_callers_device_id() {
        let legacy = RegisteredDevice {
            uid: "dev0000000000000001".to_string(),
            name: "Porch sign".to_string(),
            transport: "USB".to_string(),
            last_seen_at: 12.0,
            ..RegisteredDevice::default()
        };

        let record = record_from_registry_row(&legacy, 42);

        assert_eq!(record.device, DeviceId(42));
        assert_eq!(record.name.as_deref(), Some("Porch sign"));
        assert_eq!(record.identity.mac, None);
        assert!(!record.autoconnect);
        assert_eq!(record.last_seen, Some(Millis(12_000)));
        assert_eq!(
            record.board_id, None,
            "a row from before board id/chip existed loads with neither"
        );
        assert_eq!(record.chip, None);
    }
}
