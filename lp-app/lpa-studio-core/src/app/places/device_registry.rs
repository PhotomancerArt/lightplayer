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
    /// f64 epoch seconds, caller-supplied.
    pub last_seen_at: f64,
    /// What was last pushed to it, if anything.
    pub association: Option<DeviceAssociation>,
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
            uid: "dev_0000000000000001".to_string(),
            name: "Luna's porch sign".to_string(),
            last_seen_at: 1.0,
            association: Some(DeviceAssociation {
                device: PrefixedUid::mint(UidPrefix::Device, &[1u8; 16]),
                project: PrefixedUid::mint(UidPrefix::Project, &[2u8; 16]),
                version: ContentHash::of(b"v3"),
                at: 1.0,
            }),
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
}
