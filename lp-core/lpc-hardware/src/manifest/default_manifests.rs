use alloc::collections::BTreeMap;

use crate::{
    HardwareBoardLabelStatus, HardwareManifestFile, HardwareTarget, HwAddress, HwManifest,
    HwResource,
};

const XIAO_ESP32_C6_JSON: &str = include_str!("../../boards/seeed/xiao-esp32-c6.json");
const XIAO_ESP32_S3_PLUS_JSON: &str = include_str!("../../boards/seeed/xiao-esp32-s3-plus.json");
const DOM_Z_102_JSON: &str = include_str!("../../boards/domraem/dom-z-102.json");

pub fn default_esp32c6_hardware_manifest() -> HwManifest {
    HardwareManifestFile::read_json(XIAO_ESP32_C6_JSON)
        .and_then(|manifest| manifest.to_manifest())
        .expect("checked-in seeed/xiao-esp32-c6 board manifest must parse")
}

/// Compiled-in board profile for `fw-esp32s3`.
///
/// The profile is deliberately **incomplete**: a missing entry is a gap someone
/// fills later, whereas a wrong GPIO number reaches a soldering iron. Absent on
/// purpose, and pinned by
/// [`tests::default_esp32s3_manifest_omits_unverified_and_in_package_pins`]:
///
/// - **The user LED — tested, and GPIO21 is ruled out.** The Seeed wiki gives
///   GPIO21 for the plain XIAO ESP32-S3; espboards.dev gives GPIO22 for the
///   *Plus*, which cannot be right — the ESP32-S3 has no GPIO22 (it numbers
///   GPIO0-21 and GPIO26-48). GPIO21 was then driven on the desk board in a
///   3-blink/pause pattern for 20 s (2026-07-30) with **no visible change**,
///   so it is not the user LED on this variant either. The board's yellow LED
///   appears to be a power/charge indicator on no GPIO we drive. Settling it
///   wants the Seeed schematic or an S3 `test_gpio_calibrate` harness, not a
///   third guess.
/// - **The nine 1.27 mm castellated pads** the Plus adds over the plain XIAO.
///   No source publishes their GPIO numbers.
/// - **In-package flash and PSRAM (GPIO26-GPIO37).** Not header pins and never
///   claimable; this part carries octal PSRAM, which uses the widest set.
/// - **A radio resource.** `fw-esp32s3` registers no radio driver, so
///   advertising `/radio/0` would offer a resource nothing can open.
pub fn default_esp32s3_hardware_manifest() -> HwManifest {
    HardwareManifestFile::read_json(XIAO_ESP32_S3_PLUS_JSON)
        .and_then(|manifest| manifest.to_manifest())
        .expect("checked-in seeed/xiao-esp32-s3-plus board manifest must parse")
}

/// Compiled-in board profile for `fw-esp32v3` (classic ESP32, LX6).
///
/// The DOM-Z-102 — the classic bring-up roadmap's desk board and its
/// WLED-class deployment exemplar — rather than a bare devkit: the roadmap's
/// Q5-as-amended decision (see the plan's notes.md, 2026-07-31). Data
/// channels LED1-4 = GPIO 18/16/14/2, matching the board silkscreen and the
/// display sidecar authored from a photo of the desk unit. Like the S3
/// profile above, deliberately incomplete: only pins the board exposes are
/// listed, and UART0's GPIO1/3 are reserved because they ARE the host link
/// (CH340K bridge — no separate USB-Serial-JTAG on this chip).
pub fn default_esp32v3_hardware_manifest() -> HwManifest {
    HardwareManifestFile::read_json(DOM_Z_102_JSON)
        .and_then(|manifest| manifest.to_manifest())
        .expect("checked-in domraem/dom-z-102 hardware manifest must parse")
}

