//! Drift gate for the computed board↔firmware join.
//!
//! The join itself is unit-tested in `src/firmware_join.rs`; these tests
//! guard its *inputs* against the filesystem, and pin the computed result for
//! every checked-in board against a reviewed table. The table is a review
//! surface, not a source: it is asserted **against** the computation, so a
//! board or build def that changes what it runs shows up as a visible diff
//! here instead of silently changing the catalog.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use lpa_boards::{
    BUILD_DEF_SOURCES, CompatibilityBasis, NoBuildReason, all_boards, all_builds, board_by_id,
    compatible_builds_for, no_build_reason_for,
};

/// Reviewed truth: `(board_id, build ids it runs, best-fit headroom in MB)`.
/// Empty means the board runs nothing today — `expected_no_build_reason`
/// records why.
const EXPECTED_COMPATIBILITY: &[(&str, &[&str])] = &[
    ("domraem/dom-z-102", &["esp32v3-4mb"]),
    ("espressif/esp32-c6-devkitc-1", &["esp32c6-4mb"]),
    ("espressif/esp32-devkitc-v4", &["esp32v3-4mb"]),
    ("espressif/esp32-s3-devkitc-1", &["esp32s3-8mb"]),
    ("quinled/dig-uno", &["esp32v3-4mb"]),
    ("seeed/xiao-esp32-c6", &["esp32c6-4mb"]),
    ("seeed/xiao-esp32-s3-plus", &["esp32s3-8mb"]),
];

/// Why a board runs nothing. Empty since `fw-esp32v3`'s build def landed:
/// every checked-in board now computes at least one compatible build.
const EXPECTED_NO_BUILD: &[(&str, NoBuildReason)] = &[];

fn builds_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../lp-fw/builds")
}

/// Files in `lp-fw/builds/` that are not build defs. `served.json` is a
/// statement *about* build defs (which ones the site ships), so it shares
/// their directory but must never be walked as one.
const NON_BUILD_DEF_FILES: &[&str] = &["served.json"];

/// `build_id -> parsed build def` walked from `lp-fw/builds/`.
fn build_defs_on_disk() -> BTreeMap<String, serde_json::Value> {
    let mut defs = BTreeMap::new();
    for entry in fs::read_dir(builds_dir()).expect("lp-fw/builds") {
        let path = entry.expect("build def entry").path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if NON_BUILD_DEF_FILES.contains(&name.as_str()) {
            continue;
        }
        let Some(id) = name.strip_suffix(".json") else {
            continue;
        };
        let text = fs::read_to_string(&path).expect("read build def");
        let value: serde_json::Value =
            serde_json::from_str(&text).unwrap_or_else(|error| panic!("{name}: {error}"));
        defs.insert(id.to_string(), value);
    }
    defs
}

#[test]
fn embedded_build_defs_match_the_directory() {
    let on_disk: Vec<String> = build_defs_on_disk().keys().cloned().collect();
    let embedded: Vec<String> = BUILD_DEF_SOURCES
        .iter()
        .map(|(id, _)| (*id).to_string())
        .collect();
    let mut embedded_sorted = embedded.clone();
    embedded_sorted.sort();
    assert_eq!(
        embedded_sorted, on_disk,
        "lp-fw/builds/ and BUILD_DEF_SOURCES disagree — add the include_str! entry"
    );
}

#[test]
fn embedded_build_defs_match_their_files() {
    let on_disk = build_defs_on_disk();
    for build in all_builds() {
        let value = on_disk
            .get(&build.id)
            .unwrap_or_else(|| panic!("build def {} on disk", build.id));
        assert_eq!(value["package"], build.package.as_str());
        assert_eq!(value["chip"]["name"], build.chip.name.as_str());
        assert_eq!(value["flashSizeMb"], build.flash_size_mb);
    }
}

/// A served id with no build def would name a `firmware/<id>/` directory
/// nothing can produce: the picker would offer the board and the flash would
/// 404. The reverse is fine — a build def may exist unserved.
#[test]
fn every_served_build_has_a_build_def() {
    let on_disk = build_defs_on_disk();
    let served = lpa_boards::served_build_ids();
    assert!(!served.is_empty(), "served.json lists no builds");
    for id in served {
        assert!(
            on_disk.contains_key(id),
            "served.json lists {id}, which has no lp-fw/builds/{id}.json"
        );
        assert!(
            lpa_boards::build_by_id(id).is_some(),
            "served.json lists {id}, which is not in BUILD_DEF_SOURCES"
        );
        assert!(lpa_boards::is_served(id));
    }
}

