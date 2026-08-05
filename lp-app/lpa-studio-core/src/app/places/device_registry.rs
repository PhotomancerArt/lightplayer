//! The device registry: remembered devices, persisted in the library store.
//!
//! `/registry.json` at the store root. A device remembers which line was
//! last pushed to it (M1's `DeviceAssociation`), so behind/up-to-date is
//! computed against the right project (fleet vs family — D11).

use std::cell::RefCell;
use std::rc::Rc;

use lpc_history::DeviceAssociation;
use lpc_model::AsLpPath;
use lpfs::{FsError, LpFs};
use serde::{Deserialize, Serialize};

use crate::app::library::LibraryError;

pub const REGISTRY_PATH: &str = "/registry.json";

/// One remembered device.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisteredDevice {
    /// `dev_…` uid (string form; stamped on the device per M5's flow).
    pub uid: String,
    pub name: String,
    /// Transport label recorded at last sight, from the live session's
    /// connector class ("USB" for serial). Empty when the registry entry
    /// predates transport recording; the next sighting fills it.
    #[serde(default)]
    pub transport: String,
    /// f64 epoch seconds, caller-supplied.
    pub last_seen_at: f64,
    /// What was last pushed to it, if anything.
    pub association: Option<DeviceAssociation>,
    /// The board chosen at provisioning (`vendor/product`, board-selection
    /// M5) — gallery art + the hardware pane (M6) read it. `None` =
    /// generic/unknown; preserved across sightings by the merge upsert.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub board_id: Option<String>,
    /// The identity source, canonical string form (see
    /// [`super::HardwareId`]'s `Display`: `"efuse:aa:bb:cc:dd:ee:ff"` or
    /// `"minted"`). `None` = legacy row not yet re-keyed (device identity
    /// design §4) — the next sighting fills it in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hardware_id: Option<String>,
    /// Uids this device previously wore before a re-key (device identity
    /// design §4) — lets old history events resolve for display.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_uids: Vec<String>,
}

/// Load/save wrapper over the store.
pub struct DeviceRegistry {
    fs: Rc<RefCell<dyn LpFs>>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct RegistryFile {
    devices: Vec<RegisteredDevice>,
}

impl DeviceRegistry {
    pub fn new(fs: Rc<RefCell<dyn LpFs>>) -> Self {
        Self { fs }
    }

    pub fn list(&self) -> Result<Vec<RegisteredDevice>, LibraryError> {
        Ok(self.load()?.devices)
    }

    /// Insert or update by uid.
    pub fn upsert(&self, device: RegisteredDevice) -> Result<(), LibraryError> {
        let mut file = self.load()?;
        match file.devices.iter_mut().find(|d| d.uid == device.uid) {
            Some(existing) => *existing = device,
            None => file.devices.push(device),
        }
        self.save(&file)
    }

    /// Rename a remembered device (D34). The registry name is the
    /// user-facing truth: it wins over device-reported names at reconcile
    /// ([`upsert_device_merged`](super::device_session::upsert_device_merged))
    /// and is written back to the device when a session is live.
    pub fn rename(&self, uid: &str, name: &str) -> Result<(), LibraryError> {
        let mut file = self.load()?;
        let Some(device) = file.devices.iter_mut().find(|d| d.uid == uid) else {
            return Err(LibraryError::NotFound(format!("device {uid}")));
        };
        device.name = name.to_string();
        self.save(&file)
    }

    /// Forget a remembered device (D34 hygiene): remove it from the
    /// registry. Idempotent — forgetting an unknown uid is a no-op (the
    /// goal state is already true).
    pub fn forget(&self, uid: &str) -> Result<(), LibraryError> {
        let mut file = self.load()?;
        let before = file.devices.len();
        file.devices.retain(|d| d.uid != uid);
        if file.devices.len() == before {
            return Ok(());
        }
        self.save(&file)
    }