/// Emulator manifest: XIAO ESP32-C6 pin map, board D-labels for endpoint specs,
/// and no reserved GPIOs so projects like fyeah-sign load without hardware errors.
pub fn permissive_emu_hardware_manifest() -> HwManifest {
    let file = HardwareManifestFile::read_json(XIAO_ESP32_C6_JSON)
        .expect("checked-in seeed/xiao-esp32-c6 board manifest must parse");
    let board_labels = assigned_board_label_by_gpio(&file);

    default_esp32c6_hardware_manifest()
        .map_resources(|resource| normalize_resource_for_emu(resource, &board_labels))
        .with_target(HardwareTarget::Rv32imacEmu)
        .with_description(
            "Emulator board profile: production D-pin labels, all GPIOs available, \
             virtual WS281x/RMT and ESP-NOW radio drivers.",
        )
}

fn assigned_board_label_by_gpio(
    file: &HardwareManifestFile,
) -> BTreeMap<HwAddress, alloc::string::String> {
    let mut labels = BTreeMap::new();
    for entry in &file.board_label {
        if entry.status != Some(HardwareBoardLabelStatus::Assigned) {
            continue;
        }
        let Some(gpio_path) = &entry.gpio else {
            continue;
        };
        let Ok(address) = HwAddress::new(gpio_path.clone()) else {
            continue;
        };
        labels.insert(address, entry.label.clone());
    }
    labels
}

