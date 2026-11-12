# Phase 8 — End-to-end tests + validation

Final phase: prove the whole pipeline works under `cargo test` and
update the existing m0 integration test to match the new
`SessionMetadata` shape.

This phase depends on every preceding phase.

## Subagent assignment

`generalPurpose` subagent. Two test files (one new, one update) +
final cross-cutting validation.

## Files to create / update

```
lp-cli/tests/
└── profile_events_steady_render_smoke.rs    # NEW

lp-fw/fw-tests/tests/
└── profile_alloc_emu.rs                     # UPDATE: new meta fields,
                                             #         + assert events.jsonl
                                             #         when --collect events
```

## Contents

### `lp-cli/tests/profile_events_steady_render_smoke.rs`

End-to-end test: invokes the lp-cli binary as a subprocess, runs
profile against `examples/basic`, and asserts:

- A `profiles/<ts>--examples-basic--steady-render/` directory was created.
- `meta.json` exists, parses as JSON, contains expected new fields:
  - `mode == "steady-render"`
  - `max_cycles` is a number
  - `cycles_used > 0`
  - `terminated_by` ∈ {"profile_stop", "guest_halt", "max_cycles"}
- `events.jsonl` exists and is non-empty.
- At least one line parses as `{"cycle": <u64>, "name": <str>, "kind": <str>}`.
- At least one event with `name == "frame"` is present.
- `report.txt` exists.

```rust
//! Smoke test: `lp-cli profile --collect events --mode steady-render`
//! end-to-end against examples/basic. Verifies that the CLI produces
//! a populated profile directory with events.jsonl and the m1 metadata
//! schema.

use std::path::PathBuf;
use std::process::Command;

#[test]
fn lp_cli_profile_events_steady_render_smoke() {
    // Resolve workspace root (this test runs from lp-cli/).
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest.parent().unwrap();

    // Build lp-cli first (release-emu off — debug is fine for smoke).
    let status = Command::new(env!("CARGO"))
        .args(["build", "-p", "lp-cli"])
        .current_dir(workspace)
        .status()
        .expect("cargo build lp-cli");
    assert!(status.success(), "cargo build lp-cli failed");

    // Use a temp dir for the working directory so we don't pollute
    // the repo's profiles/ folder.
    let tmp = tempfile::tempdir().unwrap();

    let lp_cli_path = workspace
        .join("target/debug/lp-cli");

    let output = Command::new(&lp_cli_path)
        .args([
            "profile",
            // Use absolute path to examples/basic in the repo.
            workspace.join("examples/basic").to_str().unwrap(),
            "--collect", "events",
            "--mode", "steady-render",
            // Tighten max-cycles so the test runs in <30s.
            "--max-cycles", "200000000",
        ])
        .current_dir(tmp.path())
        .output()
        .expect("run lp-cli profile");

    assert!(
        output.status.success(),
        "lp-cli profile failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    // Last line of stdout is the trace dir path.
    let stdout = String::from_utf8(output.stdout).unwrap();
    let trace_dir_str = stdout.lines().last().expect("no stdout line");
    let trace_dir = PathBuf::from(trace_dir_str);
    assert!(trace_dir.exists(), "trace dir missing: {trace_dir:?}");

    // meta.json checks.
    let meta_path = trace_dir.join("meta.json");
    let meta_raw = std::fs::read_to_string(&meta_path).unwrap();
    let meta: serde_json::Value = serde_json::from_str(&meta_raw).unwrap();
    assert_eq!(meta["mode"].as_str(), Some("steady-render"));
    assert!(meta["max_cycles"].as_u64().is_some());
    let cycles_used = meta["cycles_used"].as_u64().expect("cycles_used");
    assert!(cycles_used > 0, "cycles_used should be > 0");
    let terminated_by = meta["terminated_by"].as_str().unwrap();
    assert!(
        ["profile_stop", "guest_halt", "max_cycles"].contains(&terminated_by),
        "unexpected terminated_by: {terminated_by}",
    );
    // Old field must be gone.
    assert!(meta.get("frames_requested").is_none(),
        "frames_requested should be removed in m1");

    // events.jsonl checks.
    let events_path = trace_dir.join("events.jsonl");
    let events_raw = std::fs::read_to_string(&events_path).unwrap();
    assert!(!events_raw.is_empty(), "events.jsonl is empty");
    let mut saw_frame = false;
    for line in events_raw.lines() {
        let v: serde_json::Value = serde_json::from_str(line)
            .unwrap_or_else(|e| panic!("bad jsonl: {line:?}: {e}"));
        assert!(v["cycle"].as_u64().is_some());
        assert!(v["name"].as_str().is_some());
        assert!(v["kind"].as_str().is_some());
        if v["name"].as_str() == Some("frame") {
            saw_frame = true;
        }
    }
    assert!(saw_frame, "no 'frame' event in events.jsonl");

    assert!(trace_dir.join("report.txt").exists(), "report.txt missing");
}
```