    /// Lazy re-key at sighting (device identity design §4, migration
    /// steps 2-3): fold a legacy `old_uid` row into the derived
    /// `new_uid` once a `HardwareId` resolves for it.
    ///
    /// - Row under `old_uid` only → moved to `new_uid`: `old_uid` is
    ///   pushed onto `previous_uids`, `hardware_id` is set.
    /// - Rows under BOTH → merged into the `new_uid` row: `name` /
    ///   `board_id` / `transport` prefer the more-recently-seen row's
    ///   non-empty value; `association` is whichever side has the later
    ///   `at`; `last_seen_at` is the max of the two; `previous_uids` is
    ///   the union of both rows plus `old_uid`; the `old_uid` row is
    ///   dropped.
    /// - Neither row exists, or `old_uid == new_uid` → no-op.
    pub fn rekey_or_merge(
        &self,
        old_uid: &str,
        new_uid: &str,
        hardware_id: &str,
    ) -> Result<(), LibraryError> {
        if old_uid == new_uid {
            return Ok(());
        }
        let mut file = self.load()?;
        let old_index = file.devices.iter().position(|d| d.uid == old_uid);
        let Some(old_index) = old_index else {
            // nothing sighted under the old uid: nothing to move or merge
            return Ok(());
        };
        let new_index = file.devices.iter().position(|d| d.uid == new_uid);

        match new_index {
            None => {
                let device = &mut file.devices[old_index];
                device.uid = new_uid.to_string();
                device.previous_uids.push(old_uid.to_string());
                device.hardware_id = Some(hardware_id.to_string());
            }
            Some(new_index) => {
                let old_row = file.devices[old_index].clone();
                let new_row = file.devices[new_index].clone();
                let merged =
                    merge_registered_devices(&old_row, &new_row, old_uid, new_uid, hardware_id);
                file.devices
                    .retain(|d| d.uid != old_uid && d.uid != new_uid);
                file.devices.push(merged);
            }
        }
        self.save(&file)
    }

    fn load(&self) -> Result<RegistryFile, LibraryError> {
        let fs = self.fs.borrow();
        let bytes = match fs.read_file(REGISTRY_PATH.as_path()) {
            Ok(bytes) => bytes,
            Err(FsError::NotFound(_)) => return Ok(RegistryFile::default()),
            Err(e) => return Err(LibraryError::Fs(e.to_string())),
        };
        serde_json::from_slice(&bytes).map_err(|e| LibraryError::Meta(format!("registry: {e}")))
    }

