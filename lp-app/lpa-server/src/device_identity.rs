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
use serde::Deserialize;

/// Root path of the stamped device identity file.
pub const DEVICE_IDENTITY_PATH: &str = "/.lp/device.json";

/// Read the stamped `dev…` uid from the root identity file. Missing or
/// unparseable file → `None` (an unstamped device).
pub fn read_device_uid(fs: &dyn LpFs) -> Option<String> {
    let bytes = fs.read_file(DEVICE_IDENTITY_PATH.as_path()).ok()?;
    lpc_wire::json::from_slice::<DeviceIdentityFile>(&bytes)
        .ok()
        .map(|identity| identity.uid)
}

/// The stamped device name (the identity file's `name`), if any: what the
/// board calls itself to people where Studio is not there to say it — the
/// C6's hello on the cloud relay (Wi-Fi relay plan P8). Missing file, no
/// name or an empty one → `None`.
pub fn read_device_name(fs: &dyn LpFs) -> Option<String> {
    let bytes = fs.read_file(DEVICE_IDENTITY_PATH.as_path()).ok()?;
    lpc_wire::json::from_slice::<DeviceNameFile>(&bytes)
        .ok()?
        .name
        .filter(|name| !name.trim().is_empty())
}

/// The identity file's server-relevant field (`uid`); the stamped `name`
/// and any future fields are Studio-side display data and stay unparsed.
#[derive(Deserialize)]
struct DeviceIdentityFile {
    uid: String,
}

/// The identity file's `name`, read on its own (an unstamped-uid file with
/// a name still names the board).
#[derive(Deserialize)]
struct DeviceNameFile {
    #[serde(default)]
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
}
