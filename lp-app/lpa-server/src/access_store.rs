//! Reading and changing the access files through the server's OWN
//! filesystem.
//!
//! The wire can never read an access file (the fs gate refuses, on every
//! link); the server reads them here, directly off its base fs, to know
//! which secrets are installed and whether the device is `open`, and
//! answers the edit-tier access requests (`AccessList`, `AccessAdd`,
//! `AccessRemove`, `AccessSetSwitches`) here too — a read-modify-write of
//! the device store that never goes through the wire fs path, and whose
//! answer never carries a key.
//!
//! Installed secrets = the device store's (browser keys, the account key,
//! device passwords) ∪ every loaded project's sidecar. A sidecar that is
//! missing installs nothing; a file that fails to parse installs nothing
//! and is logged — damage only ever takes access away.
//!
//! The device store itself: a **missing** store is
//! [`DeviceAccessFile::fresh`] (Bluetooth on, open to anyone nearby, no
//! keys), and a **damaged** one is [`DeviceAccessFile::locked`] (Bluetooth
//! off, open to nobody).

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use lpc_access::{DeviceAccessFile, OpenTo, ProjectAccessFile, SALT_BYTES, SecretEntry};
use lpc_model::{AsLpPath, LpPathBuf};
use lpc_wire::server::{AccessEntryInfo, ServerMsgBody};
use lpfs::LpFs;

/// The device store at root `/.lp/access.json`: [`DeviceAccessFile::fresh`]
/// when there is none, [`DeviceAccessFile::locked`] when it cannot be read.
///
/// Missing is decided by `file_exists`, not by a read error: not every fs
/// reports a missing file as `NotFound`, and a read that fails for any
/// other reason is damage, which must not turn the radio on.
pub fn read_device_store(fs: &dyn LpFs) -> DeviceAccessFile {
    let path = DeviceAccessFile::PATH.as_path();
    match fs.file_exists(path) {
        Ok(false) => return DeviceAccessFile::fresh(),
        Ok(true) => {}
        Err(error) => {
            log::warn!("access: device store unreadable, treating as locked: {error}");
            return DeviceAccessFile::locked();
        }
    }
    let bytes = match fs.read_file(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            log::warn!("access: device store unreadable, treating as locked: {error}");
            return DeviceAccessFile::locked();
        }
    };
    match DeviceAccessFile::from_json(&bytes) {
        Ok(store) => store,
        Err(error) => {
            log::warn!("access: device store unreadable, treating as locked: {error}");
            DeviceAccessFile::locked()
        }
    }
}

/// The device store as a board's boot decides Bluetooth from it: what
/// [`read_device_store`] reads, except on a board **holding its files** for
/// the C6 layout change (`fs: legacy_held`), which is
/// [`DeviceAccessFile::locked`] — Bluetooth off.
///
/// Such a board runs on a RAM filesystem, so its store reads as missing —
/// `fresh()`, Bluetooth on — while its real store waits in the old region
/// with every other file. That real store may say Bluetooth off, or be
/// damaged (locked), and the held board must not be more open than its own
/// list. So it is treated like a store that cannot be read: damage, and an
/// unreachable store, only ever take access away. USB, a trusted link,
/// still holds edit, as on every board. Finish update moves the real store
/// back, and the next boot reads it.
pub fn device_store_at_boot(
    fs: &dyn LpFs,
    fs_boot_state: lpc_wire::FsBootState,
) -> DeviceAccessFile {
    if fs_boot_state == lpc_wire::FsBootState::LegacyHeld {
        log::info!("access: files held for the layout change; the device store waits with them");
        return DeviceAccessFile::locked();
    }
    read_device_store(fs)
}

/// How this boot stands, for whether it may rewrite the device store on
/// its own (OTA plan, doors #14).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BootStanding {
    /// A build that has run before on this board: a plain image, a split
    /// image's flashed or proven core, or a trial a link has confirmed.
    Confirmed,
    /// A new core on trial that no link has confirmed yet: it may yet roll
    /// back to the core before it.
    UnconfirmedTrial,
}

/// Whether a boot may rewrite `/.lp/access.json` **on its own** — a
/// migration to a newer shape, or defaults written on read. Never on an
/// unconfirmed trial: since over-the-air updates, an older core reads the
/// same file after a rollback (it reads `secrets` and `open`, nothing
/// else), and a file it cannot read is `locked()`. A person's own edit
/// (`AccessAdd` and the rest) is not a migration and is not gated here.
///
/// Today no firmware migrates the file — `read_device_store` converts an
/// older shape in memory and never writes — and a split image's engine
/// never runs on an unconfirmed trial (the trial confirms before it fetches
/// its engine). This is the gate a future migration must call.
pub fn may_migrate_device_store(standing: BootStanding) -> bool {
    standing == BootStanding::Confirmed
}