    fn save(&self, file: &RegistryFile) -> Result<(), LibraryError> {
        let bytes = serde_json::to_vec_pretty(file)
            .map_err(|e| LibraryError::Meta(format!("registry: {e}")))?;
        let fs = self.fs.borrow();
        fs.write_file(REGISTRY_PATH.as_path(), &bytes)
            .map_err(|e| LibraryError::Fs(e.to_string()))
    }
}

/// Field-by-field merge for [`DeviceRegistry::rekey_or_merge`]'s
/// both-rows case. See that method's doc for the rules.
fn merge_registered_devices(
    old_row: &RegisteredDevice,
    new_row: &RegisteredDevice,
    old_uid: &str,
    new_uid: &str,
    hardware_id: &str,
) -> RegisteredDevice {
    let (recent, other) = if old_row.last_seen_at >= new_row.last_seen_at {
        (old_row, new_row)
    } else {
        (new_row, old_row)
    };
    let name = if recent.name.is_empty() {
        other.name.clone()
    } else {
        recent.name.clone()
    };
    let transport = if recent.transport.is_empty() {
        other.transport.clone()
    } else {
        recent.transport.clone()
    };
    let board_id = recent.board_id.clone().or_else(|| other.board_id.clone());

    let association = match (&old_row.association, &new_row.association) {
        (Some(a), Some(b)) => Some(if a.at >= b.at { a.clone() } else { b.clone() }),
        (Some(a), None) => Some(a.clone()),
        (None, Some(b)) => Some(b.clone()),
        (None, None) => None,
    };

    let mut previous_uids = old_row.previous_uids.clone();
    for uid in &new_row.previous_uids {
        if !previous_uids.contains(uid) {
            previous_uids.push(uid.clone());
        }
    }
    let old_uid = old_uid.to_string();
    if !previous_uids.contains(&old_uid) {
        previous_uids.push(old_uid);
    }

    RegisteredDevice {
        uid: new_uid.to_string(),
        name,
        transport,
        last_seen_at: old_row.last_seen_at.max(new_row.last_seen_at),
        association,
        board_id,
        hardware_id: Some(hardware_id.to_string()),
        previous_uids,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_history::{ContentHash, PrefixedUid, UidPrefix};
    use lpfs::LpFsMemory;

    #[test]
    fn upsert_and_round_trip() {
        let fs: Rc<RefCell<dyn LpFs>> = Rc::new(RefCell::new(LpFsMemory::new()));
        let registry = DeviceRegistry::new(fs.clone());
        assert!(registry.list().unwrap().is_empty());

        let device = RegisteredDevice {
            transport: "USB".to_string(),
            uid: "dev_0000000000000001".to_string(),
            name: "Luna's porch sign".to_string(),
            last_seen_at: 1.0,
            association: Some(DeviceAssociation {
                device: PrefixedUid::mint(UidPrefix::Device, &[1u8; 16]),
                project: PrefixedUid::mint(UidPrefix::Project, &[2u8; 16]),
                version: ContentHash::of(b"v3"),
                at: 1.0,
            }),
            board_id: None,
            hardware_id: None,
            previous_uids: Vec::new(),
        };
        registry.upsert(device.clone()).unwrap();
        registry
            .upsert(RegisteredDevice {
                last_seen_at: 2.0,
                ..device.clone()
            })
            .unwrap();

        let listed = DeviceRegistry::new(fs).list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].last_seen_at, 2.0);
        assert_eq!(
            listed[0].association.as_ref().unwrap().version,
            ContentHash::of(b"v3")
        );
    }

    #[test]
    fn rename_edits_the_name_and_refuses_unknown_uids() {
        let fs: Rc<RefCell<dyn LpFs>> = Rc::new(RefCell::new(LpFsMemory::new()));
        let registry = DeviceRegistry::new(fs);
        registry
            .upsert(RegisteredDevice {
                uid: "dev_0000000000000001".to_string(),
                name: "Porch sign".to_string(),
                transport: "USB".to_string(),
                last_seen_at: 1.0,
                association: None,
                board_id: None,
                hardware_id: None,
                previous_uids: Vec::new(),
            })
            .unwrap();

        registry
            .rename("dev_0000000000000001", "Luna's sign")
            .unwrap();
        assert_eq!(registry.list().unwrap()[0].name, "Luna's sign");

        assert!(matches!(
            registry.rename("dev_0000000000000002", "x"),
            Err(LibraryError::NotFound(_))
        ));
    }

    #[test]
    fn forget_removes_the_entry_and_is_idempotent() {
        let fs: Rc<RefCell<dyn LpFs>> = Rc::new(RefCell::new(LpFsMemory::new()));
        let registry = DeviceRegistry::new(fs);
        registry
            .upsert(RegisteredDevice {
                uid: "dev_0000000000000001".to_string(),
                name: "Porch sign".to_string(),
                transport: "USB".to_string(),
                last_seen_at: 1.0,
                association: None,
                board_id: None,
                hardware_id: None,
                previous_uids: Vec::new(),
            })
            .unwrap();

        registry.forget("dev_0000000000000001").unwrap();
        assert!(registry.list().unwrap().is_empty());
        // forgetting again (or an unknown uid) is a no-op, not an error
        registry.forget("dev_0000000000000001").unwrap();
    }

