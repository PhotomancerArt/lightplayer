//! Slow integration tests that boot the full `fw-emu` firmware stack (via `lp-cli profile`).
//!
//! These are gated behind `#[ignore]` because they take minutes per test
//! once per-instruction CPU profiling is enabled, which made `cargo test`
//! mainline runs unbearable. Run explicitly with:
//!
//!     cargo test -p lp-cli --test profile_cpu_smoke -- --include-ignored
//!
//! See docs/roadmaps/2026-04-19-cpu-profile/m6-validation-docs.md for the
//! testing-strategy decision recorded 2026-04-19.
//!
//! Smoke tests: `lp-cli profile --collect cpu` (and variants) produce a valid trace directory.
//! The subprocess must run with the repository workspace as `current_dir` so
//! `ensure_binary_built` can locate the workspace root. Output is isolated by a
//! unique `--note` and removed after assertions.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

/// `cargo test` runs integration tests in parallel by default; three concurrent
/// `cargo run -p lp-cli profile` subprocesses contend on Cargo's global locks and
/// heavily load the emulator. Serialize profile subprocesses (m1 alloc smoke is a
/// single test so it does not need this).
static PROFILE_SUBPROCESS_LOCK: Mutex<()> = Mutex::new(());

struct Cleanup<'a> {
    path: &'a Path,
}

impl Drop for Cleanup<'_> {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(self.path);
    }
}

fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("lp-cli crate should live one level under the workspace root")
}

fn examples_basic() -> PathBuf {
    workspace_root()
        .join("examples/basic")
        .canonicalize()
        .expect("resolve examples/basic")
}

fn manifest_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")
}

/// Runs `cargo run --manifest-path lp-cli/Cargo.toml -p lp-cli -- profile <example> …mid_args --note <note>`
/// from the workspace root (same pattern as `profile_alloc_smoke`).
/// Returns the profile output directory (under `profiles/`) whose name contains `note`.
fn run_profile(note: &str, mid_args: &[&str]) -> PathBuf {
    let _guard = PROFILE_SUBPROCESS_LOCK
        .lock()
        .expect("profile subprocess lock poisoned");

    let workspace = workspace_root();
    let example = examples_basic();
    let manifest = manifest_path();

    let mut cargo_args: Vec<String> = vec![
        "run".into(),
        "--manifest-path".into(),
        manifest.to_string_lossy().into_owned(),
        "-p".into(),
        "lp-cli".into(),
        "--".into(),
        "profile".into(),
        example.to_string_lossy().into_owned(),
    ];
    for a in mid_args {
        cargo_args.push((*a).to_string());
    }
    cargo_args.push("--note".into());
    cargo_args.push(note.to_string());

    let status = Command::new("cargo")
        .current_dir(workspace)
        .args(&cargo_args)
        .status()
        .expect("failed to spawn cargo run");

    assert!(
        status.success(),
        "cargo run -p lp-cli profile failed with {:?}",
        status.code()
    );

    let profiles_dir = workspace.join("profiles");
    assert!(
        profiles_dir.is_dir(),
        "profiles/ missing under {}",
        workspace.display()
    );

    let mut profile_run_dir: Option<PathBuf> = None;
    for entry in std::fs::read_dir(&profiles_dir).expect("read profiles") {
        let entry = entry.expect("dir entry");
        let name = entry.file_name();
        if name.to_string_lossy().contains(note) {
            profile_run_dir = Some(entry.path());
            break;
        }
    }

    profile_run_dir.unwrap_or_else(|| {
        panic!(
            "expected profiles/<...>--{note}/ under {}",
            profiles_dir.display()
        )
    })
}

#[test]
#[ignore = "boots fw-emu; slow with profile feature — run explicitly with `cargo test -- --ignored` or `--include-ignored`"]
fn profile_cpu_default_smoke() {
    let note = format!("ci-cpu-default-{}", std::process::id());
    let dir = run_profile(&note, &["--collect", "cpu", "--mode", "startup"]);
    let _cleanup = Cleanup { path: &dir };

    let meta_path = dir.join("meta.json");
    assert!(meta_path.exists(), "missing {}", meta_path.display());
    let meta: serde_json::Value =
        serde_json::from_reader(std::fs::File::open(&meta_path).unwrap()).unwrap();
    assert_eq!(meta["schema_version"], 1);
    assert_eq!(meta["cycle_model"], "esp32c6");

    assert!(dir.join("events.jsonl").exists());

    let cpu_path = dir.join("cpu-profile.json");
    assert!(cpu_path.exists());
    let cpu: serde_json::Value =
        serde_json::from_reader(std::fs::File::open(&cpu_path).unwrap()).unwrap();
    assert_eq!(cpu["schema_version"], 1);
    assert_eq!(cpu["cycle_model"], "esp32c6");
    assert!(
        cpu["total_cycles_attributed"].as_u64().unwrap_or(0) > 0,
        "expected total_cycles_attributed > 0"
    );
    let func_stats = cpu["func_stats"].as_object().expect("func_stats object");
    assert!(!func_stats.is_empty(), "func_stats should be non-empty");

    let speed_path = dir.join("cpu-profile.speedscope.json");
    assert!(speed_path.exists());
    let speed: serde_json::Value =
        serde_json::from_reader(std::fs::File::open(&speed_path).unwrap()).unwrap();
    assert_eq!(
        speed["$schema"],
        "https://www.speedscope.app/file-format-schema.json"
    );
    assert_eq!(speed["profiles"][0]["type"], "evented");

    let report_path = dir.join("report.txt");
    assert!(report_path.exists());
    let report = std::fs::read_to_string(&report_path).unwrap();
    assert!(report.contains("=== CPU summary ==="));
    assert!(report.contains("cycle_model=esp32c6"));

    assert!(
        !dir.join("heap-trace.jsonl").exists(),
        "heap-trace.jsonl should not exist without alloc collector"
    );
}

#[test]
#[ignore = "boots fw-emu; slow with profile feature — run explicitly with `cargo test -- --ignored` or `--include-ignored`"]
fn profile_cpu_uniform_model() {
    let note = format!("ci-cpu-uniform-{}", std::process::id());
    let dir = run_profile(
        &note,
        &[
            "--collect",
            "cpu",
            "--mode",
            "startup",
            "--cycle-model",
            "uniform",
        ],
    );
    let _cleanup = Cleanup { path: &dir };

    let meta: serde_json::Value =
        serde_json::from_reader(std::fs::File::open(dir.join("meta.json")).unwrap()).unwrap();
    assert_eq!(meta["cycle_model"], "uniform");

    let cpu: serde_json::Value =
        serde_json::from_reader(std::fs::File::open(dir.join("cpu-profile.json")).unwrap())
            .unwrap();
    assert_eq!(cpu["cycle_model"], "uniform");
}

#[test]
#[ignore = "boots fw-emu; slow with profile feature — run explicitly with `cargo test -- --ignored` or `--include-ignored`"]
fn profile_cpu_with_alloc() {
    let note = format!("ci-cpu-with-alloc-{}", std::process::id());
    let dir = run_profile(&note, &["--collect", "cpu,alloc", "--mode", "startup"]);
    let _cleanup = Cleanup { path: &dir };

    assert!(dir.join("cpu-profile.json").exists());
    assert!(dir.join("heap-trace.jsonl").exists());
    assert!(dir.join("events.jsonl").exists());
}
