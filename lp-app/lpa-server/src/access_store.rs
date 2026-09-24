//! Reading the access files through the server's OWN filesystem.
//!
//! The wire can never read an access file (the fs gate refuses, on every
//! link); the server reads them here, directly off its base fs, to know
//! which secrets are installed and whether the device is `open`.
//!
//! Installed secrets = the device store's (device-only secrets and the
//! account default) ∪ every loaded project's sidecar. A file that is
//! missing installs nothing; a file that fails to parse installs nothing
//! and is logged — damage only ever takes access away.

extern crate alloc;

use alloc::vec::Vec;
use lpc_access::{DeviceAccessFile, ProjectAccessFile, SecretEntry};
use lpc_model::{AsLpPath, LpPathBuf};
use lpfs::LpFs;

/// The device store at root `/.lp/access.json`, or [`DeviceAccessFile::locked`]
/// when it is missing or unreadable.
pub fn read_device_store(fs: &dyn LpFs) -> DeviceAccessFile {
    let Ok(bytes) = fs.read_file(DeviceAccessFile::PATH.as_path()) else {
        return DeviceAccessFile::locked();
    };
    match DeviceAccessFile::from_json(&bytes) {
        Ok(store) => store,
        Err(error) => {
            log::warn!("access: device store unreadable, treating as locked: {error}");
            DeviceAccessFile::locked()
        }
    }
}

/// The sidecar of the project at `project_path` (relative to the base fs,
/// as the project manager holds it), or no secrets.
pub fn read_project_secrets(fs: &dyn LpFs, project_path: &str) -> Vec<SecretEntry> {
    let path = LpPathBuf::from("/")
        .join(project_path)
        .join(ProjectAccessFile::RELATIVE_PATH.trim_start_matches('/'));
    let Ok(bytes) = fs.read_file(path.as_path()) else {
        return Vec::new();
    };
    match ProjectAccessFile::from_json(&bytes) {
        Ok(file) => file.secrets,
        Err(error) => {
            log::warn!(
                "access: project sidecar {} unreadable, installing none of it: {error}",
                path.as_str()
            );
            Vec::new()
        }
    }
}

/// Every installed secret: the device store's first, then each loaded
/// project's sidecar, in the order given.
pub fn installed_secrets<'a>(
    fs: &dyn LpFs,
    loaded_project_paths: impl IntoIterator<Item = &'a str>,
) -> Vec<SecretEntry> {
    let mut secrets = read_device_store(fs).secrets;
    for project_path in loaded_project_paths {
        secrets.extend(read_project_secrets(fs, project_path));
    }
    secrets
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_access::Tier;
    use lpfs::LpFsMemory;

    #[test]
    fn a_missing_store_is_locked() {
        let fs = LpFsMemory::new();
        assert_eq!(read_device_store(&fs), DeviceAccessFile::locked());
    }

    #[test]
    fn a_damaged_store_is_locked() {
        let fs = LpFsMemory::new();
        fs.write_file(
            DeviceAccessFile::PATH.as_path(),
            b"{\"version\":1,\"open\":true}",
        )
        .unwrap();
        assert_eq!(read_device_store(&fs), DeviceAccessFile::locked());
    }

    #[test]
    fn installed_secrets_are_the_store_then_each_sidecar() {
        let fs = LpFsMemory::new();
        let store = DeviceAccessFile {
            secrets: alloc::vec![entry("account default", Tier::Edit)],
            ble_enabled: true,
            ..DeviceAccessFile::locked()
        };
        fs.write_file(
            DeviceAccessFile::PATH.as_path(),
            store.to_json().unwrap().as_bytes(),
        )
        .unwrap();
        let sidecar = ProjectAccessFile::new(alloc::vec![entry("camp", Tier::Play)]);
        fs.write_file(
            "/projects/choker/.lp/access.json".as_path(),
            sidecar.to_json().unwrap().as_bytes(),
        )
        .unwrap();

        let labels: Vec<_> = installed_secrets(&fs, ["projects/choker", "projects/other"])
            .into_iter()
            .map(|secret| secret.label)
            .collect();
        assert_eq!(labels, ["account default", "camp"]);
    }

    fn entry(label: &str, tier: Tier) -> SecretEntry {
        SecretEntry::from_password(label, tier, label.as_bytes(), [1; 16], 1)
    }
}
