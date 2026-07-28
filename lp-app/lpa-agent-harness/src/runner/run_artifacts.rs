//! Per-run artifact directory: layout, timestamp naming, and writes.

use std::path::{Path, PathBuf};

/// The four artifacts one run leaves behind.
pub const DEBUG_JSON: &str = "debug.json";
pub const FINAL_GLSL: &str = "final.glsl";
pub const RUN_MD: &str = "run.md";
pub const TRIAGE_TXT: &str = "triage.txt";

/// Resolve the artifact directory: `--out`, or
/// `target/agent-runs/<timestamp>` (workspace target dir — this crate sits
/// at `lp-app/<crate>` depth, same convention as the eval report).
pub fn resolve_out_dir(out: Option<PathBuf>, now_epoch_secs: u64) -> PathBuf {
    out.unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/agent-runs")
            .join(timestamp_dir_name(now_epoch_secs))
    })
}

/// `YYYYMMDD-HHMMSS` (UTC) for run-dir names: sortable, no separators that
/// bother shells. Civil-date math from days-since-epoch (Howard Hinnant's
/// algorithm), so the runner needs no chrono dependency.
pub fn timestamp_dir_name(epoch_secs: u64) -> String {
    let days = epoch_secs / 86_400;
    let secs = epoch_secs % 86_400;
    let (year, month, day) = civil_from_days(days as i64);
    format!(
        "{year:04}{month:02}{day:02}-{:02}{:02}{:02}",
        secs / 3600,
        (secs % 3600) / 60,
        secs % 60
    )
}

/// Gregorian (year, month, day) from days since 1970-01-01.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

/// Write one artifact into the run dir (created on first write).
pub fn write_artifact(dir: &Path, name: &str, content: &str) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir)
        .map_err(|error| format!("cannot create {}: {error}", dir.display()))?;
    let path = dir.join(name);
    std::fs::write(&path, content)
        .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamps_are_utc_civil_dates() {
        assert_eq!(timestamp_dir_name(0), "19700101-000000");
        // 2026-07-27 15:30:45 UTC.
        assert_eq!(timestamp_dir_name(1_785_166_245), "20260727-153045");
        // Leap-day sanity: 2024-02-29 00:00:00 UTC.
        assert_eq!(timestamp_dir_name(1_709_164_800), "20240229-000000");
    }

    #[test]
    fn out_flag_wins_over_the_default() {
        let dir = resolve_out_dir(Some(PathBuf::from("/tmp/run-here")), 0);
        assert_eq!(dir, PathBuf::from("/tmp/run-here"));
        let default = resolve_out_dir(None, 0);
        assert!(
            default.ends_with("target/agent-runs/19700101-000000"),
            "{default:?}"
        );
    }
}