fn normalize_resource_for_emu(
    resource: HwResource,
    board_labels: &BTreeMap<HwAddress, alloc::string::String>,
) -> HwResource {
    let mut resource = resource.clear_reservation();
    if let Some(label) = board_labels.get(resource.address()) {
        resource = resource.with_display_label(label.clone());
    }
    resource
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{HardwareSystem, HwEndpointSpec, HwRegistry};

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

    #[test]
    fn default_esp32s3_manifest_loads_checked_in_board_profile() {
        let manifest = default_esp32s3_hardware_manifest();

        assert_eq!(manifest.board_id(), "seeed/xiao-esp32-s3-plus");
        assert_eq!(manifest.target(), Some(HardwareTarget::Esp32s3));
        // D10, the header pin the C6 profile uses for WS281x output.
        assert!(manifest.resource(&HwAddress::gpio(9)).is_some());
        // Four RMT timing resources, one per ESP32-S3 TX channel: the count of
        // these is what `Esp32S3RmtWs281xDriver` offers, so a manifest that
        // lost one would silently drop an output.
        for channel in 0..4 {
            assert!(
                manifest.resource(&HwAddress::rmt_ws281x(channel)).is_some(),
                "/rmt/ws281x{channel} must be declared"
            );
        }
        // GPIO19/20 are USB-Serial-JTAG D-/D+. They are listed only so a
        // driver cannot claim them: on this board that costs a physical replug.
        for pin in [0, 19, 20] {
            assert!(
                manifest
                    .resource(&HwAddress::gpio(pin))
                    .and_then(|resource| resource.reserved_reason())
                    .is_some(),
                "gpio{pin} must be reserved"
            );
        }
    }

    #[test]
    fn default_esp32v3_manifest_loads_checked_in_board_profile() {
        let manifest = default_esp32v3_hardware_manifest();

        assert_eq!(manifest.board_id(), "domraem/dom-z-102");
        assert_eq!(manifest.target(), Some(HardwareTarget::Esp32));
        // The four silkscreened data channels, LED1-4.
        for pin in [18, 16, 14, 2] {
            assert!(
                manifest.resource(&HwAddress::gpio(pin)).is_some(),
                "gpio{pin} (a LED data channel) must be declared"
            );
        }
        // Four RMT resources — the count the classic ws281x backend offers
        // (ADR 2026-07-31-lp-ws281x-multi-channel-driver-adoption: channel
        // count comes from here, never from driver logic).
        for channel in 0..4 {
            assert!(
                manifest.resource(&HwAddress::rmt_ws281x(channel)).is_some(),
                "/rmt/ws281x{channel} must be declared"
            );
        }
        // GPIO1/3 are UART0 through the CH340K bridge — the host link — and
        // GPIO0 is the OPT/boot button; all reserved so no driver claims them.
        for pin in [0, 1, 3] {
            assert!(
                manifest
                    .resource(&HwAddress::gpio(pin))
                    .and_then(|resource| resource.reserved_reason())
                    .is_some(),
                "gpio{pin} must be reserved"
            );
        }
    }

    /// The top-right screw terminal is IO13, silkscreened "spare" — read off
    /// the physical board and then driven on silicon (2026-08-02: WS281x
    /// frames byte-identical to the host oracle, strip lit). This corrects an
    /// earlier entry that called it IO12/relay and withheld it as the MTDI
    /// strap; GPIO13 is MTCK, not a strap, so that objection was against a
    /// pin this board does not expose.
    #[test]
    fn default_esp32v3_manifest_offers_io13_spare_terminal() {
        let manifest = default_esp32v3_hardware_manifest();

        let io13 = manifest
            .resource(&HwAddress::gpio(13))
            .expect("gpio13 (the spare screw terminal) must be declared");
        assert_eq!(io13.display_label(), "IO13");
        assert!(
            io13.reserved_reason().is_none(),
            "IO13 is claimable — it carries no strap/boot risk"
        );
        // The wrong entry must not come back: GPIO12 (MTDI strap) is not a
        // terminal on this board.
        assert!(
            manifest.resource(&HwAddress::gpio(12)).is_none(),
            "gpio12 is not exposed by this board and must stay absent"
        );
        // And no fifth /rmt/ws281xK rides along: plan_for_declared(5) gives
        // every channel 1 block — a 40 µs refill deadline against a start-path
        // masking cost measured at up to 69 µs on this board
        // (docs/adr/2026-08-02-classic-hli-refill.md). Declaring a fifth
        // channel would not add one; it would break the working four.
        assert!(
            manifest.resource(&HwAddress::rmt_ws281x(4)).is_none(),
            "a 5th RMT channel cannot be deadline-funded on the classic"
        );
    }

    /// The omissions are the point of this profile, so they are asserted rather
    /// than left to a comment: each pin here is one somebody could plausibly
    /// guess into the manifest, and guessing wrong shorts a pin.
    #[test]
    fn default_esp32s3_manifest_omits_unverified_and_in_package_pins() {
        let manifest = default_esp32s3_hardware_manifest();

        // 21: the contested user LED. 26-37: in-package flash and octal PSRAM.
        // 45/46: strapping pins, not on the header. 10-18, 38-42, 47/48: the
        // Plus's castellated pads have no published GPIO map, so no raw pin is
        // offered speculatively.
        let omitted = (10..=18)
            .chain(core::iter::once(21))
            .chain(26..=42)
            .chain(47..=48);
        for pin in omitted {
            assert!(
                manifest.resource(&HwAddress::gpio(pin)).is_none(),
                "gpio{pin} is not verified on this board and must stay absent"
            );
        }
    }

    #[test]
    fn permissive_emu_manifest_uses_board_d_labels_and_clears_reservations() {
        let manifest = permissive_emu_hardware_manifest();

        assert_eq!(manifest.board_id(), "seeed/xiao-esp32-c6");
        assert_eq!(
            manifest
                .resource(&HwAddress::gpio(20))
                .expect("gpio20")
                .display_label(),
            "D9"
        );
        assert_eq!(
            manifest
                .resource(&HwAddress::gpio(18))
                .expect("gpio18")
                .display_label(),
            "D10"
        );
        assert!(
            manifest
                .resource(&HwAddress::gpio(12))
                .expect("gpio12")
                .reserved_reason()
                .is_none()
        );
    }

    #[test]
    fn permissive_emu_manifest_opens_fyeah_sign_endpoints() {
        use alloc::rc::Rc;

        let registry = Rc::new(HwRegistry::new(permissive_emu_hardware_manifest()));
        let system = HardwareSystem::with_virtual_drivers(registry);

        system
            .open_button_by_spec(
                &HwEndpointSpec::from_static("button:local:D9"),
                crate::ButtonConfig::new(30),
            )
            .expect("button D9");
        system
            .open_ws281x_by_spec(
                &HwEndpointSpec::from_static("ws281x:local:D10"),
                crate::Ws281xConfig::new(3),
            )
            .expect("ws281x D10");
        system
            .open_radio_by_spec(
                &HwEndpointSpec::from_static("radio:local:0"),
                crate::RadioConfig::default(),
            )
            .expect("radio espnow");
    }
}
