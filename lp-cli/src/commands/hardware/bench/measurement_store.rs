//! The checked-in measurement store: `measurements/<build-id>/<board-id>.json`.
//!
//! Records are machine-written by `lp-cli hardware bench` and read by tooling
//! and (via `include_str!`) the studio catalog. This module resolves paths,
//! parses records, and refuses ones whose stored facts disagree with each
//! other or with where they are filed — the drift guard that keeps a
//! hand-edited record from passing as a measurement. [`write_record`] is the
//! single write path, and the bench command is its only caller.
//!
//! See `measurements/README.md` for the record fields and the staleness
//! posture.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use lpc_model::MeasurementRecord;

/// Store root, relative to the repo root.
pub const MEASUREMENTS_DIR: &str = "measurements";

/// A record plus where it was loaded from.
#[derive(Debug, Clone)]
pub struct StoredMeasurement {
    pub record: MeasurementRecord,
    pub path: PathBuf,
}

/// The store directory inside a repo checkout.
pub fn store_root(repo_root: &Path) -> PathBuf {
    repo_root.join(MEASUREMENTS_DIR)
}

/// Path of the record for one (build × board) pair. The board id's `/` becomes
/// `-` so each build's records are one flat directory.
pub fn record_path(repo_root: &Path, build_id: &str, board_id: &str) -> PathBuf {
    record_path_in(&store_root(repo_root), build_id, board_id)
}

/// [`record_path`] against an explicit store root — the seam the bench's
/// `--out` writes through.
pub fn record_path_in(store_root: &Path, build_id: &str, board_id: &str) -> PathBuf {
    store_root
        .join(build_id)
        .join(format!("{}.json", board_file_stem(board_id)))
}

/// Write `record` into the store at `store_root`, returning where it landed.
///
/// The only write path in the repo: records are produced by a bench run, not
/// typed. The derivation is re-checked here so a caller that built a record by
/// hand cannot file one the loader would later refuse.
pub fn write_record(store_root: &Path, record: &MeasurementRecord) -> Result<PathBuf> {
    if !record.limit_is_derived() {
        bail!(
            "refusing to write a record whose limitLeds ({}) is not floor({} * {})",
            record.limit_leds,
            record.raw_boundary_leds,
            record.margin
        );
    }

    let path = record_path_in(store_root, &record.build_id, &record.board_id);
    let dir = path
        .parent()
        .with_context(|| format!("{} has no parent directory", path.display()))?;
    std::fs::create_dir_all(dir)
        .with_context(|| format!("creating measurement directory {}", dir.display()))?;

    let mut json =
        serde_json::to_string_pretty(record).context("serializing measurement record")?;
    json.push('\n');
    std::fs::write(&path, json).with_context(|| format!("writing {}", path.display()))?;
    Ok(path)
}

/// The file stem a board id is stored under (`seeed/xiao-esp32-c6` →
/// `seeed-xiao-esp32-c6`).
pub fn board_file_stem(board_id: &str) -> String {
    board_id.replace('/', "-")
}

/// Parse a record, checking the invariants the store depends on.
pub fn parse_record(json: &str) -> Result<MeasurementRecord> {
    let record: MeasurementRecord =
        serde_json::from_str(json).context("measurement record is not valid JSON")?;
    if !record.limit_is_derived() {
        bail!(
            "measurement record for {} × {} claims limitLeds {} but \
             floor({} * {}) is {} — records are machine-written; rerun the bench \
             rather than editing one",
            record.build_id,
            record.board_id,
            record.limit_leds,
            record.raw_boundary_leds,
            record.margin,
            lpc_model::measurement::derive_limit_leds(record.raw_boundary_leds, record.margin)
        );
    }
    Ok(record)
}

/// Load the record for one pair, or `None` when the pair has never been
/// measured. A missing record is silence, not an error.
pub fn load_record(
    repo_root: &Path,
    build_id: &str,
    board_id: &str,
) -> Result<Option<StoredMeasurement>> {
    let path = record_path(repo_root, build_id, board_id);
    if !path.exists() {
        return Ok(None);
    }
    load_path(&path).map(Some)
}

/// Load every record in the store, sorted by (build id, board id). An absent
/// store directory is an empty store.
pub fn load_records(repo_root: &Path) -> Result<Vec<StoredMeasurement>> {
    let root = repo_root.join(MEASUREMENTS_DIR);
    if !root.exists() {
        return Ok(Vec::new());
    }

    let mut paths = Vec::new();
    for build_entry in
        std::fs::read_dir(&root).with_context(|| format!("reading {}", root.display()))?
    {
        let build_entry = build_entry?;
        if !build_entry.file_type()?.is_dir() {
            continue;
        }
        for record_entry in std::fs::read_dir(build_entry.path())? {
            let path = record_entry?.path();
            if path.extension().is_some_and(|ext| ext == "json") {
                paths.push(path);
            }
        }
    }
    paths.sort();

    paths.iter().map(|path| load_path(path)).collect()
}