If `tempfile` isn't a dev-dep of `lp-cli`, add it to
`lp-cli/Cargo.toml [dev-dependencies]`.

This test is slow (it builds fw-emu inside lp-cli's profile run).
Mark it `#[ignore]` if the workspace convention is to keep
`cargo test` fast — m0's `profile_alloc_emu` precedent decides.
Check that file:

```bash
head -30 lp-fw/fw-tests/tests/profile_alloc_emu.rs
```

If `profile_alloc_emu` is `#[ignore]`, mirror it. Otherwise leave
it as a normal test.

### `lp-fw/fw-tests/tests/profile_alloc_emu.rs` updates

Two changes:

1. **Metadata field assertions**: Replace any check on
   `meta["frames_requested"]` with checks on the new fields:
   ```rust
   assert!(meta["mode"].as_str().is_some());
   assert!(meta["max_cycles"].as_u64().is_some());
   assert!(meta["cycles_used"].as_u64().is_some());
   assert!(meta["terminated_by"].as_str().is_some());
   assert!(meta.get("frames_requested").is_none());
   ```
2. **CLI invocation**: Replace `--frames N` with
   `--mode all --max-cycles <X>` (use `all` mode so the test isn't
   accidentally truncated by the steady-render gate):
   ```rust
   .args([
       "profile",
       project_path,
       "--collect", "alloc",
       "--mode", "all",
       "--max-cycles", "200000000",
   ])
   ```

Read the existing test first to find the exact spots to patch:

```bash
# (use Read tool, not cat)
```

The existing assertions on `events_recorded > 0` from the alloc
collector should still pass because alloc tracing is unchanged.

## Final cross-cutting validation

After this phase merges, run:

```bash
# Workspace-wide build (catches any cfg / feature breakage)
cargo build --workspace
cargo build --workspace --all-features        # esp32 will likely
                                               # require its target
                                               # — skip if not set up

# All tests
cargo test --workspace

# fw-emu-specific
cargo test -p fw-tests
cargo test -p lp-cli
cargo test -p lp-riscv-emu
cargo test -p lp-perf
cargo test -p lp-perf --features log

# Smoke run by hand:
cargo run -p lp-cli -- profile --collect events --mode steady-render
ls -la profiles/$(ls profiles/ | tail -1)
cat profiles/$(ls profiles/ | tail -1)/meta.json
head -5 profiles/$(ls profiles/ | tail -1)/events.jsonl
cat profiles/$(ls profiles/ | tail -1)/report.txt

# Dual-collector smoke:
cargo run -p lp-cli -- profile --collect alloc,events --mode steady-render
```

## Acceptance checklist (cross-checked against m1 roadmap)

- [ ] `lp-base/lp-perf` crate exists with `noop` / `syscall` / `log` sinks.
- [ ] Engine emits `frame`, `project-load`, `shader-compile` events.
- [ ] lpvm-native emits `shader-link` events.
- [ ] fw-emu enables `lp-perf/syscall`; fw-esp32 stays at noop.
- [ ] `SYSCALL_PERF_EVENT = 10` reserved in `lp-riscv-emu-shared`.
- [ ] `SYSCALL_JIT_MAP_LOAD = 11` and `SYSCALL_JIT_MAP_UNLOAD = 12` reserved.
- [ ] Host syscall handler routes events to `ProfileSession::on_perf_event`.
- [ ] `EventsCollector` writes `events.jsonl` with `cycle/name/kind`.
- [ ] `ProfileMode` selectable on CLI; `steady-render` is default.
- [ ] `--max-cycles` enforced with warning on hit; exit 0.
- [ ] `--frames` removed.
- [ ] `meta.json` contains `mode`, `max_cycles`, `cycles_used`, `terminated_by`.
- [ ] Trace dir name format: `<ts>--<workload>--<mode>[--<note>]`.
- [ ] CLI handler split into `handler.rs` + `workload.rs` + `output.rs` + `mode/`.
- [ ] m0 frame-driving bug fixed: `run_until_yield_or_stop` actually executes guest steps.
- [ ] All tests in `cargo test --workspace` pass.
- [ ] `profiles/` is in `.gitignore` (already done in m0).

## Out of scope (deferred follow-ups)

- `Enable`/`Disable` gate semantics (m2).
- Per-event one-shot warning de-duplication for unknown event names.
- Performance overhead measurement (informal: `events` collector
  should add <5% wall time vs `--collect alloc` only).
- Schema versioning beyond v1 (no consumers yet).
