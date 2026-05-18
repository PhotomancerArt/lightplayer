extern crate alloc;

use alloc::string::{String, ToString};
use core::str;

use lpc_shared::hardware::{
    HardwareManifest, HardwareManifestFile, default_esp32c6_hardware_manifest,
};
use lpfs::LpFs;
use lpfs::lp_path::AsLpPath;

const HARDWARE_MANIFEST_PATH: &str = "/hardware.toml";

pub fn load_hardware_manifest(fs: &dyn LpFs) -> HardwareManifest {
    match fs.read_file(HARDWARE_MANIFEST_PATH.as_path()) {
        Ok(bytes) => parse_override(&bytes).unwrap_or_else(|message| {
            log::warn!(
                "hardware manifest override at {HARDWARE_MANIFEST_PATH} is invalid: {message}; using compiled default"
            );
            default_esp32c6_hardware_manifest()
        }),
        Err(lpfs::FsError::NotFound(_)) => default_esp32c6_hardware_manifest(),
        Err(error) => {
            log::warn!(
                "failed to read hardware manifest override at {HARDWARE_MANIFEST_PATH}: {error}; using compiled default"
            );
            default_esp32c6_hardware_manifest()
        }
    }
}

fn parse_override(bytes: &[u8]) -> Result<HardwareManifest, String> {
    let text = str::from_utf8(bytes).map_err(|error| error.to_string())?;
    HardwareManifestFile::read_toml(text)
        .and_then(|manifest| manifest.to_manifest())
        .map_err(|error| error.to_string())
}
