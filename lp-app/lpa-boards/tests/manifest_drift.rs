//! Drift gate between display sidecars and runtime board manifests.
//!
//! The runtime manifest (`<vendor>/<product>.json`, compiled into firmware) is
//! the authority on claimable resources; the display sidecar
//! (`<vendor>/<product>.display.json`) is presentation. These tests keep the
//! two from silently disagreeing — a wrong GPIO in either file is a
//! physical-damage class of mistake (see boards/README.md).

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use lpa_boards::{BoardDisplayFile, PinRole};
use lpc_hardware::manifest::hw_manifest_file::{HardwareBoardLabelFile, HardwareResourceFile};
use lpc_hardware::{HardwareBoardLabelStatus, HardwareManifestFile};

/// Boards with display sidecars but deliberately no runtime manifest, with
/// the reason. Shrink this list as firmware targets land.
const DISPLAY_ONLY: &[(&str, &str)] = &[
    (
        "espressif/esp32-devkitc-v4",
        "no devkit on the desk; classic target (HardwareTarget::Esp32) landed \
         2026-07-31 — manifest when one needs calibrating",
    ),
    (
        "quinled/dig-uno",
        "no board on the desk to verify GPIOs against; classic target landed \
         2026-07-31",
    ),
];

/// The mirror case: runtime manifests whose display sidecar is landing on a
/// different in-flight branch, with the reason. Strict like DISPLAY_ONLY —
/// once the sidecar merges, the entry must be removed.
const RUNTIME_ONLY: &[(&str, &str)] = &[];

fn boards_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../lp-core/lpc-hardware/boards")
}

/// `board_id -> (display?, runtime?)` walked from the boards directory.
fn manifest_pairs() -> BTreeMap<String, (Option<BoardDisplayFile>, Option<HardwareManifestFile>)> {
    let mut pairs: BTreeMap<String, (Option<BoardDisplayFile>, Option<HardwareManifestFile>)> =
        BTreeMap::new();
    for vendor_entry in fs::read_dir(boards_dir()).expect("boards dir") {
        let vendor_dir = vendor_entry.expect("vendor entry").path();
        if !vendor_dir.is_dir() {
            continue;
        }
        let vendor = vendor_dir
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        for file_entry in fs::read_dir(&vendor_dir).expect("vendor dir") {
            let path = file_entry.expect("file entry").path();
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            let text = fs::read_to_string(&path).expect("read manifest");
            if let Some(stem) = name.strip_suffix(".display.json") {
                let display = BoardDisplayFile::read_json(&text)
                    .unwrap_or_else(|error| panic!("{vendor}/{name}: {error}"));
                assert_eq!(
                    display.board_id,
                    format!("{vendor}/{stem}"),
                    "{vendor}/{name}: board_id must match its path"
                );
                let key = display.board_id.clone();
                pairs.entry(key).or_default().0 = Some(display);
            } else if let Some(stem) = name.strip_suffix(".json") {
                let runtime = HardwareManifestFile::read_json(&text)
                    .unwrap_or_else(|error| panic!("{vendor}/{name}: {error}"));
                runtime
                    .validate()
                    .unwrap_or_else(|error| panic!("{vendor}/{name}: {error}"));
                assert_eq!(
                    runtime.id,
                    format!("{vendor}/{stem}"),
                    "{vendor}/{name}: id must match its path"
                );
                let key = runtime.id.clone();
                pairs.entry(key).or_default().1 = Some(runtime);
            }
        }
    }
    pairs
}