    #[test]
    fn legacy_registry_json_with_no_new_fields_still_parses() {
        // additive #[serde(default)] fields (device identity design §4):
        // a pre-P1 row on disk must load with hardware_id: None and an
        // empty previous_uids, no format version bump.
        let fs: Rc<RefCell<dyn LpFs>> = Rc::new(RefCell::new(LpFsMemory::new()));
        let legacy = serde_json::json!({
            "devices": [{
                "uid": "dev_0000000000000001",
                "name": "Porch sign",
                "transport": "USB",
                "lastSeenAt": 1.0,
                "association": null,
            }]
        });
        fs.borrow()
            .write_file(
                REGISTRY_PATH.as_path(),
                serde_json::to_vec(&legacy).unwrap().as_slice(),
            )
            .unwrap();

        let registry = DeviceRegistry::new(fs);
        let listed = registry.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].hardware_id, None);
        assert!(listed[0].previous_uids.is_empty());
    }

    #[test]
    fn rekey_or_merge_moves_a_solo_row_and_records_the_old_uid() {
        let fs: Rc<RefCell<dyn LpFs>> = Rc::new(RefCell::new(LpFsMemory::new()));
        let registry = DeviceRegistry::new(fs);
        registry
            .upsert(RegisteredDevice {
                uid: "dev_000000000000old1".to_string(),
                name: "Porch sign".to_string(),
                transport: "USB".to_string(),
                last_seen_at: 1.0,
                association: None,
                board_id: Some("esp32-c6-devkit".to_string()),
                hardware_id: None,
                previous_uids: Vec::new(),
            })
            .unwrap();

        registry
            .rekey_or_merge(
                "dev_000000000000old1",
                "dev_000000000000new1",
                "efuse:aa:bb:cc:dd:ee:ff",
            )
            .unwrap();

        let listed = registry.list().unwrap();
        assert_eq!(listed.len(), 1, "moved, not duplicated");
        let row = &listed[0];
        assert_eq!(row.uid, "dev_000000000000new1");
        assert_eq!(row.previous_uids, vec!["dev_000000000000old1".to_string()]);
        assert_eq!(row.hardware_id.as_deref(), Some("efuse:aa:bb:cc:dd:ee:ff"));
        assert_eq!(row.name, "Porch sign");
        assert_eq!(row.board_id.as_deref(), Some("esp32-c6-devkit"));
    }

    #[test]
    fn rekey_or_merge_merges_both_rows_preferring_the_recent_sighting() {
        let fs: Rc<RefCell<dyn LpFs>> = Rc::new(RefCell::new(LpFsMemory::new()));
        let registry = DeviceRegistry::new(fs);
        let project = PrefixedUid::mint(UidPrefix::Project, &[1u8; 16]);

        // the OLD (stamped) row: seen first, carries an older association
        registry
            .upsert(RegisteredDevice {
                uid: "dev_000000000000old1".to_string(),
                name: "Stamped name".to_string(),
                transport: "".to_string(),
                last_seen_at: 10.0,
                association: Some(DeviceAssociation {
                    device: "dev_000000000000old1".parse().unwrap(),
                    project,
                    version: ContentHash::of(b"v1"),
                    at: 10.0,
                }),
                board_id: None,
                hardware_id: None,
                previous_uids: vec!["dev_00000000eviction".to_string()],
            })
            .unwrap();

        // the NEW (derived) row: seen more recently, empty name/transport
        registry
            .upsert(RegisteredDevice {
                uid: "dev_000000000000new1".to_string(),
                name: "".to_string(),
                transport: "USB".to_string(),
                last_seen_at: 20.0,
                association: Some(DeviceAssociation {
                    device: "dev_000000000000new1".parse().unwrap(),
                    project,
                    version: ContentHash::of(b"v2"),
                    at: 20.0,
                }),
                board_id: Some("esp32-c6-devkit".to_string()),
                hardware_id: Some("efuse:aa:bb:cc:dd:ee:ff".to_string()),
                previous_uids: Vec::new(),
            })
            .unwrap();

        registry
            .rekey_or_merge(
                "dev_000000000000old1",
                "dev_000000000000new1",
                "efuse:aa:bb:cc:dd:ee:ff",
            )
            .unwrap();

        let listed = registry.list().unwrap();
        assert_eq!(
            listed.len(),
            1,
            "the old row is dropped, not kept alongside"
        );
        let row = &listed[0];
        assert_eq!(row.uid, "dev_000000000000new1");
        // transport: new row is more recent AND non-empty -> new wins
        assert_eq!(row.transport, "USB");
        // name: new row is more recent but EMPTY -> falls back to old's
        assert_eq!(row.name, "Stamped name");
        // board_id: only the new row has one
        assert_eq!(row.board_id.as_deref(), Some("esp32-c6-devkit"));
        assert_eq!(row.last_seen_at, 20.0);
        // association: new row's `at` is later
        assert_eq!(
            row.association.as_ref().unwrap().version,
            ContentHash::of(b"v2")
        );
        assert_eq!(row.hardware_id.as_deref(), Some("efuse:aa:bb:cc:dd:ee:ff"));
        // previous_uids: union of both rows' history, plus the old uid itself
        let mut previous = row.previous_uids.clone();
        previous.sort();
        let mut expected = vec![
            "dev_00000000eviction".to_string(),
            "dev_000000000000old1".to_string(),
        ];
        expected.sort();
        assert_eq!(previous, expected);
    }

    #[test]
    fn rekey_or_merge_is_a_noop_when_old_uid_is_unknown() {
        let fs: Rc<RefCell<dyn LpFs>> = Rc::new(RefCell::new(LpFsMemory::new()));
        let registry = DeviceRegistry::new(fs);
        registry
            .upsert(RegisteredDevice {
                uid: "dev_000000000000new1".to_string(),
                name: "Porch sign".to_string(),
                transport: "USB".to_string(),
                last_seen_at: 1.0,
                association: None,
                board_id: None,
                hardware_id: None,
                previous_uids: Vec::new(),
            })
            .unwrap();

        registry
            .rekey_or_merge(
                "dev_000000000000none",
                "dev_000000000000new1",
                "efuse:aa:bb:cc:dd:ee:ff",
            )
            .unwrap();

        let listed = registry.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].hardware_id, None, "nothing to move: untouched");
    }

    #[test]
    fn rekey_or_merge_is_a_noop_for_identical_uids() {
        let fs: Rc<RefCell<dyn LpFs>> = Rc::new(RefCell::new(LpFsMemory::new()));
        let registry = DeviceRegistry::new(fs);
        registry
            .upsert(RegisteredDevice {
                uid: "dev_0000000000000001".to_string(),
                name: "Porch sign".to_string(),
                transport: "USB".to_string(),
                last_seen_at: 1.0,
                association: None,
                board_id: None,
                hardware_id: None,
                previous_uids: Vec::new(),
            })
            .unwrap();

        registry
            .rekey_or_merge(
                "dev_0000000000000001",
                "dev_0000000000000001",
                "efuse:aa:bb:cc:dd:ee:ff",
            )
            .unwrap();

        let listed = registry.list().unwrap();
        assert_eq!(listed[0].hardware_id, None, "same uid: untouched");
    }

    #[test]
    fn rekey_or_merge_is_a_noop_when_neither_row_exists() {
        let fs: Rc<RefCell<dyn LpFs>> = Rc::new(RefCell::new(LpFsMemory::new()));
        let registry = DeviceRegistry::new(fs);
        registry
            .rekey_or_merge(
                "dev_000000000000one1",
                "dev_000000000000two1",
                "efuse:aa:bb:cc:dd:ee:ff",
            )
            .unwrap();
        assert!(registry.list().unwrap().is_empty());
    }
}