/// Read and validate one record file, including that its path agrees with the
/// ids it carries.
fn load_path(path: &Path) -> Result<StoredMeasurement> {
    let json =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let record = parse_record(&json).with_context(|| format!("parsing {}", path.display()))?;

    let build_dir = path
        .parent()
        .and_then(|parent| parent.file_name())
        .unwrap_or_default()
        .to_string_lossy();
    if record.build_id != build_dir {
        bail!(
            "{} declares buildId `{}` but sits under `{}` — the directory is the build id",
            path.display(),
            record.build_id,
            build_dir
        );
    }

    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    if board_file_stem(&record.board_id) != stem {
        bail!(
            "{} declares boardId `{}`, which files as `{}.json`",
            path.display(),
            record.board_id,
            board_file_stem(&record.board_id)
        );
    }

    Ok(StoredMeasurement {
        record,
        path: path.to_path_buf(),
    })
}

#[cfg(test)]
mod tests {
    use lpc_model::MEASUREMENT_DEFAULT_MARGIN;
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn board_ids_file_as_one_flat_name_per_build() {
        assert_eq!(
            board_file_stem("seeed/xiao-esp32-c6"),
            "seeed-xiao-esp32-c6"
        );
        assert_eq!(
            record_path(Path::new("/repo"), "esp32c6-4mb", "seeed/xiao-esp32-c6"),
            Path::new("/repo/measurements/esp32c6-4mb/seeed-xiao-esp32-c6.json")
        );
    }

    /// A record written by the bench loads back with its ids and derivation
    /// intact — the round trip the store exists for.
    #[test]
    fn loads_a_written_record() {
        let repo = TempDir::new().unwrap();
        let record = store_a_record(repo.path(), "esp32c6-4mb", "seeed/xiao-esp32-c6", 250);

        let loaded = load_record(repo.path(), "esp32c6-4mb", "seeed/xiao-esp32-c6")
            .unwrap()
            .expect("record exists");
        assert_eq!(loaded.record, record);
        assert_eq!(loaded.record.limit_leds, 200);

        let all = load_records(repo.path()).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].record.board_id, "seeed/xiao-esp32-c6");
    }

    /// A pair that was never measured is silence, and so is an absent store —
    /// P4 writes the first records, so an empty tree must be a normal state.
    #[test]
    fn missing_records_and_missing_store_are_empty_not_errors() {
        let repo = TempDir::new().unwrap();
        assert!(load_records(repo.path()).unwrap().is_empty());
        assert!(
            load_record(repo.path(), "esp32c6-4mb", "seeed/xiao-esp32-c6")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn rejects_a_hand_edited_limit() {
        let repo = TempDir::new().unwrap();
        store_a_record(repo.path(), "esp32c6-4mb", "seeed/xiao-esp32-c6", 250);
        let path = record_path(repo.path(), "esp32c6-4mb", "seeed/xiao-esp32-c6");
        let edited = std::fs::read_to_string(&path)
            .unwrap()
            .replace("\"limitLeds\": 200", "\"limitLeds\": 250");
        std::fs::write(&path, edited).unwrap();

        let error = load_records(repo.path()).unwrap_err().to_string();
        assert!(error.contains("parsing"), "{error}");
    }

    #[test]
    fn rejects_a_record_filed_under_the_wrong_pair() {
        let repo = TempDir::new().unwrap();
        store_a_record(repo.path(), "esp32c6-4mb", "seeed/xiao-esp32-c6", 250);
        let path = record_path(repo.path(), "esp32c6-4mb", "seeed/xiao-esp32-c6");
        let moved = path.with_file_name("domraem-dom-z-102.json");
        std::fs::rename(&path, &moved).unwrap();

        let error = load_records(repo.path()).unwrap_err().to_string();
        assert!(error.contains("boardId"), "{error}");
    }

    /// The write side files a record where the read side looks for it, and a
    /// rerun of a pair replaces its record rather than accumulating.
    #[test]
    fn writing_a_record_files_it_where_the_loader_looks() {
        let repo = TempDir::new().unwrap();
        store_a_record(repo.path(), "esp32s3-8mb", "seeed/xiao-esp32-s3-plus", 400);
        assert!(record_path(repo.path(), "esp32s3-8mb", "seeed/xiao-esp32-s3-plus").exists());

        store_a_record(repo.path(), "esp32s3-8mb", "seeed/xiao-esp32-s3-plus", 410);
        let all = load_records(repo.path()).unwrap();
        assert_eq!(all.len(), 1, "a rerun replaces the pair's record");
        assert_eq!(all[0].record.raw_boundary_leds, 410);
    }

    /// A record whose limit was tampered with never reaches the store.
    #[test]
    fn refuses_to_write_an_underived_limit() {
        let repo = TempDir::new().unwrap();
        let mut record = a_record("esp32c6-4mb", "seeed/xiao-esp32-c6", 250);
        record.limit_leds = 250;

        let error = write_record(&store_root(repo.path()), &record)
            .unwrap_err()
            .to_string();
        assert!(error.contains("limitLeds"), "{error}");
    }

    /// A record stored the way the bench stores it — through the real write
    /// path, so every read-side test is reading real output.
    fn store_a_record(
        repo_root: &Path,
        build_id: &str,
        board_id: &str,
        raw_boundary_leds: u32,
    ) -> MeasurementRecord {
        let record = a_record(build_id, board_id, raw_boundary_leds);
        write_record(&store_root(repo_root), &record).unwrap();
        record
    }

    fn a_record(build_id: &str, board_id: &str, raw_boundary_leds: u32) -> MeasurementRecord {
        MeasurementRecord::new_leds_max_safe(
            build_id,
            board_id,
            raw_boundary_leds,
            MEASUREMENT_DEFAULT_MARGIN,
            "2026-08-02",
            "5466346",
            false,
            1,
        )
    }
}
