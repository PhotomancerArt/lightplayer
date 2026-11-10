//! The on-device identity convention: `/.lp/device.json` at the server's
//! filesystem ROOT.
//!
//! Identity is DEVICE-scoped, so it lives at the base-fs root — outside
//! every project storage dir — and survives project pushes (which only
//! replace `projects/<storage>/`). Studio stamps the file over the wire
//! (a root-path `FsRequest::Write`); embedders read it at boot to populate
//! the hello's `device_uid`, and `ClientRequest::Hello` re-reads it so a
//! post-stamp hello reports the live identity without a reboot.

extern crate alloc;

use alloc::string::String;
use lpc_model::AsLpPath;
use lpfs::LpFs;
use serde::{Deserialize, Serialize};

/// Root path of the stamped device identity file.
pub const DEVICE_IDENTITY_PATH: &str = "/.lp/device.json";

/// Read the stamped `dev…` uid from the root identity file. Missing or
/// unparseable file → `None` (an unstamped device).
pub fn read_device_uid(fs: &dyn LpFs) -> Option<String> {
    let bytes = fs.read_file(DEVICE_IDENTITY_PATH.as_path()).ok()?;
    let uid = lpc_wire::json::from_slice::<DeviceIdentityFile>(&bytes)
        .ok()
        .map(|identity| identity.uid);
    // A name-only file is a named, unstamped board — not a broken one.
    let named_only = uid.is_none()
        && lpc_wire::json::from_slice::<DeviceIdentityFields>(&bytes)
            .is_ok_and(|fields| fields.uid.is_none());
    if uid.is_none() && !named_only {
        log::warn!(
            "identity: /.lp/device.json is {}; serving as an unstamped board",
            if bytes.is_empty() {
                "empty"
            } else {
                "not a readable identity"
            }
        );
    }
    uid
}

/// The board's own name (the identity file's `name`), if a person gave it
/// one: what Studio shows, what the board advertises over Bluetooth
/// (`LP-<name>`) and what it calls itself on the cloud relay. Missing file,
/// no name or a blank one → `None`.
pub fn read_device_name(fs: &dyn LpFs) -> Option<String> {
    let bytes = fs.read_file(DEVICE_IDENTITY_PATH.as_path()).ok()?;
    lpc_wire::json::from_slice::<DeviceIdentityFields>(&bytes)
        .ok()?
        .name
        .and_then(|name| lpc_wire::clean_device_name(&name))
}

/// Write the board's name (`None` clears it) into the identity file,
/// keeping a stamped `uid` the file already holds. Returns the name as
/// stored: `name` cleaned by [`lpc_wire::clean_device_name`].
///
/// The file's shape is unchanged — `{ "uid"?, "name"? }` — and a name-only
/// file was always a readable one (an unstamped board with a name). Keys
/// other than those two are not kept: no writer ever put any there.
pub fn write_device_name(fs: &dyn LpFs, name: Option<&str>) -> Result<Option<String>, String> {
    let name = name.and_then(lpc_wire::clean_device_name);
    let uid = fs
        .read_file(DEVICE_IDENTITY_PATH.as_path())
        .ok()
        .and_then(|bytes| lpc_wire::json::from_slice::<DeviceIdentityFields>(&bytes).ok())
        .and_then(|fields| fields.uid);
    let fields = DeviceIdentityFields {
        uid,
        name: name.clone(),
    };
    let json = lpc_wire::json::to_string(&fields).map_err(|error| alloc::format!("{error}"))?;
    fs.write_file(DEVICE_IDENTITY_PATH.as_path(), json.as_bytes())
        .map_err(|error| alloc::format!("could not write {DEVICE_IDENTITY_PATH}: {error}"))?;
    Ok(name)
}

/// The identity file's server-relevant field (`uid`); the stamped `name`
/// is read on its own ([`read_device_name`]).
#[derive(Deserialize)]
struct DeviceIdentityFile {
    uid: String,
}

/// The identity file read whole and leniently: either key may be missing
/// (an unstamped board with a name, a stamped one never named).
#[derive(Deserialize, Serialize)]
struct DeviceIdentityFields {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    uid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

#[cfg(test)]
mod tests {
    use lpfs::LpFsMemory;

    use super::*;

    #[test]
    fn reads_the_stamped_uid_from_the_root_file() {
        let fs = LpFsMemory::new();
        fs.write_file(
            DEVICE_IDENTITY_PATH.as_path(),
            br#"{"uid":"dev0000000000000001","name":"Porch sign"}"#,
        )
        .unwrap();

        assert_eq!(read_device_uid(&fs).as_deref(), Some("dev0000000000000001"));
    }

    #[test]
    fn missing_or_unparseable_file_reads_as_unstamped() {
        let fs = LpFsMemory::new();
        assert_eq!(read_device_uid(&fs), None);

        fs.write_file(DEVICE_IDENTITY_PATH.as_path(), b"not json")
            .unwrap();
        assert_eq!(read_device_uid(&fs), None);
    }

    #[test]
    fn an_empty_file_reads_as_unstamped() {
        let fs = LpFsMemory::new();
        fs.write_file(DEVICE_IDENTITY_PATH.as_path(), b"").unwrap();
        assert_eq!(read_device_uid(&fs), None);
    }

    #[test]
    fn reads_the_stamped_name_and_nothing_for_none_or_blank() {
        let fs = LpFsMemory::new();
        assert_eq!(read_device_name(&fs), None);
        fs.write_file(
            DEVICE_IDENTITY_PATH.as_path(),
            br#"{"uid":"dev0000000000000001","name":"Porch sign"}"#,
        )
        .unwrap();
        assert_eq!(read_device_name(&fs).as_deref(), Some("Porch sign"));
        for file in [
            &br#"{"uid":"dev0000000000000001"}"#[..],
            br#"{"name":"  "}"#,
        ] {
            fs.write_file(DEVICE_IDENTITY_PATH.as_path(), file).unwrap();
            assert_eq!(read_device_name(&fs), None);
        }
    }

    #[test]
    fn writing_a_name_keeps_the_stamped_uid() {
        let fs = LpFsMemory::new();
        fs.write_file(
            DEVICE_IDENTITY_PATH.as_path(),
            br#"{"uid":"dev0000000000000001","name":"Porch sign"}"#,
        )
        .unwrap();
        let stored = write_device_name(&fs, Some("  Flower-Viatrix ")).unwrap();
        assert_eq!(stored.as_deref(), Some("Flower-Viatrix"));
        assert_eq!(read_device_uid(&fs).as_deref(), Some("dev0000000000000001"));
        assert_eq!(read_device_name(&fs).as_deref(), Some("Flower-Viatrix"));
    }

    #[test]
    fn naming_an_unstamped_board_writes_a_name_only_file() {
        let fs = LpFsMemory::new();
        write_device_name(&fs, Some("Porch")).unwrap();
        assert_eq!(
            fs.read_file(DEVICE_IDENTITY_PATH.as_path()).unwrap(),
            br#"{"name":"Porch"}"#
        );
        assert_eq!(read_device_uid(&fs), None);
        assert_eq!(write_device_name(&fs, Some("  ")).unwrap(), None);
        assert_eq!(read_device_name(&fs), None);
    }
}
