//! Snapshot file read/compare/update for lpfn Q32 regression tests.

use std::path::PathBuf;

/// Match `crate::util::test_helpers::float_to_fixed` (unit tests) so probes match existing q32 tests.
pub fn float_to_fixed(f: f32) -> i32 {
    const SCALE: f32 = 65536.0;
    const MAX_FLOAT: f32 = 0x7FFF_FFFF as f32 / SCALE;
    const MIN_FLOAT: f32 = i32::MIN as f32 / SCALE;

    if f > MAX_FLOAT {
        0x7FFF_FFFF
    } else if f < MIN_FLOAT {
        i32::MIN
    } else {
        (f * SCALE).round() as i32
    }
}

fn snapshot_file(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/snapshots/lpfn_q32")
        .join(format!("{name}.snap.txt"))
}

pub fn assert_snapshot(name: &str, actual: &str) {
    let path = snapshot_file(name);
    if std::env::var("LP_UPDATE_SNAPSHOTS").is_ok() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(expected, actual, "snapshot mismatch for {name}");
}

/// Call after all `assert_snapshot` calls when `LP_UPDATE_SNAPSHOTS` is set.
pub fn finish_snapshot_update_if_requested() {
    if std::env::var("LP_UPDATE_SNAPSHOTS").is_ok() {
        panic!("wrote tests/snapshots/lpfn_q32/*.snap.txt; rerun without LP_UPDATE_SNAPSHOTS");
    }
}