/// Write the device store, always at the current version.
pub fn write_device_store(fs: &dyn LpFs, store: &DeviceAccessFile) -> Result<(), String> {
    let json = store.to_json().map_err(|error| format!("{error}"))?;
    fs.write_file(DeviceAccessFile::PATH.as_path(), json.as_bytes())
        .map_err(|error| format!("{error}"))
}

/// `AccessList`: the device store's switches and entries, without keys.
#[inline(never)]
pub fn access_list(fs: &dyn LpFs) -> ServerMsgBody {
    list_body(&read_device_store(fs))
}

/// `AccessAdd`: merge `entry` into the device store (the same salt
/// replaces), write it, and answer the new list. Past the cap it is
/// refused and nothing is written.
///
/// Starts from what [`read_device_store`] reads: a missing store starts
/// fresh, and a damaged one starts locked — the write then replaces the
/// damage with a valid store, which is the recovery a trusted link is for.
#[inline(never)]
pub fn access_add(fs: &dyn LpFs, entry: SecretEntry) -> ServerMsgBody {
    let mut store = read_device_store(fs);
    if let Err(error) = store.upsert_secret(entry) {
        return ServerMsgBody::Error {
            error: format!("cannot add access: {error}"),
        };
    }
    write_and_list(fs, &store)
}

/// `AccessRemove`: drop the entry with `salt` (a no-op when absent) and
/// answer the list. Nothing is written when nothing changed.
#[inline(never)]
pub fn access_remove(fs: &dyn LpFs, salt: &[u8; SALT_BYTES]) -> ServerMsgBody {
    let mut store = read_device_store(fs);
    if !store.remove_secret(salt) {
        return list_body(&store);
    }
    write_and_list(fs, &store)
}

/// `AccessSetSwitches`: set whichever settings are given and answer the
/// list. `ble_enabled` applies at the next boot; the list reports the
/// stored value. `open` applies at once.
#[inline(never)]
pub fn access_set_switches(
    fs: &dyn LpFs,
    ble_enabled: Option<bool>,
    open: Option<OpenTo>,
) -> ServerMsgBody {
    let mut store = read_device_store(fs);
    if let Some(ble_enabled) = ble_enabled {
        store.ble_enabled = ble_enabled;
    }
    if let Some(open) = open {
        store.open = open;
    }
    write_and_list(fs, &store)
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

fn write_and_list(fs: &dyn LpFs, store: &DeviceAccessFile) -> ServerMsgBody {
    match write_device_store(fs, store) {
        Ok(()) => list_body(store),
        Err(error) => ServerMsgBody::Error {
            error: format!("cannot write the device's access list: {error}"),
        },
    }
}

fn list_body(store: &DeviceAccessFile) -> ServerMsgBody {
    ServerMsgBody::AccessList {
        ble_enabled: store.ble_enabled,
        open: store.open,
        entries: store.secrets.iter().map(AccessEntryInfo::from).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_access::Tier;
    use lpfs::LpFsMemory;

    #[test]
    fn a_missing_store_is_fresh_open_and_with_bluetooth_on() {
        let fs = LpFsMemory::new();
        let store = read_device_store(&fs);
        assert_eq!(store, DeviceAccessFile::fresh());
        assert!(store.ble_enabled);
        assert_eq!(store.open, OpenTo::Edit);
    }

    /// G1 rehearsal (2026-10-03): the spare C6, held for the layout change
    /// with a 16-entry store in its old region, booted on a RAM filesystem,
    /// read that as "no store" and advertised Bluetooth (`LP-8e30`). A held
    /// board boots locked: Bluetooth off until Finish update moves its real
    /// store back.
    #[test]
    fn a_held_board_boots_locked_with_bluetooth_off() {
        // The held board's RAM filesystem: no store on it.
        let fs = LpFsMemory::new();
        assert!(
            read_device_store(&fs).ble_enabled,
            "the premise: read as missing, it would be fresh with Bluetooth on"
        );
        let store = device_store_at_boot(&fs, lpc_wire::FsBootState::LegacyHeld);
        assert_eq!(store, DeviceAccessFile::locked());
        assert!(!store.ble_enabled);
        assert_eq!(store.open, OpenTo::Nobody);
        assert!(store.secrets.is_empty());
        // Every other boot reads the store as before.
        for fs_state in [
            lpc_wire::FsBootState::Mounted,
            lpc_wire::FsBootState::Formatted,
            lpc_wire::FsBootState::Memory,
        ] {
            assert_eq!(
                device_store_at_boot(&fs, fs_state),
                DeviceAccessFile::fresh()
            );
        }
    }

    #[test]
    fn a_damaged_store_is_locked_with_bluetooth_off() {
        let fs = LpFsMemory::new();
        fs.write_file(
            DeviceAccessFile::PATH.as_path(),
            b"{\"version\":2,\"open\":true}",
        )
        .unwrap();
        let store = read_device_store(&fs);
        assert_eq!(store, DeviceAccessFile::locked());
        assert!(!store.ble_enabled);
        assert_eq!(store.open, OpenTo::Nobody);
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
