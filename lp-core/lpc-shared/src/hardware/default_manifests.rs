use alloc::collections::BTreeMap;

use super::{
    HardwareAddress, HardwareBoardLabelStatus, HardwareManifest, HardwareManifestFile,
    HardwareResource,
};

const XIAO_ESP32_C6_TOML: &str = include_str!("../../boards/seeed/xiao-esp32-c6.toml");

pub fn default_esp32c6_hardware_manifest() -> HardwareManifest {
    HardwareManifestFile::read_toml(XIAO_ESP32_C6_TOML)
        .and_then(|manifest| manifest.to_manifest())
        .expect("checked-in seeed/xiao-esp32-c6 hardware manifest must parse")
}

/// Emulator manifest: XIAO ESP32-C6 pin map, board D-labels for endpoint specs,
/// and no reserved GPIOs so projects like fyeah-sign load without hardware errors.
pub fn permissive_emu_hardware_manifest() -> HardwareManifest {
    let file = HardwareManifestFile::read_toml(XIAO_ESP32_C6_TOML)
        .expect("checked-in seeed/xiao-esp32-c6 hardware manifest must parse");
    let board_labels = assigned_board_label_by_gpio(&file);

    default_esp32c6_hardware_manifest()
        .map_resources(|resource| normalize_resource_for_emu(resource, &board_labels))
        .with_target(super::HardwareTarget::Rv32imacEmu)
        .with_description(
            "Emulator board profile: production D-pin labels, all GPIOs available, \
             virtual WS281x/RMT and ESP-NOW radio drivers.",
        )
}

fn assigned_board_label_by_gpio(
    file: &HardwareManifestFile,
) -> BTreeMap<HardwareAddress, alloc::string::String> {
    let mut labels = BTreeMap::new();
    for entry in &file.board_label {
        if entry.status != Some(HardwareBoardLabelStatus::Assigned) {
            continue;
        }
        let Some(gpio_path) = &entry.gpio else {
            continue;
        };
        let Ok(address) = HardwareAddress::new(gpio_path.clone()) else {
            continue;
        };
        labels.insert(address, entry.label.clone());
    }
    labels
}

fn normalize_resource_for_emu(
    resource: HardwareResource,
    board_labels: &BTreeMap<HardwareAddress, alloc::string::String>,
) -> HardwareResource {
    let mut resource = resource.clear_reservation();
    if let Some(label) = board_labels.get(resource.address()) {
        resource = resource.with_display_label(label.clone());
    }
    resource
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware::{HardwareAddress, HardwareEndpointSpec, HardwareRegistry, HardwareSystem};

    #[test]
    fn default_esp32c6_manifest_loads_checked_in_board_profile() {
        let manifest = default_esp32c6_hardware_manifest();

        assert_eq!(manifest.board_id(), "seeed/xiao-esp32-c6");
        assert!(manifest.resource(&HardwareAddress::gpio(18)).is_some());
        assert!(manifest.resource(&HardwareAddress::rmt_ws281x(0)).is_some());
        assert!(
            manifest
                .resource(&HardwareAddress::gpio(12))
                .and_then(|resource| resource.reserved_reason())
                .is_some()
        );
    }

    #[test]
    fn permissive_emu_manifest_uses_board_d_labels_and_clears_reservations() {
        let manifest = permissive_emu_hardware_manifest();

        assert_eq!(manifest.board_id(), "seeed/xiao-esp32-c6");
        assert_eq!(
            manifest
                .resource(&HardwareAddress::gpio(20))
                .expect("gpio20")
                .display_label(),
            "D9"
        );
        assert_eq!(
            manifest
                .resource(&HardwareAddress::gpio(18))
                .expect("gpio18")
                .display_label(),
            "D10"
        );
        assert!(
            manifest
                .resource(&HardwareAddress::gpio(12))
                .expect("gpio12")
                .reserved_reason()
                .is_none()
        );
    }

    #[test]
    fn permissive_emu_manifest_opens_fyeah_sign_endpoints() {
        use alloc::rc::Rc;

        let registry = Rc::new(HardwareRegistry::new(permissive_emu_hardware_manifest()));
        let system = HardwareSystem::with_virtual_drivers(registry);

        system
            .open_button_by_spec(
                &HardwareEndpointSpec::from_static("button:gpio:D9"),
                super::super::ButtonConfig::new(30),
            )
            .expect("button D9");
        system
            .open_ws281x_by_spec(
                &HardwareEndpointSpec::from_static("ws281x:rmt:D10"),
                super::super::Ws281xConfig::new(3, None),
            )
            .expect("ws281x D10");
        system
            .open_radio_by_spec(
                &HardwareEndpointSpec::from_static("radio:espnow:0"),
                super::super::RadioConfig::default(),
            )
            .expect("radio espnow");
    }
}
