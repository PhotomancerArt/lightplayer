/// The one firmware build the Studio site serves today. `esp32c6-4mb` is a
/// build-def id (`lp-fw/builds/`), and the packaged directory is named after
/// it, so a per-build path is `./firmware/<build id>/manifest.json`.
///
/// Per-build selection belongs to the provisioning picker, which does not
/// exist yet (board-selection roadmap M5). When it lands it must not grow its
/// own compatibility table: `lpa_boards::compatible_builds_for(board)` returns
/// the runnable builds for a chosen board, best fit first, and this constant
/// is the generic fallback for "no board chosen".
pub const DEFAULT_ESP32C6_FIRMWARE_MANIFEST_PATH: &str = "./firmware/esp32c6-4mb/manifest.json";
// The deployment's served-build list lives target-unconditionally at
// `crate::provider::SERVED_FIRMWARE_BUILDS` (host-built UI code reads it);
// keep it in lockstep with this path.
pub const DEFAULT_ESPTOOL_MODULE_PATH: &str = "https://cdn.jsdelivr.net/npm/esptool-js@0.6.0/+esm";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserSerialEsp32Options {
    pub firmware_manifest_path: String,
    pub esptool_module_path: Option<String>,
}

impl BrowserSerialEsp32Options {
    pub fn new(firmware_manifest_path: impl Into<String>) -> Self {
        Self {
            firmware_manifest_path: firmware_manifest_path.into(),
            esptool_module_path: None,
        }
    }

    pub fn with_esptool_module_path(mut self, esptool_module_path: impl Into<String>) -> Self {
        self.esptool_module_path = Some(esptool_module_path.into());
        self
    }

    pub(crate) fn esptool_module_path(&self) -> &str {
        self.esptool_module_path.as_deref().unwrap_or("")
    }
}

impl Default for BrowserSerialEsp32Options {
    fn default() -> Self {
        Self::new(DEFAULT_ESP32C6_FIRMWARE_MANIFEST_PATH)
            .with_esptool_module_path(DEFAULT_ESPTOOL_MODULE_PATH)
    }
}