#[test]
fn every_board_has_both_files_or_a_recorded_reason() {
    for (board_id, (display, runtime)) in manifest_pairs() {
        let runtime_only = RUNTIME_ONLY.iter().any(|(id, _)| *id == board_id);
        match (&display, runtime_only) {
            (None, false) => panic!(
                "{board_id}: runtime manifest has no display sidecar and no RUNTIME_ONLY \
                 reason — the catalog can't show it"
            ),
            (Some(_), true) => {
                panic!("{board_id}: has a display sidecar — remove it from RUNTIME_ONLY")
            }
            _ => {}
        }
        let allowlisted = DISPLAY_ONLY.iter().any(|(id, _)| *id == board_id);
        match (&runtime, allowlisted) {
            (None, false) => panic!(
                "{board_id}: display sidecar has no runtime manifest and no DISPLAY_ONLY reason"
            ),
            (Some(_), true) => {
                panic!("{board_id}: has a runtime manifest — remove it from DISPLAY_ONLY")
            }
            _ => {}
        }
    }
}

#[test]
fn embedded_catalog_matches_the_directory() {
    let pairs = manifest_pairs();
    let embedded: Vec<&str> = lpa_boards::DISPLAY_MANIFEST_SOURCES
        .iter()
        .map(|(id, _)| *id)
        .collect();
    // The embedded catalog mirrors DISPLAY sidecars; runtime-only boards
    // (RUNTIME_ONLY above) have nothing to embed yet.
    let with_display: Vec<&String> = pairs
        .iter()
        .filter(|(_, (display, _))| display.is_some())
        .map(|(id, _)| id)
        .collect();
    for board_id in &with_display {
        assert!(
            embedded.contains(&board_id.as_str()),
            "{board_id}: display sidecar exists on disk but is not embedded in lpa_boards::catalog"
        );
    }
    assert_eq!(
        embedded.len(),
        with_display.len(),
        "embedded catalog lists a board with no on-disk display sidecar"
    );
    // And the embedded bytes are the on-disk bytes (include_str! path typos).
    for (board_id, source) in lpa_boards::DISPLAY_MANIFEST_SOURCES {
        let path = boards_dir().join(format!("{board_id}.display.json"));
        let on_disk = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("{board_id}: reading {path:?}: {error}"));
        assert_eq!(
            *source, on_disk,
            "{board_id}: embedded source differs from the on-disk sidecar"
        );
    }
}

/// Same completeness + byte-identity gate for the embedded RUNTIME
/// manifests (provisioning writes these to devices as `/hardware.json` —
/// a stale or missing embed ships the wrong pin map).
#[test]
fn embedded_runtime_manifests_match_the_directory() {
    let pairs = manifest_pairs();
    let with_runtime: Vec<&String> = pairs
        .iter()
        .filter(|(_, (_, runtime))| runtime.is_some())
        .map(|(id, _)| id)
        .collect();
    let embedded: Vec<&str> = lpa_boards::RUNTIME_MANIFEST_SOURCES
        .iter()
        .map(|(id, _)| *id)
        .collect();
    for board_id in &with_runtime {
        assert!(
            embedded.contains(&board_id.as_str()),
            "{board_id}: runtime manifest exists on disk but is not embedded in \
             lpa_boards::RUNTIME_MANIFEST_SOURCES"
        );
    }
    assert_eq!(
        embedded.len(),
        with_runtime.len(),
        "RUNTIME_MANIFEST_SOURCES lists a board with no on-disk runtime manifest"
    );
    for (board_id, source) in lpa_boards::RUNTIME_MANIFEST_SOURCES {
        let path = boards_dir().join(format!("{board_id}.json"));
        let on_disk = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("{board_id}: reading {path:?}: {error}"));
        assert_eq!(
            *source, on_disk,
            "{board_id}: embedded runtime manifest differs from the on-disk file"
        );
    }
}

