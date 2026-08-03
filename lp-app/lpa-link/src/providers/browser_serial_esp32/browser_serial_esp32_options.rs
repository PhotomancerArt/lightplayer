/// Where the Studio site publishes packaged firmware. `lp-cli firmware
/// package <id>` writes `firmware/<id>/`, so a build's manifest is always
/// `<base>/<build id>/manifest.json`.
pub const DEFAULT_FIRMWARE_BASE_PATH: &str = "./firmware";

/// The build a flash request with no `build_id` gets.
///
/// This is the DEPLOYMENT DEFAULT, not "the image Studio flashes": which
/// build a device should receive is computed per request from its chip and
/// the picked board (`lpa_boards::provisioning_build_id`). The default only
/// covers a request that arrives with nothing known about the endpoint —
/// and the flash-time chip guard refuses it when the endpoint turns out to
/// be another ISA, so a wrong default is a clear error rather than a
/// silently wrong image.
///
/// Which builds are published at all is `lp-fw/builds/served.json`
/// (`lpa_boards::served_build_ids`); this must name one of them.
pub const DEFAULT_FIRMWARE_BUILD_ID: &str = "esp32c6-4mb";

pub const DEFAULT_ESPTOOL_MODULE_PATH: &str = "https://cdn.jsdelivr.net/npm/esptool-js@0.6.0/+esm";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserSerialEsp32Options {
    /// Directory the packaged `firmware/<build id>/` trees live under.
    pub firmware_base_path: String,
    /// Build flashed when a request names none.
    pub default_build_id: String,
    pub esptool_module_path: Option<String>,
}

impl BrowserSerialEsp32Options {
    pub fn new(firmware_base_path: impl Into<String>) -> Self {
        Self {
            firmware_base_path: firmware_base_path.into(),
            default_build_id: DEFAULT_FIRMWARE_BUILD_ID.to_string(),
            esptool_module_path: None,
        }
    }

    pub fn with_default_build_id(mut self, default_build_id: impl Into<String>) -> Self {
        self.default_build_id = default_build_id.into();
        self
    }

    pub fn with_esptool_module_path(mut self, esptool_module_path: impl Into<String>) -> Self {
        self.esptool_module_path = Some(esptool_module_path.into());
        self
    }

    /// The manifest URL for `build_id`, or for the deployment default when
    /// the request named none.
    pub fn firmware_manifest_path(&self, build_id: Option<&str>) -> String {
        let build_id = build_id.unwrap_or(&self.default_build_id);
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
            options.firmware_manifest_path(Some("esp32s3-8mb")),
            "./firmware/esp32s3-8mb/manifest.json"
        );
        // No build named: the deployment default, not a guess.
        assert_eq!(
            options.firmware_manifest_path(None),
            "./firmware/esp32c6-4mb/manifest.json"
        );
    }

    #[test]
    fn a_trailing_slash_on_the_base_does_not_double_up() {
        let options = BrowserSerialEsp32Options::new("/assets/firmware/");
        assert_eq!(
            options.firmware_manifest_path(Some("esp32v3-4mb")),
            "/assets/firmware/esp32v3-4mb/manifest.json"
        );
    }
}
