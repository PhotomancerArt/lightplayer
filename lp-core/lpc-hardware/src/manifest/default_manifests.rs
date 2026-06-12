use crate::{HwManifest, HardwareManifestFile};

const XIAO_ESP32_C6_TOML: &str = include_str!("../../boards/seeed/xiao-esp32-c6.toml");

pub fn default_esp32c6_hardware_manifest() -> HwManifest {
    HardwareManifestFile::read_toml(XIAO_ESP32_C6_TOML)
        .and_then(|manifest| manifest.to_manifest())
        .expect("checked-in seeed/xiao-esp32-c6 hardware manifest must parse")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HwAddress;

    #[test]
    fn default_esp32c6_manifest_loads_checked_in_board_profile() {
        let manifest = default_esp32c6_hardware_manifest();

        assert_eq!(manifest.board_id(), "seeed/xiao-esp32-c6");
        assert!(manifest.resource(&HwAddress::gpio(18)).is_some());
        assert!(manifest.resource(&HwAddress::rmt_ws281x(0)).is_some());
        assert!(
            manifest
                .resource(&HwAddress::gpio(12))
                .and_then(|resource| resource.reserved_reason())
                .is_some()
        );
    }
}
