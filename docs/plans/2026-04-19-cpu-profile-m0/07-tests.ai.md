# Phase 7 — Tests: rewire emu test, add CLI smoke test

Restore the end-to-end alloc test on the new API (it was
intentionally left broken at the end of phase 6) and add a new
CLI-level smoke test that drives `lp-cli profile --collect alloc`
and checks the produced trace dir contents.

Depends on phase 6.

## Subagent assignment

`generalPurpose` subagent. Two test files; bounded changes.

## Files to touch

```
lp-fw/fw-tests/tests/alloc_trace_emu.rs   → RENAME → profile_alloc_emu.rs
lp-cli/tests/profile_alloc_smoke.rs       → NEW
```

## Step 1 — Rename + rewire `alloc_trace_emu.rs`

### Rename

```bash
git mv lp-fw/fw-tests/tests/alloc_trace_emu.rs \
       lp-fw/fw-tests/tests/profile_alloc_emu.rs
```

### Rewire body

The test currently:

1. Builds fw-emu with `--features alloc-trace` (already updated
   to `--features profile` in phase 2).
2. Constructs `Riscv32Emulator::new(...).with_alloc_trace(...)`.
3. Drives the workload.
4. Calls `finish_alloc_trace()`.
5. Asserts on `heap-trace.jsonl` content / `meta.json` content.

Post-phase-6, `with_alloc_trace`/`finish_alloc_trace` are gone.
Update to:

```rust
use lp_riscv_emu::profile::{
    AllocCollector, Collector, ProfileSession, SessionMetadata, TraceSymbol,
};

let trace_dir = tempfile::tempdir()?;
let symbols = extract_symbols(&elf);   // same helper used today

let metadata = SessionMetadata {
    schema_version: 1,
    timestamp: "2026-01-01T00:00:00Z".into(),  // fixed string for test stability
    project: "fw-tests".into(),
    workload: "alloc-emu".into(),
    note: None,
    clock_source: "emu_estimated",
    frames_requested: FRAMES,
    symbols,
};

let alloc = Box::new(AllocCollector::new(trace_dir.path(), heap_start, heap_size)?);

let mut emu = Riscv32Emulator::new(...)
    .with_profile_session(
        trace_dir.path().to_path_buf(),
        &metadata,
        vec![alloc],
    )?;

// drive workload (unchanged)
for _ in 0..FRAMES {
    emu.advance_time(40)?;
}

emu.finish_profile_session()?;
```

### Update assertions

The existing assertions on `heap-trace.jsonl` body lines should
pass unchanged — the wire format is preserved.

The existing assertions on `meta.json` shape will likely fail
because phases 1+5 restructured it (added `schema_version`,
`clock_source`, `workload`, `note`, `frames_requested`,
`collectors.alloc`). Update them to match the new shape, per
the design doc.

If the test does any byte-for-byte snapshot comparison of
`meta.json`, replace with structured assertions
(e.g. `serde_json::from_str` then check fields), so future
metadata extensions don't break it.

If it asserts on `report.txt` content (possibly only via
absence/presence), update for the new banner format
(`=== Heap Allocation ===` first line).

## Step 2 — `lp-cli/tests/profile_alloc_smoke.rs`

A new integration test that shells out to the built `lp-cli`
binary and verifies the trace dir contents end-to-end.

Pattern (use `assert_cmd` if already a dev-dep, else `Command`):

```rust
use std::process::Command;
use tempfile::tempdir;

#[test]
fn profile_alloc_smoke() {
    let workdir = tempdir().unwrap();

    // Run from a scratch CWD so `profiles/` shows up under `workdir`.
    let status = Command::new(env!("CARGO_BIN_EXE_lp-cli"))
        .current_dir(workdir.path())
        .args([
            "profile",
            "examples/basic",          // resolved relative to the workspace root
            "--collect", "alloc",
            "--frames", "2",
            "--note", "smoke",
        ])
        .env("CARGO_MANIFEST_DIR", env!("CARGO_MANIFEST_DIR"))
        .status()
        .expect("failed to spawn lp-cli");
    assert!(status.success(), "lp-cli profile failed");

    // Find the produced dir.
    let profiles = workdir.path().join("profiles");
    let entry = std::fs::read_dir(&profiles).unwrap()
        .next().unwrap().unwrap();
    let dir = entry.path();
    assert!(dir.file_name().unwrap().to_string_lossy().ends_with("--smoke"));

    // Check expected files exist and are non-empty.
    for f in ["meta.json", "heap-trace.jsonl", "report.txt"] {
        let p = dir.join(f);
        assert!(p.exists(), "missing {}", p.display());
        assert!(std::fs::metadata(&p).unwrap().len() > 0, "empty {}", p.display());
    }

    // Sanity-check meta.json shape.
    let meta: serde_json::Value =
        serde_json::from_reader(std::fs::File::open(dir.join("meta.json")).unwrap()).unwrap();
    assert_eq!(meta["schema_version"], 1);
    assert!(meta["collectors"]["alloc"].is_object());
    assert!(meta["collectors"]["alloc"]["heap_start"].is_number());

    // Sanity-check report banner.
    let report = std::fs::read_to_string(dir.join("report.txt")).unwrap();
    assert!(report.contains("=== Heap Allocation ==="));
}
```

Caveats:
- Resolving `examples/basic` from a temp CWD is going to fail
  unless the path is made absolute. Construct it via
  `std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/basic")`
  (adjust depth) or whatever pattern the existing CLI tests use.
  Check `lp-cli/tests/` for prior art before reinventing.
- The test needs the lp-cli binary, which means a fairly heavy
  build cost. Mark with `#[ignore]` if it's prohibitive in
  default `cargo test`, but **prefer not to** — the point of this
  test is to be a real CI gate.

Also add a separate small test (in the same file) for the diff
stub:

```rust
#[test]
fn profile_diff_stub_exits_nonzero() {
    let out = Command::new(env!("CARGO_BIN_EXE_lp-cli"))
        .args(["profile", "diff", "/tmp/a", "/tmp/b"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("not yet implemented"));
    assert!(stderr.contains("m2"));
}
```

## Validation

```bash
cargo test -p fw-tests --test profile_alloc_emu
cargo test -p lp-cli --test profile_alloc_smoke

# Full workspace, just to make sure nothing else regressed.
cargo test --workspace
```

## Out of scope for this phase

- Anything in examples/, justfile, docs (phase 8).
