# Phase 8 — Integration tests + cleanup

End-to-end integration tests against `examples/basic`. Documentation
sweep — README mentions of the profile command, AGENTS.md
collector list, and any docs referring to the m1 default of
`--collect events`. Lint pass and final cleanup.

**Manual review** — needs human eyes on the integration test outputs
and any docs touched.

## Dependencies

- All previous phases (P1–P7).

## Files

### `lp-cli/tests/profile_cpu.rs` — new

Four end-to-end tests using `assert_cmd` (or whatever m1's existing
integration tests use):

#### Test 1 — `cpu_default_smoke`

```rust
#[test]
fn cpu_default_smoke() {
    let dir = tempdir().unwrap();
    let status = Command::cargo_bin("lp-cli").unwrap()
        .args(["profile", "examples/basic",
               "--dir", dir.path().to_str().unwrap(),
               "--max-cycles", "10000000"])
        .assert()
        .success();

    let trace_dir = find_only_subdir(dir.path());
    assert!(trace_dir.join("meta.json").exists());
    assert!(trace_dir.join("events.jsonl").exists());
    assert!(trace_dir.join("cpu-profile.json").exists());
    assert!(trace_dir.join("cpu-profile.speedscope.json").exists());
    assert!(trace_dir.join("report.txt").exists());
    assert!(!trace_dir.join("heap-trace.jsonl").exists());

    // Validate cpu-profile.json
    let cpu: serde_json::Value = serde_json::from_slice(
        &std::fs::read(trace_dir.join("cpu-profile.json")).unwrap()
    ).unwrap();
    assert_eq!(cpu["schema_version"], 1);
    assert_eq!(cpu["cycle_model"], "esp32c6");
    assert!(cpu["total_cycles_attributed"].as_u64().unwrap() > 0);
    assert!(cpu["func_stats"].as_object().unwrap().len() > 0);

    // Validate report.txt has CPU summary
    let report = std::fs::read_to_string(trace_dir.join("report.txt")).unwrap();
    assert!(report.contains("=== CPU summary ==="));
    assert!(report.contains("cycle_model=esp32c6"));
}
```

#### Test 2 — `cpu_with_alloc`

```rust
#[test]
fn cpu_with_alloc() {
    let dir = tempdir().unwrap();
    Command::cargo_bin("lp-cli").unwrap()
        .args(["profile", "examples/basic",
               "--dir", dir.path().to_str().unwrap(),
               "--collect", "cpu,alloc",
               "--max-cycles", "10000000"])
        .assert()
        .success();

    let trace_dir = find_only_subdir(dir.path());
    assert!(trace_dir.join("cpu-profile.json").exists());
    assert!(trace_dir.join("heap-trace.jsonl").exists());
    assert!(trace_dir.join("events.jsonl").exists()); // auto-included
}
```

#### Test 3 — `cpu_uniform_model`

```rust
#[test]
fn cpu_uniform_model() {
    let dir = tempdir().unwrap();
    Command::cargo_bin("lp-cli").unwrap()
        .args(["profile", "examples/basic",
               "--dir", dir.path().to_str().unwrap(),
               "--cycle-model", "uniform",
               "--max-cycles", "10000000"])
        .assert()
        .success();

    let trace_dir = find_only_subdir(dir.path());
    let meta: serde_json::Value = serde_json::from_slice(
        &std::fs::read(trace_dir.join("meta.json")).unwrap()
    ).unwrap();
    assert_eq!(meta["cycle_model"], "uniform");

    let cpu: serde_json::Value = serde_json::from_slice(
        &std::fs::read(trace_dir.join("cpu-profile.json")).unwrap()
    ).unwrap();
    assert_eq!(cpu["cycle_model"], "uniform");

    // Sanity: with uniform model, every cycle = 1 instruction.
    // total_cycles_attributed should be roughly the number of instructions
    // executed during the steady-render window. Not asserting exact
    // because warmup boundary is fuzzy.
}
```

#### Test 4 — `cpu_compile_mode`

```rust
#[test]
fn cpu_compile_mode() {
    // Ensures the gate→Enable-on-profile-start wiring works for non-steady-render modes.
    let dir = tempdir().unwrap();
    Command::cargo_bin("lp-cli").unwrap()
        .args(["profile", "examples/basic",
               "--dir", dir.path().to_str().unwrap(),
               "--mode", "compile",
               "--max-cycles", "10000000"])
        .assert()
        .success();

    let trace_dir = find_only_subdir(dir.path());
    let cpu: serde_json::Value = serde_json::from_slice(
        &std::fs::read(trace_dir.join("cpu-profile.json")).unwrap()
    ).unwrap();
    // Compile mode captures from boot to first shader-compile end.
    // Should have non-trivial cycle attribution.
    assert!(cpu["total_cycles_attributed"].as_u64().unwrap() > 1000);
    assert!(cpu["func_stats"].as_object().unwrap().len() > 5);

    // events.jsonl should have profile:start as the first event.
    let events: Vec<serde_json::Value> = std::fs::read_to_string(
        trace_dir.join("events.jsonl")
    ).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(events[0]["name"], "profile:start");
}
```

### Documentation sweep

Search the repo for references to the old `--collect` default and
the m1 CLI surface:

```bash
rg -l '\-\-collect events' docs/
rg -l '\-\-collect alloc' docs/
rg -l 'cycle_model' docs/
```

Update:

- `README.md` (if it mentions `lp-cli profile` examples) — show
  `--collect cpu` as the default; add a one-liner about
  `--cycle-model`.
- `docs/roadmaps/2026-04-19-cpu-profile/m2-cpu-collector.md` — mark
  m2 as **landed** at the top, link to the m2 plan dir.
- `AGENTS.md` — if it documents available collectors, add `cpu`.

### Code cleanup

```bash
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

Address any new lints introduced by m2 changes.

### `Cargo.toml` audit

Confirm no new dependencies were added by m2. The design doc says
no new deps — verify with `git diff Cargo.toml lp-cli/Cargo.toml
lp-riscv/lp-riscv-emu/Cargo.toml`.

## Tests

The four integration tests above. Plus running the full workspace
test suite:

```bash
cargo test --workspace
```

Manual smoke tests:

1. `lp-cli profile examples/basic` — produces all expected files.
2. `cargo run -p lp-cli -- profile examples/basic --mode all
   --max-cycles 50000000` — flame chart in
   `cpu-profile.speedscope.json`. Drop into
   https://www.speedscope.app/ and visually confirm.
3. `lp-cli profile examples/basic --cycle-model uniform` — confirm
   `meta.json` and `cpu-profile.json` both show
   `cycle_model: "uniform"`.

## Risk + rollout

- **Risk**: integration test flakiness. `examples/basic`'s exact
  cycle count varies between runs (especially with JIT). Tests
  assert "non-empty" and "structurally correct" — not exact
  numbers. Avoids flake.
- **Risk**: integration tests depend on `examples/basic` being
  buildable and runnable. m1's tests already do this, so this is
  not a new risk.
- **Risk**: docs drift. After P8 merges, anyone re-reading the m2
  roadmap should see "landed" at the top. If we forget, the
  roadmap stays in `docs/roadmaps/` indefinitely as in-progress.

## Acceptance

- `cargo test --workspace` passes.
- `cargo clippy --workspace --all-targets -- -D warnings` clean.
- `cargo fmt --all` shows no diff.
- All four new integration tests pass.
- Manual smoke test 2 (browser flame chart) renders correctly.
- Roadmap m2 doc marked landed.
- README and AGENTS.md reflect new default and `--cycle-model` flag.