/// Every catalog board must say where the pixels plug in: project
/// generation (P03 of the gallery rework) authors
/// `ws281x:local:<default wire>`, so a board with no stated wire cannot be
/// generated for. Structural validity (the name is an output-eligible pin
/// or terminal) is `BoardDisplayFile::validate`'s job; this adds the
/// completeness gate and the runtime cross-check.
#[test]
fn every_board_declares_a_default_led_wire_the_runtime_manifest_allows() {
    for (board_id, (display, runtime)) in manifest_pairs() {
        let Some(display) = display else { continue };
        let Some(first) = display.default_led_wire() else {
            panic!(
                "{board_id}: no default_led_wires — the setup flow cannot generate a \
                 first project for this board"
            );
        };
        let wires: Vec<(&str, u8)> = display.output_wires().collect();
        let gpio = wires
            .iter()
            .find(|(label, _)| *label == first)
            .map(|(_, gpio)| *gpio)
            .unwrap_or_else(|| panic!("{board_id}: default wire {first} has no gpio"));
        let Some(runtime) = runtime else { continue };
        for wire in &display.default_led_wires {
            let gpio = wires
                .iter()
                .find(|(label, _)| label == wire)
                .map(|(_, gpio)| *gpio)
                .unwrap_or_else(|| panic!("{board_id}: default wire {wire} has no gpio"));
            let resource = runtime
                .gpio
                .iter()
                .find(|resource| resource.address == format!("/gpio/{gpio}"))
                .unwrap_or_else(|| {
                    panic!(
                        "{board_id}: default LED wire {wire} (gpio {gpio}) is not a claimable \
                         resource in the runtime manifest"
                    )
                });
            assert!(
                resource.reserved_reason.is_none(),
                "{board_id}: default LED wire {wire} (gpio {gpio}) is reserved in the runtime \
                 manifest ({:?}) — generation would author an endpoint the firmware refuses",
                resource.reserved_reason
            );
        }
        // The first wire is the one single-output generation takes.
        assert_eq!(
            display.default_led_wire(),
            Some(first),
            "{board_id}: default wire is the head of the list (gpio {gpio})"
        );
    }
}

/// A power-gate pin is never a wire. The descriptor's whole promise is that
/// the LED rail hangs off this GPIO, so the runtime manifest must declare it
/// **and** reserve it (no driver may claim it), the resources it feeds must
/// exist, and the catalog must not offer it as somewhere to plug pixels in.
#[test]
fn power_gate_pins_are_declared_reserved_and_never_wires() {
    for (board_id, (display, runtime)) in manifest_pairs() {
        let Some(runtime) = runtime else { continue };
        for gate in &runtime.power_gate {
            let resource = runtime
                .gpio
                .iter()
                .find(|resource| resource.address == gate.gpio)
                .unwrap_or_else(|| {
                    panic!(
                        "{board_id}: power gate names {} but no gpio resource declares it",
                        gate.gpio
                    )
                });
            assert!(
                resource.reserved_reason.is_some(),
                "{board_id}: power-gate pin {} is claimable — a driver could take the pin \
                 the output rail hangs on",
                gate.gpio
            );
            for feed in &gate.feeds {
                assert!(
                    runtime
                        .resource
                        .iter()
                        .chain(runtime.gpio.iter())
                        .any(|resource| &resource.address == feed),
                    "{board_id}: power gate feeds {feed}, which this manifest does not declare"
                );
            }
            let Some(display) = display.as_ref() else {
                continue;
            };
            let gate_gpio: u8 = gate
                .gpio
                .strip_prefix("/gpio/")
                .and_then(|number| number.parse().ok())
                .unwrap_or_else(|| panic!("{board_id}: power gate address {}", gate.gpio));
            for (label, gpio) in display.output_wires() {
                assert_ne!(
                    gpio, gate_gpio,
                    "{board_id}: wire {label} is the power-gate pin — generation would author \
                     an endpoint onto the gate that powers the rail"
                );
            }
        }
    }
}

