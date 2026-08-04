/// Where the Studio site publishes packaged firmware. `lp-cli firmware
/// package <id>` writes `firmware/<id>/`, so a build's manifest is always
/// `<base>/<build id>/manifest.json`.
pub const DEFAULT_FIRMWARE_BASE_PATH: &str = "./firmware";

pub const DEFAULT_ESPTOOL_MODULE_PATH: &str = "https://cdn.jsdelivr.net/npm/esptool-js@0.6.0/+esm";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserSerialEsp32Options {
    /// Directory the packaged `firmware/<build id>/` trees live under.
    pub firmware_base_path: String,
    pub esptool_module_path: Option<String>,
}

impl BrowserSerialEsp32Options {
    pub fn new(firmware_base_path: impl Into<String>) -> Self {
        Self {
            firmware_base_path: firmware_base_path.into(),
            esptool_module_path: None,
        }
    }

    pub fn with_esptool_module_path(mut self, esptool_module_path: impl Into<String>) -> Self {
        self.esptool_module_path = Some(esptool_module_path.into());
        self
    }

    /// The manifest URL for `build_id`.
    ///
    /// There is deliberately NO default: a flash request that names no
    /// build is refused upstream (Yona, 2026-08-03 — "there shouldn't be a
    /// fallback for firmware. either it matches, or its a fail case").
    /// A deployment default meant a classic ESP32 with no chip evidence
    /// got aimed at the C6 image, and only the flash-time chip guard
    /// caught it — as a confusing refusal that named a build nobody chose.
    pub fn firmware_manifest_path(&self, build_id: &str) -> String {
        format!(
            "{}/{build_id}/manifest.json",
            self.firmware_base_path.trim_end_matches('/')
        )
    }

    pub(crate) fn esptool_module_path(&self) -> &str {
        self.esptool_module_path.as_deref().unwrap_or("")
    }
}

impl Default for BrowserSerialEsp32Options {
    fn default() -> Self {
        Self::new(DEFAULT_FIRMWARE_BASE_PATH).with_esptool_module_path(DEFAULT_ESPTOOL_MODULE_PATH)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_build_id_names_its_packaged_directory() {
        let options = BrowserSerialEsp32Options::default();
        assert_eq!(
            options.firmware_manifest_path("esp32s3-8mb"),
            "./firmware/esp32s3-8mb/manifest.json"
        );
    }

    #[test]
    fn a_trailing_slash_on_the_base_does_not_double_up() {
        let options = BrowserSerialEsp32Options::new("/assets/firmware/");
        assert_eq!(
            options.firmware_manifest_path("esp32v3-4mb"),
            "/assets/firmware/esp32v3-4mb/manifest.json"
        );
    }
}
