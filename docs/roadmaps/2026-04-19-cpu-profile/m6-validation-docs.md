# Milestone 6: Validation, Cleanup, and Documentation

## Goal

Close the roadmap. Validate the full profiler end-to-end with a
synthetic A/B test where the answer is known a priori; remove any
temporary scaffolding accumulated across earlier milestones; write
the long-form documentation home at
`docs/design/native/fw-profile/` so the system is discoverable and
maintainable after the roadmap completes.

This is the milestone that turns "the feature works" into "the
feature is production infrastructure."

## Suggested Plan Name

`profile-m6-validation-docs`

## Scope

### In scope

- **Synthetic A/B validation suite.** A small set of deliberate
  micro-changes to `examples/basic` (or a dedicated
  `examples/profile-validation/` if isolating is cleaner) where the
  cycle-impact prediction is independently verifiable:

  1. **Add a busy-loop**: insert `for i in 0..10000 { black_box(i)
     }` in a known function. Verify diff attributes ~10000 *
     loop-cycle-cost to that function.
  2. **Move work between functions**: take 1000 cycles of arithmetic
     out of `f1` and put them in `f2`. Verify diff shows ~−1000
     in `f1` and ~+1000 in `f2` — no third party shows up.
  3. **Add a deep call**: introduce a 5-deep call chain. Verify
     speedscope flame chart shows the chain at correct depth and
     cycle proportions.
  4. **Add a tail call**: convert a regular call to a tail call.
     Verify caller credit is preserved (no orphan attribution).
  5. **Remove a function entirely**: verify diff's "removed
     functions" section identifies it correctly.

  Each test is automated as an integration test in
  `lp-cli/tests/profile_validation.rs`. Tolerances accommodate
  cycle-model imprecision (e.g. ±5%) but the *direction* and
  *attribution target* must be exact.

- **Hardware correlation validation (gated on hardware availability).**
  One real four-corner workflow run, captured into
  `docs/design/native/fw-profile/correlation-baseline.md` as a
  reference data point: "as of m6 completion, the cycle model
  achieves N% sign-agreement and ±Mpp magnitude error on
  examples/basic." Future regressions in correlation become
  detectable against this baseline.

