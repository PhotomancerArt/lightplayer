//! Smoke test: `lp-cli profile --collect events --mode startup` end-to-end against
//! `examples/basic`. Uses startup mode so the profile gate stops after the first
//! frame completes (fast, like `profile_alloc_smoke`). Verifies `events.jsonl` and
//! the m1 `meta.json` schema.

use std::path::{Path, PathBuf};
use std::process::Command;

#[test]
fn lp_cli_profile_events_startup_smoke() {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("lp-cli crate should live one level under the workspace root");

    let examples_basic = workspace_root
        .join("examples/basic")
        .canonicalize()
        .expect("resolve examples/basic");

    let note = format!("ci-events-smoke-{}", std::process::id());
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
            "events",
            "--mode",
            "startup",
            "--note",
            note.as_str(),
        ])
        .status()
        .expect("failed to spawn cargo run");

    assert!(
        status.success(),
        "cargo run -p lp-cli profile (events) failed with {:?}",
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

    let dir_name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
    assert!(
        dir_name.contains("startup"),
        "trace dir should include mode slug 'startup': {dir_name}"
    );

    let meta_path = dir.join("meta.json");
    assert!(meta_path.exists(), "missing {}", meta_path.display());
    assert!(std::fs::metadata(&meta_path).unwrap().len() > 0);

    let meta: serde_json::Value =
        serde_json::from_reader(std::fs::File::open(&meta_path).unwrap()).unwrap();
    assert_eq!(meta["schema_version"], 1);
    assert_eq!(meta["mode"].as_str(), Some("startup"));
    assert!(
        meta["max_cycles"].as_u64().is_some(),
        "max_cycles should be a JSON number"
    );
    let cycles_used = meta["cycles_used"].as_u64().expect("cycles_used");
    assert!(cycles_used > 0, "cycles_used should be > 0");
    assert_eq!(
        meta["terminated_by"].as_str(),
        Some("profile_stop"),
        "startup mode should end via profile gate (profile_stop)"
    );
    assert!(
        meta.get("frames_requested").is_none(),
        "frames_requested should be removed in m1"
    );
    assert!(
        meta["symbols"]
            .as_array()
            .map(|a| !a.is_empty())
            .unwrap_or(false),
        "symbols should be non-empty"
    );
    assert!(
        meta["collectors"]["events"].is_object(),
        "collectors.events should be an object"
    );

    let events_path = dir.join("events.jsonl");
    assert!(events_path.exists(), "events.jsonl missing");
    let events_raw = std::fs::read_to_string(&events_path).unwrap();
    assert!(!events_raw.is_empty(), "events.jsonl is empty");

    let mut saw_frame = false;
    for line in events_raw.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let v: serde_json::Value =
            serde_json::from_str(line).unwrap_or_else(|e| panic!("bad jsonl: {line:?}: {e}"));
        assert!(
            v["cycle"].as_u64().is_some(),
            "each line should have numeric cycle: {v}"
        );
        assert!(
            v["name"].as_str().is_some(),
            "each line should have name: {v}"
        );
        assert!(
            v["kind"].as_str().is_some(),
            "each line should have kind: {v}"
        );
        if v["name"].as_str() == Some("frame") {
            saw_frame = true;
        }
    }
    assert!(saw_frame, "no 'frame' event in events.jsonl");

    let report_path = dir.join("report.txt");
    assert!(report_path.exists(), "report.txt missing");
    let report = std::fs::read_to_string(&report_path).unwrap();
    assert!(!report.is_empty());
    let first_line = report.trim_end().lines().next().unwrap_or("");
    assert!(
        first_line.starts_with("=== Perf Events ==="),
        "unexpected report banner: {:?}",
        first_line.chars().take(80).collect::<String>()
    );
}
