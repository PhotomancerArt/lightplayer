pub const DEFAULT_ESP32C6_FIRMWARE_MANIFEST_PATH: &str = "./firmware/esp32c6/manifest.json";
pub const DEFAULT_ESPTOOL_MODULE_PATH: &str = "https://unpkg.com/esptool-js@0.6.0/lib/index.js";

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
