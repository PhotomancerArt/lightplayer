//! `lp-cli hardware bench` — resolve what is being measured, run the ramp,
//! write the record.
//!
//! The command owns the *identity* half of a measurement: which board, which
//! firmware build, which port, and the provenance stamped onto the record.
//! [`super::run`] owns the hardware half and [`super::schedule`] the
//! arithmetic.

use std::path::Path;

use anyhow::{Context, Result, bail};
use lpc_hardware::HardwareManifestFile;
use lpc_model::{MEASUREMENT_DEFAULT_MARGIN, MeasurementRecord};

use crate::commands::firmware::build_def::{
    BuildDef, find_repo_root, load_build_def, load_build_defs,
};
use crate::commands::fwcheck::port::resolve_esp32_port;
use crate::commands::hardware::args::BenchArgs;
use crate::commands::hardware::manifest::board_manifest_store::BoardManifestStore;

use super::measurement_store::{store_root, write_record};
use super::run::{BenchOutcome, BenchPlan, BenchStepReport, run_bench};
use super::schedule::{StepOutcome, default_start_leds};
use super::workload::workload_endpoint;

/// Version of the bench harness. Bumped when the procedure changes in a way
/// that could move the boundary without the metric definition changing (a
/// different settle window, a different death criterion, a different way of
/// deploying) — a record carries it so two numbers measured by two harnesses
/// are never silently compared.
pub const BENCH_HARNESS_VERSION: u32 = 1;

pub fn handle_bench(args: BenchArgs) -> Result<()> {
    let repo_root = match &args.repo {
        Some(repo) => repo.clone(),
        None => find_repo_root()?,
    };
    let board = BoardManifestStore::discover(Some(repo_root.clone()), args.boards_dir.clone())?
        .load(&args.board)
        .with_context(|| format!("loading board manifest `{}`", args.board))?;
    let build = resolve_build(&repo_root, &board, args.build.as_deref())?;
    let endpoint = workload_endpoint(&board)?;
    let port = resolve_esp32_port(args.port.as_deref(), Some(&build.chip.name))?;
    let start_leds = args
        .start
        .unwrap_or_else(|| default_start_leds(&build.chip.name));

    println!("bench:  {} × {}", build.id, board.id);
    println!("port:   {port}");
    println!("output: {}", endpoint.as_str());
    println!("start:  {start_leds} LEDs");
    println!();

    let expected_commit = packaged_commit(&repo_root, &build.id);
    if expected_commit.is_none() {
        eprintln!(
            "note:   no packaged manifest for `{}` — the image on the board cannot be \
             verified against a build (run `lp-cli firmware package {}` to enable that check)",
            build.id, build.id
        );
    }

    let outcome = run_bench(BenchPlan {
        port,
        expected_package: build.package.clone(),
        expected_commit,
        endpoint,
        start_leds,
        verbose: args.verbose,
    })?;

    let record = MeasurementRecord {
        notes: notes(&outcome),
        ..MeasurementRecord::new_leds_max_safe(
            &build.id,
            &board.id,
            outcome.boundary_leds,
            MEASUREMENT_DEFAULT_MARGIN,
            today(),
            &outcome.fw_commit,
            outcome.fw_dirty,
            BENCH_HARNESS_VERSION,
        )
    };

    let store = match &args.out {
        Some(out) => out.clone(),
        None => store_root(&repo_root),
    };
    let path = write_record(&store, &record)?;

    print_summary(&outcome, &record, &path);
    Ok(())
}

