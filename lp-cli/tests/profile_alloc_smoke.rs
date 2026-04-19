//! Smoke test: `lp-cli profile --collect alloc` produces a valid trace directory.
//!
//! The subprocess must run with the repository workspace as `current_dir` so
//! `ensure_binary_built` can locate the workspace root. Output is isolated by a
//! unique `--note` and removed after assertions.

use std::path::{Path, PathBuf};
use std::process::Command;

#[test]
fn profile_alloc_smoke() {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("lp-cli crate should live one level under the workspace root");

    let examples_basic = workspace_root
        .join("examples/basic")
        .canonicalize()
        .expect("resolve examples/basic");

    let note = format!("ci-smoke-{}", std::process::id());
    let manifest_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");

    let status = Command::new("cargo")
        .current_dir(workspace_root)
        .args([
            "run",
            "--manifest-path",
            manifest_path.to_str().expect("utf8 manifest"),
            "-p",
            "lp-cli",
            "--",
            "profile",
            examples_basic.to_str().expect("utf8 examples"),
            "--collect",
            "alloc",
            "--frames",
            "2",
            "--note",
            note.as_str(),
        ])
        .status()
        .expect("failed to spawn cargo run");

    assert!(
        status.success(),
        "cargo run -p lp-cli profile failed with {:?}",
        status.code()
    );

    let profiles_dir = workspace_root.join("profiles");
    assert!(
        profiles_dir.is_dir(),
        "profiles/ missing under {}",
        workspace_root.display()
    );

    let mut profile_run_dir: Option<PathBuf> = None;
    for entry in std::fs::read_dir(&profiles_dir).expect("read profiles") {
        let entry = entry.expect("dir entry");
        let name = entry.file_name();
        let s = name.to_string_lossy();
        if s.contains(&note) {
            profile_run_dir = Some(entry.path());
            break;
        }
    }
    let dir = profile_run_dir.unwrap_or_else(|| {
        panic!(
            "expected profiles/<...>--{note}/ under {}",
            profiles_dir.display()
        )
    });

    struct Cleanup<'a> {
        path: &'a Path,
    }
    impl Drop for Cleanup<'_> {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(self.path);
        }
    }
    let _cleanup = Cleanup { path: &dir };

    let meta_path = dir.join("meta.json");
    assert!(meta_path.exists(), "missing {}", meta_path.display());
    assert!(std::fs::metadata(&meta_path).unwrap().len() > 0);

    let meta: serde_json::Value =
        serde_json::from_reader(std::fs::File::open(&meta_path).unwrap()).unwrap();
    assert_eq!(meta["schema_version"], 1);
    assert!(
        meta["symbols"]
            .as_array()
            .map(|a| !a.is_empty())
            .unwrap_or(false),
        "symbols should be non-empty"
    );
    assert!(
        meta["collectors"]["alloc"].is_object(),
        "collectors.alloc should be an object"
    );

    let heap_trace = dir.join("heap-trace.jsonl");
    assert!(heap_trace.exists());
    let trace = std::fs::read_to_string(&heap_trace).unwrap();
    assert!(!trace.is_empty());

    let mut saw_shape = false;
    for line in trace.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let event: serde_json::Value = serde_json::from_str(line).expect("jsonl line");
        if event.get("t").is_some()
            && event.get("ptr").is_some()
            && event.get("sz").is_some()
            && event.get("ic").is_some()
            && event.get("frames").is_some()
            && event.get("free").is_some()
        {
            saw_shape = true;
            break;
        }
    }
    assert!(
        saw_shape,
        "expected at least one alloc event with t/ptr/sz/ic/frames/free"
    );

    let report_path = dir.join("report.txt");
    assert!(report_path.exists());
    let report = std::fs::read_to_string(&report_path).unwrap();
    assert!(!report.is_empty());
    let first_line = report.trim_end().lines().next().unwrap_or("");
    assert!(
        first_line.starts_with("=== Heap Allocation ==="),
        "unexpected report banner: {:?}",
        first_line.chars().take(80).collect::<String>()
    );
}