/// The dig2go is the board that forced the descriptor: GPIO12 is both the
/// LED-supply gate and the MTDI flash-voltage strap, so it must be reserved
/// in the runtime manifest AND named by a power gate — never one without the
/// other.
#[test]
fn the_dig2go_gates_gpio12_and_reserves_it() {
    let (_, runtime) = manifest_pairs()
        .remove("quinled/dig2go")
        .expect("the dig2go is checked in");
    let runtime = runtime.expect("the dig2go has a runtime manifest");
    let gate = match runtime.power_gate.as_slice() {
        [gate] => gate,
        gates => panic!(
            "expected exactly one dig2go power gate, got {}",
            gates.len()
        ),
    };
    assert_eq!(gate.gpio, "/gpio/12");
    assert_eq!(gate.feeds, ["/rmt/ws281x0"]);
    let pin = runtime
        .gpio
        .iter()
        .find(|resource| resource.address == "/gpio/12")
        .expect("the gate pin is a declared resource");
    assert!(
        pin.reserved_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("MTDI")),
        "the reservation must say why the pin is dangerous: {:?}",
        pin.reserved_reason
    );
}

#[test]
fn display_pins_agree_with_runtime_manifests() {
    for (board_id, (display, runtime)) in manifest_pairs() {
        let (Some(display), Some(runtime)) = (display, runtime) else {
            continue;
        };
        let label_map: BTreeMap<&str, &HardwareBoardLabelFile> = runtime
            .board_label
            .iter()
            .map(|entry| (entry.label.as_str(), entry))
            .collect();
        let gpio_addresses: BTreeMap<u32, &HardwareResourceFile> = runtime
            .gpio
            .iter()
            .filter_map(|resource| {
                resource
                    .address
                    .strip_prefix("/gpio/")
                    .and_then(|n| n.parse().ok())
                    .map(|n| (n, resource))
            })
            .collect();

        let terminals = display
            .hw
            .terminals
            .iter()
            .map(|t| (t.label.as_str(), t.role, t.gpio, Vec::new()));
        let pins = display.pins().map(|p| {
            (
                p.label.as_str(),
                p.role,
                p.gpio,
                p.caps.iter().map(|c| c.kind).collect::<Vec<_>>(),
            )
        });

        for (label, role, gpio, cap_kinds) in pins.chain(terminals) {
            // Rule 1: a runtime board_label with the same silkscreen label is
            // authoritative for the gpio mapping (or its absence).
            if let Some(entry) = label_map.get(label) {
                match entry.status {
                    Some(HardwareBoardLabelStatus::NotFound) => assert!(
                        gpio.is_none(),
                        "{board_id}: pin {label} claims gpio {gpio:?} but calibration recorded not-found"
                    ),
                    _ => {
                        if let Some(mapped) = entry
                            .gpio
                            .as_deref()
                            .and_then(|address| address.strip_prefix("/gpio/"))
                            .and_then(|n| n.parse::<u8>().ok())
                        {
                            assert_eq!(
                                gpio,
                                Some(mapped),
                                "{board_id}: pin {label} disagrees with the runtime board_label mapping"
                            );
                        }
                    }
                }
            }
            let Some(gpio) = gpio else { continue };
            match gpio_addresses.get(&u32::from(gpio)) {
                // Rule 2: a display pin whose gpio the runtime deliberately
                // omits must present as non-claimable.
                None => assert!(
                    !role.output_eligible(),
                    "{board_id}: pin {label} (gpio {gpio}) is {role:?} but the runtime manifest \
                     omits /gpio/{gpio} — either add the resource or mark the pin non-eligible"
                ),
                // Rule 3: a runtime-reserved gpio must not read as a plain io
                // pin in the catalog.
                Some(resource) if resource.reserved_reason.is_some() => assert!(
                    role != PinRole::Io || cap_kinds.contains(&lpa_boards::CapKind::Warn),
                    "{board_id}: pin {label} (gpio {gpio}) is reserved in the runtime manifest \
                     ({:?}) but displays as plain io with no warn cap",
                    resource.reserved_reason
                ),
                Some(_) => {}
            }
        }
    }
}
