# Phase 7: End-to-end integration test

> Read `00-notes.md` and `00-design.md` for shared context.
> Depends on phases 1-6 all landing.

## Scope of phase

Add a single end-to-end test that exercises the full chain: a small
GLSL shader compiled by `lpvm-native` runs in `fw-emu` under
`fw-tests`, the host receives `SYSCALL_JIT_MAP_LOAD`, the resulting
`meta.json` contains `dynamic_symbols`, and the produced report (or
direct symbolizer call against the produced trace) resolves a JIT'd
PC to the expected shader function name.

This is the proof that all six previous phases compose. It is also
the proof that we have not regressed `scene_render_emu` or
`profile_alloc_emu`.

### In scope

- New test in `fw-tests` (likely a sibling of
  `fw-tests/tests/profile_alloc_emu.rs`) named something like
  `profile_jit_symbols_emu.rs`. Pick a name and location consistent
  with the existing tests in that crate.
- The test:
  1. Picks a small shader fixture that produces at least one named
     function (use an existing fixture from `fw-tests/fixtures/` or
     similar; if none is suitable, add a tiny one — but first try
     re-using).
  2. Runs the existing `fw-emu` profile flow that produces a trace
     directory.
  3. Reads `meta.json` from the produced trace dir.
  4. Asserts `dynamic_symbols` is present, non-empty, and contains an
     entry whose `name` matches the expected shader function.
  5. Picks a PC inside that entry's `[addr, addr+size)` range and
     asserts the CLI `Symbolizer` (phase 5) resolves it to that name.
- If the existing `fw-tests` infrastructure makes (4) trivial via an
  existing report-checking helper, prefer that. Don't reinvent.

### Out of scope

- Any production-code changes. If you need a code change to make this
  test work, **stop and report** — that's a phase 1-6 oversight.
- Performance / regression benchmarking.
- Adding a new fixture if an existing one will do.

## Code Organization Reminders

- Test helpers at the **bottom** of the test file.
- One thing tested per `#[test]`. If you want to assert multiple
  facts, group them into one cohesive end-to-end test (the chain is
  the unit of value here).
- Keep the test under ~50 lines of actual logic. If it's growing,
  factor a builder.

## Sub-agent Reminders

- Do **not** commit.
- Do **not** modify production code. If something doesn't work, that's
  a real bug — stop and report it, don't paper over it.
- Do **not** weaken or skip pre-existing `fw-tests`.
- Do **not** add `#[ignore]` to make a flaky test pass.
- Do **not** suppress warnings.
- If the test reveals real wiring bugs, **stop and report** with the
  exact failure. Do not start patching production code from this
  phase.
- Report back: files changed, validation output, any deviations.

## Implementation Details

### 1. Mirror an existing test

Open `fw-tests/tests/profile_alloc_emu.rs` (or whichever existing
profiling test is closest in shape). Mirror its scaffolding:

- Same `#[cfg]` flags / harness.
- Same fixture loading pattern.
- Same trace-dir reading.

### 2. Pick a fixture

Browse `fw-tests/fixtures/` (or wherever the existing tests look) for
a shader with a clearly-named function. If multiple are suitable,
pick the smallest — runtime matters less than fixture stability.

If nothing is suitable, add a minimal GLSL fixture
`palette_warm.glsl` (or similar) with an obviously-named function,
and document why an existing fixture didn't work.

### 3. Test body sketch

```rust
#[test]
fn jit_symbols_round_trip_to_meta_and_symbolizer() {
    let trace_dir = run_fixture_under_profile("palette_warm.glsl");

    let meta_path = trace_dir.join("meta.json");
    let meta: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&meta_path).expect("read meta.json"),
    )
    .expect("parse meta.json");

    let dynamic = meta["dynamic_symbols"]
        .as_array()
        .expect("dynamic_symbols present and array");
    assert!(!dynamic.is_empty(), "dynamic_symbols should be non-empty");

    let entry = dynamic
        .iter()
        .find(|e| e["name"].as_str() == Some("palette_warm"))
        .expect("palette_warm not in dynamic_symbols");

    let addr = entry["addr"].as_u64().expect("addr");
    let size = entry["size"].as_u64().expect("size");

    // Sanity check: PC in the middle of the function should resolve.
    let pc = addr + size / 2;

    let symbolizer = lp_cli::profile::Symbolizer::load(&meta_path)
        .expect("Symbolizer::load");
    let name = symbolizer.lookup(pc).resolved();
    assert_eq!(name, Some("palette_warm"));
}
```

Adjust:

- `run_fixture_under_profile` to whatever helper the existing tests
  use (might be inline in those tests; copy the pattern).
- `lp_cli::profile::Symbolizer::load` to the actual public path. If
  it isn't `pub`, prefer adding a thin `pub` wrapper to lp-cli rather
  than reaching into private modules.
- Function name "palette_warm" to whatever the chosen fixture
  defines.

### 4. If the symbolizer isn't easily callable from `fw-tests`

Two options:

a. Read `meta.json` directly in the test and do a manual interval
   lookup against `dynamic_symbols`. This is fine — the goal is to
   prove the chain works, and a direct interval check on the
   serialized output is a real integration test of phases 1-4.
b. Expose a small `pub` function in `lp-cli::profile` that takes a
   meta.json path + PC and returns a name. Cleaner, but only if
   warranted.

Pick (a) by default. If the resulting test is awkward, escalate.

## Validate

```bash
# The new test, plus the two pre-existing emu tests we must not regress.
cargo test -p fw-tests --test profile_jit_symbols_emu \
                       --test profile_alloc_emu \
                       --test scene_render_emu

# Also: the firmware builds we depend on must still cleanly build.
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```

All four invocations must succeed cleanly.
