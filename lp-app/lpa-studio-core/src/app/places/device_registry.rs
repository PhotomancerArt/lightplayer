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
///
/// [`Default`] exists so the additive fields below can be filled by
/// `..Default::default()` rather than restated at every construction site —
/// the shape has grown three times now, and it will grow again.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisteredDevice {
    /// `dev…` uid (string form; stamped on the device per M5's flow).
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
    /// The chip family (device model P1/P2, `DeviceRecord.chip`), learned
    /// from a boot banner or a hello's firmware package and never cleared by
    /// a later sighting that does not know it. Additive: a row written
    /// before the device-model rebuild loads with `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chip: Option<String>,
    /// The firmware label the board last reported in a hello
    /// (`DeviceRecord.firmware`, "fw-esp32c6 abc1234"), same learned-never-
    /// cleared rule as `chip`. Memory for the card header's identity line
    /// after a close or a restart — never the Firmware zone's verdict.
    /// Additive: a row written before it exists loads with `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub firmware: Option<String>,
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
    /// The rebuilt device model's own handle for this row
    /// (`lpa_devices::DeviceId`), so a `PersistRecord` after a refresh finds
    /// the row it belongs to instead of minting a second one. `None` = a row
    /// written before the model existed; the next sighting fills it.
    ///
    /// Additive, like every field below the original four: the on-disk format
    /// is the same file, and a legacy row loads with these absent (see
    /// `legacy_registry_json_with_no_new_fields_still_parses`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_id: Option<u64>,
    /// The name the DEVICE reports for itself (its provisioned name).
    /// Distinct from [`Self::name`], which stays the user-facing truth.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_name: Option<String>,
    /// Whether the user asked for this device's port to be opened whenever
    /// it appears (the model's `Intent::autoconnect`).
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub autoconnect: bool,
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

    /// Upsert only the fields the device MODEL owns, leaving everything else
    /// a row already carries alone.
    ///
    /// The device model's `Command::PersistRecord` describes identity and
    /// preferences and nothing else (a record that said "flashing" would be a
    /// state machine with a disk). A whole-row [`Self::upsert`] would
    /// therefore erase what the row remembers about pushes — `association`,
    /// `board_id`, `previous_uids` — every time a board said hello.
    ///
    /// The user-facing `name` follows the same rule as the merge above: a
    /// model record with no user name does not blank a name the registry
    /// already holds.
    pub fn upsert_model_fields(&self, device: RegisteredDevice) -> Result<(), LibraryError> {
        let mut file = self.load()?;
        let Some(existing) = file.devices.iter_mut().find(|row| row.uid == device.uid) else {
            file.devices.push(device);
            return self.save(&file);
        };
        if !device.name.is_empty() {
            existing.name = device.name;
        }
        if !device.transport.is_empty() {
            existing.transport = device.transport;
        }
        if device.last_seen_at > existing.last_seen_at {
            existing.last_seen_at = device.last_seen_at;
        }
        if device.hardware_id.is_some() {
            existing.hardware_id = device.hardware_id;
        }
        if device.device_name.is_some() {
            existing.device_name = device.device_name;
        }
        existing.device_id = device.device_id.or(existing.device_id);
        // Learned, never cleared (device model P1/P2, mirroring `board_id`/
        // `chip` on `DeviceRecord` itself): a window that did not re-learn
        // one must not blank what an earlier sighting already established.
        if device.board_id.is_some() {
            existing.board_id = device.board_id;
        }
        if device.chip.is_some() {
            existing.chip = device.chip;
        }
        if device.firmware.is_some() {
            existing.firmware = device.firmware;
        }
        existing.autoconnect = device.autoconnect;
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
    let chip = recent.chip.clone().or_else(|| other.chip.clone());
    let firmware = recent.firmware.clone().or_else(|| other.firmware.clone());

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
        chip,
        firmware,
        hardware_id: Some(hardware_id.to_string()),
        previous_uids,
        // Model-side fields follow the same recent-row-wins rule as the rest.
        device_id: recent.device_id.or(other.device_id),
        device_name: recent
            .device_name
            .clone()
            .or_else(|| other.device_name.clone()),
        autoconnect: old_row.autoconnect || new_row.autoconnect,
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
            uid: "dev0000000000000001".to_string(),
            name: "Luna's porch sign".to_string(),
            last_seen_at: 1.0,
            association: Some(DeviceAssociation {
                device: PrefixedUid::mint(UidPrefix::Device, &[1u8; 16]),
                project: PrefixedUid::mint(UidPrefix::Project, &[2u8; 16]),
                version: ContentHash::of(b"v3"),
                at: 1.0,
            }),
            ..RegisteredDevice::default()
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
                uid: "dev0000000000000001".to_string(),
                name: "Porch sign".to_string(),
                transport: "USB".to_string(),
                last_seen_at: 1.0,
                association: None,
                ..RegisteredDevice::default()
            })
            .unwrap();

        registry
            .rename("dev0000000000000001", "Luna's sign")
            .unwrap();
        assert_eq!(registry.list().unwrap()[0].name, "Luna's sign");

        assert!(matches!(
            registry.rename("dev0000000000000002", "x"),
            Err(LibraryError::NotFound(_))
        ));
    }

    #[test]
    fn forget_removes_the_entry_and_is_idempotent() {
        let fs: Rc<RefCell<dyn LpFs>> = Rc::new(RefCell::new(LpFsMemory::new()));
        let registry = DeviceRegistry::new(fs);
        registry
            .upsert(RegisteredDevice {
                uid: "dev0000000000000001".to_string(),
                name: "Porch sign".to_string(),
                transport: "USB".to_string(),
                last_seen_at: 1.0,
                association: None,
                ..RegisteredDevice::default()
            })
            .unwrap();

        registry.forget("dev0000000000000001").unwrap();
        assert!(registry.list().unwrap().is_empty());
        // forgetting again (or an unknown uid) is a no-op, not an error
        registry.forget("dev0000000000000001").unwrap();
    }

    #[test]
    fn legacy_registry_json_with_no_new_fields_still_parses() {
        // additive #[serde(default)] fields (device identity design §4):
        // a pre-P1 row on disk must load with hardware_id: None and an
        // empty previous_uids, no format version bump.
        let fs: Rc<RefCell<dyn LpFs>> = Rc::new(RefCell::new(LpFsMemory::new()));
        let legacy = serde_json::json!({
            "devices": [{
                "uid": "dev0000000000000001",
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
        assert_eq!(
            listed[0].chip, None,
            "a row written before the device-model rebuild has no chip"
        );
    }

    #[test]
    fn rekey_or_merge_moves_a_solo_row_and_records_the_old_uid() {
        let fs: Rc<RefCell<dyn LpFs>> = Rc::new(RefCell::new(LpFsMemory::new()));
        let registry = DeviceRegistry::new(fs);
        registry
            .upsert(RegisteredDevice {
                uid: "dev00000000000past1".to_string(),
                name: "Porch sign".to_string(),
                transport: "USB".to_string(),
                last_seen_at: 1.0,
                association: None,
                board_id: Some("esp32-c6-devkit".to_string()),
                ..RegisteredDevice::default()
            })
            .unwrap();

        registry
            .rekey_or_merge(
                "dev00000000000past1",
                "dev000000000000new1",
                "efuse:aa:bb:cc:dd:ee:ff",
            )
            .unwrap();

        let listed = registry.list().unwrap();
        assert_eq!(listed.len(), 1, "moved, not duplicated");
        let row = &listed[0];
        assert_eq!(row.uid, "dev000000000000new1");
        assert_eq!(row.previous_uids, vec!["dev00000000000past1".to_string()]);
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
                uid: "dev00000000000past1".to_string(),
                name: "Stamped name".to_string(),
                transport: "".to_string(),
                last_seen_at: 10.0,
                association: Some(DeviceAssociation {
                    device: "dev00000000000past1".parse().unwrap(),
                    project,
                    version: ContentHash::of(b"v1"),
                    at: 10.0,
                }),
                previous_uids: vec!["dev00000000ev1ct10n".to_string()],
                ..RegisteredDevice::default()
            })
            .unwrap();

        // the NEW (derived) row: seen more recently, empty name/transport
        registry
            .upsert(RegisteredDevice {
                uid: "dev000000000000new1".to_string(),
                name: "".to_string(),
                transport: "USB".to_string(),
                last_seen_at: 20.0,
                association: Some(DeviceAssociation {
                    device: "dev000000000000new1".parse().unwrap(),
                    project,
                    version: ContentHash::of(b"v2"),
                    at: 20.0,
                }),
                board_id: Some("esp32-c6-devkit".to_string()),
                hardware_id: Some("efuse:aa:bb:cc:dd:ee:ff".to_string()),
                ..RegisteredDevice::default()
            })
            .unwrap();

        registry
            .rekey_or_merge(
                "dev00000000000past1",
                "dev000000000000new1",
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
        assert_eq!(row.uid, "dev000000000000new1");
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
            "dev00000000ev1ct10n".to_string(),
            "dev00000000000past1".to_string(),
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
                uid: "dev000000000000new1".to_string(),
                name: "Porch sign".to_string(),
                transport: "USB".to_string(),
                last_seen_at: 1.0,
                association: None,
                ..RegisteredDevice::default()
            })
            .unwrap();

        registry
            .rekey_or_merge(
                "dev000000000000n0ne",
                "dev000000000000new1",
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
                uid: "dev0000000000000001".to_string(),
                name: "Porch sign".to_string(),
                transport: "USB".to_string(),
                last_seen_at: 1.0,
                association: None,
                ..RegisteredDevice::default()
            })
            .unwrap();

        registry
            .rekey_or_merge(
                "dev0000000000000001",
                "dev0000000000000001",
                "efuse:aa:bb:cc:dd:ee:ff",
            )
            .unwrap();

        let listed = registry.list().unwrap();
        assert_eq!(listed[0].hardware_id, None, "same uid: untouched");
    }

    /// The model owns identity and preferences; the row owns what was
    /// pushed to the board. A sighting must not erase the latter — the
    /// whole reason `upsert_model_fields` exists beside `upsert`.
    #[test]
    fn a_model_upsert_preserves_what_the_row_remembers_about_pushes() {
        let fs: Rc<RefCell<dyn LpFs>> = Rc::new(RefCell::new(LpFsMemory::new()));
        let registry = DeviceRegistry::new(fs);
        let association = DeviceAssociation {
            device: PrefixedUid::mint(UidPrefix::Device, &[1u8; 16]),
            project: PrefixedUid::mint(UidPrefix::Project, &[2u8; 16]),
            version: ContentHash::of(b"v3"),
            at: 5.0,
        };
        registry
            .upsert(RegisteredDevice {
                uid: "dev0000000000000001".to_string(),
                name: "Porch sign".to_string(),
                transport: "USB".to_string(),
                last_seen_at: 5.0,
                association: Some(association.clone()),
                board_id: Some("esp32-c6-devkit".to_string()),
                previous_uids: vec!["dev00000000000past1".to_string()],
                ..RegisteredDevice::default()
            })
            .unwrap();

        registry
            .upsert_model_fields(RegisteredDevice {
                uid: "dev0000000000000001".to_string(),
                // The model holds no user name for this board yet.
                name: String::new(),
                transport: "USB".to_string(),
                last_seen_at: 9.0,
                hardware_id: Some("efuse:aa:bb:cc:dd:ee:ff".to_string()),
                device_id: Some(4),
                device_name: Some("Bench board".to_string()),
                autoconnect: true,
                ..RegisteredDevice::default()
            })
            .unwrap();

        let row = registry.list().unwrap().pop().expect("one row");
        assert_eq!(row.association, Some(association), "the push survived");
        assert_eq!(row.board_id.as_deref(), Some("esp32-c6-devkit"));
        assert_eq!(row.previous_uids, vec!["dev00000000000past1".to_string()]);
        assert_eq!(
            row.name, "Porch sign",
            "an empty model name never blanks the user's"
        );
        assert_eq!(row.device_id, Some(4));
        assert_eq!(row.device_name.as_deref(), Some("Bench board"));
        assert!(row.autoconnect);
        assert_eq!(row.last_seen_at, 9.0);
    }

    /// A model sighting that DOES learn a board id, chip and firmware label
    /// writes them — the whole point of wiring `DeviceRecord.board_id`/
    /// `chip`/`firmware` into the row (device-card-v2 plan P2's amendment;
    /// firmware bench 2026-09-04). A later sighting that knows none of them
    /// must not blank what this one just learned.
    #[test]
    fn a_model_upsert_learns_board_id_chip_and_firmware_and_never_blanks_them() {
        let fs: Rc<RefCell<dyn LpFs>> = Rc::new(RefCell::new(LpFsMemory::new()));
        let registry = DeviceRegistry::new(fs);
        registry
            .upsert_model_fields(RegisteredDevice {
                uid: "dev0000000000000001".to_string(),
                transport: "USB".to_string(),
                last_seen_at: 1.0,
                board_id: Some("seeed/xiao-esp32-c6".to_string()),
                chip: Some("esp32c6".to_string()),
                firmware: Some("fw-esp32c6 abc1234".to_string()),
                ..RegisteredDevice::default()
            })
            .unwrap();

        let row = registry.list().unwrap().pop().expect("one row");
        assert_eq!(row.board_id.as_deref(), Some("seeed/xiao-esp32-c6"));
        assert_eq!(row.chip.as_deref(), Some("esp32c6"));
        assert_eq!(row.firmware.as_deref(), Some("fw-esp32c6 abc1234"));

        // A later window that re-learns neither (a hello with no board id,
        // no boot banner) must not blank what this sighting established.
        registry
            .upsert_model_fields(RegisteredDevice {
                uid: "dev0000000000000001".to_string(),
                transport: "USB".to_string(),
                last_seen_at: 2.0,
                ..RegisteredDevice::default()
            })
            .unwrap();

        let row = registry.list().unwrap().pop().expect("one row");
        assert_eq!(row.board_id.as_deref(), Some("seeed/xiao-esp32-c6"));
        assert_eq!(row.chip.as_deref(), Some("esp32c6"));
        assert_eq!(row.firmware.as_deref(), Some("fw-esp32c6 abc1234"));
    }

    /// The additive fields serialize only when they carry something, so a
    /// registry written by this build stays readable by one that predates
    /// them — the on-disk format is the same file, not a new one.
    #[test]
    fn the_model_fields_are_absent_on_disk_until_they_hold_something() {
        let fs: Rc<RefCell<dyn LpFs>> = Rc::new(RefCell::new(LpFsMemory::new()));
        let registry = DeviceRegistry::new(fs.clone());
        registry
            .upsert(RegisteredDevice {
                uid: "dev0000000000000001".to_string(),
                name: "Porch sign".to_string(),
                transport: "USB".to_string(),
                last_seen_at: 1.0,
                ..RegisteredDevice::default()
            })
            .unwrap();

        let bytes = fs
            .borrow()
            .read_file(REGISTRY_PATH.as_path())
            .expect("the registry file");
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let row = &json["devices"][0];
        for absent in ["deviceId", "deviceName", "autoconnect"] {
            assert!(
                row.get(absent).is_none(),
                "{absent} should be absent: {row}"
            );
        }
    }

    #[test]
    fn rekey_or_merge_is_a_noop_when_neither_row_exists() {
        let fs: Rc<RefCell<dyn LpFs>> = Rc::new(RefCell::new(LpFsMemory::new()));
        let registry = DeviceRegistry::new(fs);
        registry
            .rekey_or_merge(
                "dev0000000000001st1",
                "dev0000000000002nd1",
                "efuse:aa:bb:cc:dd:ee:ff",
            )
            .unwrap();
        assert!(registry.list().unwrap().is_empty());
    }
}
