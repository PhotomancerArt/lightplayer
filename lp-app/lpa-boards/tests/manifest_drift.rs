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