/// Which firmware build this board is being measured against.
///
/// `--build` is explicit. Otherwise the board's target picks it: a build def's
/// `chip.name` and a board manifest's `target` are the same vocabulary
/// (`esp32`, `esp32c6`, `esp32s3`), so the join is an equality, and an
/// ambiguous one is an error rather than a guess — the record names a build.
fn resolve_build(
    repo_root: &Path,
    board: &HardwareManifestFile,
    explicit: Option<&str>,
) -> Result<BuildDef> {
    if let Some(id) = explicit {
        let build = load_build_def(repo_root, id)?;
        if build.chip.name != board.target.as_str() {
            bail!(
                "build `{}` targets {} but board `{}` is {} — that pair cannot boot",
                build.id,
                build.chip.name,
                board.id,
                board.target
            );
        }
        return Ok(build);
    }

    let target = board.target.as_str();
    let mut matching: Vec<BuildDef> = load_build_defs(repo_root)?
        .into_iter()
        .filter(|build| build.chip.name == target)
        .collect();

    match matching.len() {
        1 => Ok(matching.remove(0)),
        0 => bail!(
            "no firmware build targets `{target}` (board `{}`); \
             see lp-fw/builds/",
            board.id
        ),
        _ => bail!(
            "several firmware builds target `{target}` — pass --build <id> (one of: {})",
            matching
                .iter()
                .map(|build| build.id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// Local date, as the record's `measuredOn`. A run happens on a desk, so the
/// desk's date is the honest one.
fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

/// Anything the run learned that the numbered fields cannot hold — today, the
/// OOM byte counts the firmware prints but the wire drops.
/// Commit recorded in the last `firmware package` output for this build, if
/// any. Read straight out of the distribution manifest's extracted `core`,
/// so it is the commit of the bytes that were actually written.
fn packaged_commit(repo_root: &Path, build_id: &str) -> Option<String> {
    let path = repo_root
        .join("target/studio-web-assets/firmware")
        .join(build_id)
        .join("manifest.json");
    let raw = std::fs::read_to_string(path).ok()?;
    let manifest: serde_json::Value = serde_json::from_str(&raw).ok()?;
    manifest
        .get("core")?
        .get("commit")?
        .as_str()
        .map(str::to_owned)
}

fn notes(outcome: &BenchOutcome) -> Option<String> {
    if outcome.oom_stats.is_empty() {
        return None;
    }
    Some(format!("oom stats: {}", outcome.oom_stats.join(" | ")))
}

fn print_summary(outcome: &BenchOutcome, record: &MeasurementRecord, path: &Path) {
    println!();
    println!(
        "{:>7}  {:>9}  {:>10}  {:>10}  {}",
        "LEDs", "result", "free heap", "used heap", "frames"
    );
    for step in &outcome.steps {
        println!(
            "{:>7}  {:>9}  {:>10}  {:>10}  {}",
            step.leds,
            match step.outcome {
                StepOutcome::Survived => "survived",
                StepOutcome::Died => "oom",
            },
            free_heap_cell(step),
            used_heap_cell(step),
            step.frames
        );
    }
    if let Some(fit) = per_led_fit(&outcome.steps) {
        println!();
        println!("{fit}");
    }
    println!();
    println!(
        "boundary {} LEDs → limit {} (margin {})",
        record.raw_boundary_leds, record.limit_leds, record.margin
    );
    println!(
        "firmware {}{}, metric {}@{}, harness {}",
        record.fw_commit,
        if record.fw_dirty { "-dirty" } else { "" },
        record.metric,
        record.metric_version,
        record.harness_version
    );
    if let Some(notes) = &record.notes {
        println!("{notes}");
    }
    println!("record   {}", path.display());
}

/// Least-squares fit of `used` against LED count over the surviving steps.
/// The slope is the marginal cost of one LED; the intercept is everything
/// that does not scale with LEDs (idle + project residency + whatever the
/// compile left behind). Two points is enough to be useful, so the bar is
/// low — this is a reading aid printed beside the data it came from, not a
/// checked-in claim.
fn per_led_fit(steps: &[BenchStepReport]) -> Option<String> {
    let points: Vec<(f64, f64)> = steps
        .iter()
        .filter_map(|step| Some((f64::from(step.leds), f64::from(step.max_used_bytes?))))
        .collect();
    if points.len() < 2 {
        return None;
    }
    let n = points.len() as f64;
    let mean_x = points.iter().map(|(x, _)| x).sum::<f64>() / n;
    let mean_y = points.iter().map(|(_, y)| y).sum::<f64>() / n;
    let covariance: f64 = points
        .iter()
        .map(|(x, y)| (x - mean_x) * (y - mean_y))
        .sum();
    let variance: f64 = points.iter().map(|(x, _)| (x - mean_x).powi(2)).sum();
    if variance == 0.0 {
        return None;
    }
    let slope = covariance / variance;
    let intercept = mean_y - slope * mean_x;
    Some(format!(
        "used ≈ {:.1} B/LED × leds + {:.0} B fixed  ({} points)",
        slope,
        intercept,
        points.len()
    ))
}

fn used_heap_cell(step: &BenchStepReport) -> String {
    match step.max_used_bytes {
        Some(bytes) => format!("{}k", bytes / 1024),
        None => "-".to_string(),
    }
}

fn free_heap_cell(step: &BenchStepReport) -> String {
    match step.min_free_bytes {
        Some(bytes) => format!("{}k", bytes / 1024),
        None => "-".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use lpc_hardware::HardwareTarget;

    use super::*;

    fn board(id: &str, target: HardwareTarget) -> HardwareManifestFile {
        HardwareManifestFile::new(id, target, "vendor", "Product")
    }

    /// A board picks its build by chip, so the common case needs no --build.
    #[test]
    fn the_boards_target_picks_the_build() {
        let repo_root = find_repo_root().unwrap();
        for (board_id, target, expected) in [
            (
                "seeed/xiao-esp32-c6",
                HardwareTarget::Esp32c6,
                "esp32c6-4mb",
            ),
            (
                "seeed/xiao-esp32-s3-plus",
                HardwareTarget::Esp32s3,
                "esp32s3-8mb",
            ),
            ("domraem/dom-z-102", HardwareTarget::Esp32, "esp32v3-4mb"),
        ] {
            let build = resolve_build(&repo_root, &board(board_id, target), None).unwrap();
            assert_eq!(build.id, expected);
        }
    }

    /// A target with no firmware is an error naming the board, not a panic or
    /// a silent fallback to some other chip's build.
    #[test]
    fn a_target_with_no_firmware_is_refused() {
        let repo_root = find_repo_root().unwrap();
        let error = resolve_build(
            &repo_root,
            &board("test/emu", HardwareTarget::Rv32imacEmu),
            None,
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("rv32imac_emu"), "{error}");
    }

    /// An explicit --build still has to be able to boot on the board.
    #[test]
    fn an_explicit_build_for_the_wrong_chip_is_refused() {
        let repo_root = find_repo_root().unwrap();
        let error = resolve_build(
            &repo_root,
            &board("seeed/xiao-esp32-c6", HardwareTarget::Esp32c6),
            Some("esp32s3-8mb"),
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("cannot boot"), "{error}");

        assert!(
            resolve_build(
                &repo_root,
                &board("seeed/xiao-esp32-c6", HardwareTarget::Esp32c6),
                Some("esp32c6-4mb"),
            )
            .is_ok()
        );
    }

    #[test]
    fn the_measured_on_date_is_an_iso_day() {
        let today = today();
        assert_eq!(today.len(), 10, "{today}");
        assert_eq!(today.matches('-').count(), 2, "{today}");
    }
}