- **Scaffolding removal pass.** Sweep the codebase for:
  - Debug `eprintln!` / `dbg!` calls added during m1-m5
    development.
  - Commented-out experimental code paths.
  - `TODO(m1)` / `TODO(m2)` etc. markers — either resolve, convert
    to `TODO(future)` with rationale, or open a follow-up issue.
  - Old `mem-profile` / `heap-summary` references in docs or
    comments not caught in m0.
  - Unused `#[cfg(feature = "alloc-trace")]` after the m0 rename
    to `feature = "profile"`.
  - The old `lp-riscv-emu/src/alloc_trace.rs` module if not
    fully removed in m0.
  - **Mainline tests that boot fw-emu** (decision recorded
    2026-04-19; see [m2 → Test-infrastructure follow-up](./m2-cpu-collector.md#test-infrastructure-follow-up-action-item)).
    Concrete targets:
    - `lp-cli/tests/profile_cpu_smoke.rs` (3 tests, ~10-15 min serial) →
      mark `#[ignore]` and document the on-demand invocation.
    - `lp-cli/tests/profile_alloc_smoke.rs` (~3 min) → same treatment.
    - `lp-cli/tests/profile_events_steady_render_smoke.rs` → same
      treatment.
    Replace the lost mainline coverage with small-program tests in
    `lp-riscv-emu/tests/` modeled on `abi_tests.rs` /
    `guest_app_tests.rs`: tiny cranelift-compiled RV32 fixtures with
    known call structures, run directly through `Riscv32Emulator` +
    `ProfileSession`, asserting against `CpuCollector` / `EventsCollector`
    / `AllocCollector` internals. These run in milliseconds and stay in
    the default `cargo test` path.

- **Documentation home: `docs/design/native/fw-profile/`.**
  New directory with the following files:

  - `README.md` — top-level index and quickstart:
    ```
    # fw-profile: Firmware profiling toolkit
    
    Quickstart:
      lp-cli profile examples/basic
      lp-cli profile examples/basic --diff
      lp-cli profile diff <a> <b>
    
    See: architecture.md, perf-events.md, cycle-model.md,
         hardware-correlation.md, jit-symbols.md, schemas.md
    ```
  - `architecture.md` — high-level: collector pattern,
    `ProfileSession`, perf-event substrate, dual emu/device path.
    Diagram showing data flow from engine emission → emu syscall →
    collector → trace dir → diff.
  - `perf-events.md` — event vocabulary, `PerfEventSink` trait,
    `ProfileMode` semantics, how to add a new event boundary,
    how to add a new mode.
  - `cycle-model.md` — `CycleModel::Esp32C6` rationale, cost-class
    table, when to refine, how to validate refinements via
    correlation.
  - `hardware-correlation.md` — four-corner workflow,
    `correlation-baseline.md` interpretation, expected agreement
    levels, how to debug bias.
  - `jit-symbols.md` — `SYSCALL_JIT_MAP_LOAD` ABI,
    `JitSymbols` overlay, status of `_UNLOAD` (reserved, not
    implemented, design note for future hot-reload).
  - `schemas.md` — wire formats: `events.jsonl`, `cpu-profile.json`,
    `meta.json`, `heap-trace.jsonl`. Schema-version policy.
  - `correlation-baseline.md` — captured baseline correlation
    numbers from m6's hardware run.

- **Roadmap completion checkmark.** Update
  `docs/roadmaps/2026-04-19-cpu-profile/overview.md` with a
  completion section: dates per milestone, link to the
  documentation home, link to the baseline correlation report,
  any deferred follow-up items captured as future-work.

- **Follow-up tracking.** Capture deferred items as a clear list at
  the end of `overview.md` so they're discoverable:
  - `--raw-events` opt-in for raw event stream.
  - `SYSCALL_JIT_MAP_UNLOAD` implementation when hot-reload
    arrives.
  - Per-event `arg: u32` payload (ABI room reserved).
  - Other device targets beyond ESP32-C6.
  - Markdown / HTML diff output formats.
  - Per-basic-block / source-line JIT symbolization.
  - Live "perf top" device streaming.

- **Tests.**
  - The synthetic A/B suite itself becomes the tests.
  - `lp-cli/tests/profile_validation.rs` runs as part of regular
    `cargo test` (each test is fast — a few hundred milliseconds
    of emulation each).

### Out of scope

- New profiler features. m6 is closing the loop, not extending it.
- Refactoring the hot-path loop architecture. Captured as
  future-work if it proves needed.
- Building a CI gate that *enforces* correlation-baseline
  thresholds. Documented as a possible future job.
- A general "profiler tutorial" video / screencast — out of scope
  for code-side documentation.

## Key Decisions

- **Synthetic A/B validates direction and attribution, not absolute
  cycle counts.** The cycle model is approximate by design; what
  matters is that "I added 10K cycles of loop body, the profiler
  attributes ~10K extra cycles to *the function I changed*." Drift
  in the absolute count is fine.

- **Correlation baseline lives in-repo as documentation, not as
  a CI assertion.** Captures *what was achievable* at m6. Future
  drift becomes investigable; whether to enforce it via CI is a
  separate decision.

- **Documentation lives in `docs/design/native/fw-profile/`.** Per
  the design-doc convention used elsewhere in the repo. Roadmap
  documents in `docs/roadmaps/...` are *plans*; design docs in
  `docs/design/...` are *what is*.

- **Follow-up tracking in `overview.md`, not a fresh roadmap.**
  Each item is small and isolated. If any grows into a real
  multi-milestone effort, it can spawn its own roadmap then.

- **Scaffolding removal is its own deliberate pass.** Easy to skip
  in the heat of building each milestone; doing it last as a
  dedicated activity catches what would otherwise become
  permanent cruft.

- **No mainline tests boot fw-emu.** Mainline coverage for collectors
  exercises `Riscv32Emulator` + `ProfileSession` directly against tiny
  cranelift-compiled fixtures (the `abi_tests.rs` pattern) — no project
  load, no JIT shader compile, no driven frames. End-to-end tests that
  do boot fw-emu are valuable for pre-release validation but live behind
  `#[ignore]` and run one at a time. Decided 2026-04-19 after the m2
  smoke tests turned `cargo test -p lp-cli` into a 15+ minute loop.

## Deliverables

### Documentation
- New directory: `docs/design/native/fw-profile/` with eight files
  listed above.
- Updated: `docs/roadmaps/2026-04-19-cpu-profile/overview.md`
  with completion section + follow-up list.

### `lp-cli` crate
- New: `lp-cli/tests/profile_validation.rs` — synthetic A/B suite.
- Possibly: small edits to surface debug info needed by tests
  (e.g. ensuring meta.json includes test-relevant fields).

### `examples/` (possibly)
- New: `examples/profile-validation/` if isolating from
  `examples/basic` is cleaner. Contains five small variants of a
  shader/render path, each designed to exercise one validation
  case.

### Codebase-wide
- Scaffolding removal: edits scattered across whatever files
  accumulated debug code. No structural changes.

## Dependencies

- m0-m5 — full roadmap.
- For hardware correlation baseline: m4 must have produced at
  least one successful four-corner run.

## Validation

```bash
# Workspace builds cleanly
cargo build --workspace

# Full test suite green
cargo test --workspace

# Synthetic A/B suite specifically
cargo test -p lp-cli profile_validation

# Documentation renders (visual inspection)
ls docs/design/native/fw-profile/
# Expected: README.md, architecture.md, perf-events.md,
# cycle-model.md, hardware-correlation.md, jit-symbols.md,
# schemas.md, correlation-baseline.md.

# Hardware correlation baseline (manual, requires hardware)
# Re-run a four-corner workflow against current state
# Compare to numbers in docs/design/native/fw-profile/correlation-baseline.md
# If the numbers degraded vs. baseline: investigate before merging m6.

# Scaffolding sweep verification
rg -i 'TODO\(m[0-9]\)' lp-* lp-fw/ examples/
# Expected: no matches.
rg 'eprintln!\(".*profile' lp-* lp-fw/
# Expected: no matches in non-test code.
rg 'alloc_trace' lp-*  # other than in m0 rename history
# Expected: zero matches (fully migrated to profile).

# Smoke test: full happy path
cargo run -p lp-cli --release -- profile examples/basic
cargo run -p lp-cli --release -- profile examples/basic --diff
# Both succeed; reports clean and readable.

# Smoke test: composability
cargo run -p lp-cli --release -- profile examples/basic \
  --collect cpu,alloc,events
# All three sections present in report.txt.

# README quickstart works literally as written
cd /tmp && rm -rf scratch && mkdir scratch && cd scratch
git clone <repo>  # or cp
cd <repo>
# Follow docs/design/native/fw-profile/README.md quickstart commands
# Expected: each command produces the expected output.
```

## Estimated Scope

- New code (validation suite): ~300-500 LOC.
- New code (potentially `examples/profile-validation/`): ~200 LOC.
- Documentation: ~1500-2500 lines of markdown across 8 files.
- Scaffolding removal: small edits across an unknown number of
  files (estimate: ~10-20 small diffs).
- Files touched: ~25-40 (mostly small).

## Agent Execution Notes

Suggested order:

1. **Validation suite first.** The most important deliverable;
   blocks everything else. Build the five synthetic test cases,
   verify they pass against the current implementation. If a test
   reveals a real bug, fix it before continuing.
2. **Hardware correlation baseline.** Run a real four-corner
   workflow, capture the numbers, write
   `correlation-baseline.md`. (Skip if hardware unavailable; note
   in milestone exit criteria that baseline is "TBD; m6 completion
   does not strictly require it.")
3. **Scaffolding sweep.** Use the rg patterns in the validation
   section. Triage each match.
4. **Documentation.** Eight files; write them in this order so
   each builds on prior:
   1. `architecture.md` — sets vocabulary.
   2. `perf-events.md` — most foundational subsystem.
   3. `cycle-model.md` — narrow scope.
   4. `jit-symbols.md` — narrow scope.
   5. `schemas.md` — reference material; cross-links from above.
   6. `hardware-correlation.md` — depends on schemas + perf-events.
   7. `correlation-baseline.md` — small; captures baseline.
   8. `README.md` — last; links to everything.
5. **Update `overview.md`** with completion section + follow-up
   list.
6. **Final smoke run** of all validation steps in the validation
   block above.

If hardware unavailable, m6 ships without
`correlation-baseline.md` populated — captured as a follow-up to
do on next available hardware session.
