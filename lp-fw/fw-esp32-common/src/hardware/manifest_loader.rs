use alloc::string::{String, ToString};
use core::str;

use lpc_hardware::{HardwareManifestFile, HwManifest};
use lpfs::LpFs;
use lpfs::lp_path::AsLpPath;

const HARDWARE_MANIFEST_PATH: &str = "/hardware.json";

/// Load the on-device hardware manifest, falling back to the chip crate's
/// compiled-in default when the override is absent or invalid.
pub fn load_hardware_manifest(fs: &dyn LpFs, fallback: fn() -> HwManifest) -> HwManifest {
    match fs.read_file(HARDWARE_MANIFEST_PATH.as_path()) {
        Ok(bytes) => parse_override(&bytes).unwrap_or_else(|message| {
            log::warn!(
                "hardware manifest override at {HARDWARE_MANIFEST_PATH} is invalid: {message}; using compiled default"
            );
            fallback()
        }),
        Err(lpfs::FsError::NotFound(_)) => fallback(),
        Err(error) => {
            log::warn!(
                "failed to read hardware manifest override at {HARDWARE_MANIFEST_PATH}: {error}; using compiled default"
            );
            fallback()
        }
    }
}

fn parse_override(bytes: &[u8]) -> Result<HwManifest, String> {
    let text = str::from_utf8(bytes).map_err(|error| error.to_string())?;
    HardwareManifestFile::read_json(text)
        .and_then(|manifest| manifest.to_manifest())
        .map_err(|error| error.to_string())
}