/// Features are read per **package** (one `manifest-core.expected.json` per
/// firmware crate). Two build defs of the same package with different cargo
/// features would make that fixture describe neither honestly — the day that
/// happens, the fixtures must go per-build.
#[test]
fn build_defs_sharing_a_package_share_cargo_features() {
    let mut by_package: BTreeMap<String, (String, serde_json::Value)> = BTreeMap::new();
    for (id, value) in build_defs_on_disk() {
        let package = value["package"].as_str().expect("package").to_string();
        let features = value["cargoFeatures"].clone();
        if let Some((first_id, first_features)) = by_package.get(&package) {
            assert_eq!(
                first_features, &features,
                "{first_id} and {id} share package {package} but not cargoFeatures — \
                 manifest-core fixtures are per package"
            );
        } else {
            by_package.insert(package, (id, features));
        }
    }
}

#[test]
fn computed_compatibility_matches_the_reviewed_fixture() {
    let mut actual: Vec<(String, Vec<String>)> = all_boards()
        .iter()
        .map(|board| {
            let ids = compatible_builds_for(board)
                .into_iter()
                .map(|matched| matched.build.id.clone())
                .collect();
            (board.board_id.clone(), ids)
        })
        .collect();
    actual.sort();

    let expected: Vec<(String, Vec<String>)> = EXPECTED_COMPATIBILITY
        .iter()
        .map(|(board_id, builds)| {
            (
                (*board_id).to_string(),
                builds.iter().map(|id| (*id).to_string()).collect(),
            )
        })
        .collect();
    assert_eq!(
        actual, expected,
        "computed board↔firmware compatibility drifted from the reviewed table"
    );
}

#[test]
fn boards_without_a_build_say_why() {
    for (board_id, expected) in EXPECTED_NO_BUILD {
        let board = board_by_id(board_id).expect("checked-in board");
        assert_eq!(
            no_build_reason_for(board).as_ref(),
            Some(expected),
            "{board_id}"
        );
    }
    // Every board that the fixture says runs nothing must be explained.
    for (board_id, builds) in EXPECTED_COMPATIBILITY {
        if builds.is_empty() {
            assert!(
                EXPECTED_NO_BUILD.iter().any(|(id, _)| id == board_id),
                "{board_id} runs nothing and has no recorded reason"
            );
        }
    }
}

/// Compatibility must stay *computed*: no checked-in board may carry a
/// firmware pin today, and any pin added later has to name a real build whose
/// chip the board actually is (a pin relaxes flash fit, never chip identity).
#[test]
fn firmware_pins_are_absent_and_would_be_well_formed() {
    for board in all_boards() {
        for pin in board.firmware_allow.iter().chain(&board.firmware_deny) {
            let build = lpa_boards::build_by_id(&pin.build_id).unwrap_or_else(|| {
                panic!(
                    "{}: pin names unknown build {}",
                    board.board_id, pin.build_id
                )
            });
            assert_eq!(
                build.chip.name, board.family,
                "{}: pinned build {} targets another chip",
                board.board_id, pin.build_id
            );
        }
        assert!(
            board.firmware_allow.is_empty() && board.firmware_deny.is_empty(),
            "{} pins firmware by hand — remove it, or move this board out of the \
             'no exceptions checked in' expectation with the reason reviewed",
            board.board_id
        );
    }
    // ...and with no pins, every match is a computed one.
    for board in all_boards() {
        for matched in compatible_builds_for(board) {
            assert_eq!(matched.basis, CompatibilityBasis::Computed);
        }
    }
}

/// `flash_mb` is the join's input; `flash` is what the reader sees. They are
/// two statements of one fact, so they may not disagree.
#[test]
fn flash_mb_agrees_with_the_display_string() {
    for board in all_boards() {
        let Some(flash_mb) = board.flash_mb else {
            continue;
        };
        let stated = board
            .flash
            .split_whitespace()
            .next()
            .and_then(|number| number.parse::<u32>().ok())
            .unwrap_or_else(|| {
                panic!(
                    "{}: flash {:?} states no MB number but flash_mb is set",
                    board.board_id, board.flash
                )
            });
        assert!(
            board.flash.contains("MB"),
            "{}: flash_mb is in megabytes; flash reads {:?}",
            board.board_id,
            board.flash
        );
        assert_eq!(stated, flash_mb, "{}", board.board_id);
    }
}
