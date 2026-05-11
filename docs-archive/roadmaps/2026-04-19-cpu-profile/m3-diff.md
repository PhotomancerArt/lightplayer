# Milestone 3: Diff

## Goal

Make A/B comparisons between profile runs a one-shot. Implement the
`lp-cli profile diff <a> <b>` standalone subcommand (stub from m0
becomes real) and add the `--diff [PATH]` convenience flag to `lp-cli
profile` that auto-finds the most recent prior matching run and
diffs against it after the new profile completes.

This is the milestone that turns m2's "I have a flame chart" into the
user's primary question answered: *"I changed X. Did cycles go up or
down, and where?"*

## Suggested Plan Name

`profile-m3-diff`

## Scope

### In scope

- **`lp-cli profile diff <a> <b>` standalone subcommand.** Replaces
  the m0 stub. Reads `cpu-profile.json` (and optionally
  `heap-trace.jsonl`) from each trace dir, computes per-function and
  per-edge deltas, prints a diff report.

  ```
  lp-cli profile diff <trace-dir-a> <trace-dir-b>
                      [--top N=20]
                      [--threshold-pct F=1.0]
                      [--threshold-cycles N=1000]
                      [--sort {regression,improvement,absolute}=regression]
  ```

  Output (text by default):

  ```
  Profile diff (a → b)
  ====================
    cycle_model:   esp32c6 → esp32c6
    mode:          steady-render → steady-render
    workload:      examples-basic → examples-basic
    total cycles:  11,000,000 → 9,800,000  (-1,200,000  -10.9%)

  Top 20 regressions (by Δ self_cycles):
       +384,512  +18.2%  shader::rainbow::palette_warm
       +120,004   +6.5%  ...

  Top 20 improvements (by Δ self_cycles):
     -1,632,000  -84.1%  led_driver::push_pixel  ← target of perf fix
       -120,000  -12.0%  ...

  Functions added in B (top 5 by self_cycles):  none
  Functions removed in A (top 5 by self_cycles):  led_driver::push_pixel_legacy

  Filtered: 47 functions below thresholds (--threshold-pct=1.0 --threshold-cycles=1000)
  ```

- **`--diff [PATH]` flag on `lp-cli profile`**:
  - `--diff` (no arg): after profile completes, find the most recent
    prior trace dir with the same `<workload>--<mode>` prefix that
    isn't the run we just finished. Run a diff. Append the diff
    report below the regular `report.txt` output on stdout.
  - `--diff <path>`: diff against that explicit trace dir.
  - Absent: just the new profile's summary, no diff.

- **Auto-find-prior logic.** Given the just-finished trace dir like
  `traces/2026-04-19T15-30-22--examples-basic--steady-render/`,
  enumerate `traces/*--examples-basic--steady-render*`, sort by
  timestamp prefix, pick the most recent that isn't the just-finished
  one. If none found: print "no prior matching run; skipping diff."
  Don't error — `--diff` on the first run of a workload is a valid
  no-op.

- **Diff data model.** In new module
  `lp-cli/src/commands/profile/diff.rs`:

  ```rust
  pub struct ProfileDiff {
      pub a: ProfileSummary,          // from cpu-profile.json
      pub b: ProfileSummary,
      pub func_deltas: Vec<FuncDelta>,
      pub edge_deltas: Vec<EdgeDelta>,
      pub funcs_added: Vec<FuncEntry>,
      pub funcs_removed: Vec<FuncEntry>,
  }
  ```

  Functions matched by symbol name (preferred) or by PC if names
  unavailable. Cross-binary PC drift is a real concern; symbol-name
  matching is the default.

- **Threshold filtering.** A function appears in the report only if
  `abs(delta_cycles) >= threshold_cycles` AND
  `abs(delta_pct) >= threshold_pct`. Default both to nonzero so
  "noise" entries (tiny inlining-driven shifts) don't dominate the
  report. Functions filtered out are summarized as one line at the
  bottom.

- **Default sort: regressions first.** When triaging "did I make it
  worse?", regressions are what you scroll to first. `--sort
  improvement` flips it; `--sort absolute` orders by `abs(delta)`.

- **Compatibility check.** When diffing, verify both trace dirs have
  matching `cycle_model` and `mode` in their `meta.json`. Mismatch
  emits a warning ("comparing esp32c6 vs uniform; deltas may be
  misleading") but proceeds. Schema version mismatch emits an error
  and aborts.

- **Alloc diff (when both runs include alloc collector).** When both
  trace dirs contain `heap-trace.jsonl`, also produce an alloc diff
  section: top-N functions by Δ allocations, top-N by Δ bytes
  allocated. Same threshold mechanism.

- **`--format json` (CI integration affordance).** Optional output as
  JSON for CI systems that want to assert "no regression > X cycles."
  Single flag, ~40 LOC. Schema: a flat dump of the `ProfileDiff`
  struct.

- **Tests.**
  - Unit test: synthetic `cpu-profile.json` pair → expected
    `ProfileDiff` structure.
  - Unit test: auto-find-prior logic against a synthetic `traces/`
    directory layout.
  - Unit test: threshold filtering edge cases (exactly at threshold,
    zero delta, missing function).
  - Unit test: schema-version mismatch errors.
  - Integration test: run `profile --collect cpu` twice against
    `examples/basic`, then `profile diff` between them; verify the
    diff identifies known cycle differences (within tolerance).

### Out of scope

- `HardwarePerfSink` and device-side perf-log diff — m4 (`perf-log-diff`
  mode lives there since it operates on `events.jsonl` instead of
  `cpu-profile.json`).
- Four-corner correlation report — m4.
- JIT symbol overlay — m5. (Diff continues to use static ELF symbols
  + `<jit:0xADDR>` placeholders; matches by PC fall back when
  unsymbolized regions diverge.)
- Per-call-edge differential flame chart — possible future-work; not
  in scope.
- Markdown / HTML output formats — possible future-work.

## Key Decisions

- **Symbol-name matching first, PC matching fallback.** Two builds of
  the same firmware will have different absolute PCs (linker
  arrangement varies). Matching by mangled symbol name preserves
  function identity across builds. PCs are the fallback for
  symbol-less regions (e.g. JIT code before m5).

- **Regressions sorted first by default.** This is the question
  developers actually ask after making changes. Optimization-celebration
  mode is `--sort improvement`.

- **Both threshold flags default to nonzero.** Eliminates noise entries
  from inlining shifts and other "every function moved by 100 cycles"
  artifacts. Makes the report scannable.

- **`--format json` ships with m3, not as future-work.** Trivial cost,
  enables CI gating, and we don't want CI integration to require a
  separate later milestone.

- **Compatibility mismatch is a warning, not an error.** A diff
  between `esp32c6` and `uniform` runs is *informative* (you might be
  intentionally checking model sensitivity); just label it clearly.
  Schema-version mismatch is the only hard error (we genuinely can't
  parse old data).

- **Auto-find-prior is workload+mode keyed, not just workload.** A
  `steady-render` profile and a `compile` profile are not comparable;
  `--diff` should only auto-pair runs of the same kind.

- **Alloc diff piggybacks on cpu diff.** Same threshold mechanism,
  same report shape, separate section. No new command — `lp-cli
  profile diff <a> <b>` handles both axes if both ran.

## Deliverables

### `lp-cli` crate
- New: `lp-cli/src/commands/profile/diff.rs` — `ProfileDiff` data
  model, diff computation, output formatting.
- New: `lp-cli/src/commands/profile/diff_finder.rs` — auto-find-prior
  logic.
- Updated: `lp-cli/src/commands/profile/args.rs` — add `--diff [PATH]`
  to main `profile` args; add `--top`, `--threshold-pct`,
  `--threshold-cycles`, `--sort`, `--format` to `profile diff`
  subcommand args.
- Updated: `lp-cli/src/commands/profile/handler.rs` — invokes
  diff_finder + diff after profile completes when `--diff` set.
- Updated: `lp-cli/src/commands/profile/mod.rs` — replaces the m0
  diff stub with real implementation.

### Tests
- `lp-cli/tests/profile_diff_unit.rs` — unit tests for
  `ProfileDiff`, auto-find-prior, thresholds.
- `lp-cli/tests/profile_diff_integration.rs` — end-to-end
  two-run-then-diff against `examples/basic`.

## Dependencies

- m0 — Foundation refactor (trace dir layout, m0's stub diff command
  to replace).
- m2 — CPU collector + `cpu-profile.json` writer (without the
  canonical JSON, there's nothing to diff).

m1 not strictly required (`events.jsonl` not consumed by m3), but in
practice m3 will be implemented after m2 which depends on m1.

## Validation

```bash
# Workspace builds
cargo build --workspace

# Unit tests
cargo test -p lp-cli

# End-to-end: two runs + standalone diff
cargo run -p lp-cli --release -- profile examples/basic
# Then make a small change to a shader function, rebuild, rerun
cargo run -p lp-cli --release -- profile examples/basic
# Diff
cargo run -p lp-cli --release -- profile diff \
  traces/<earlier-sess> traces/<later-sess>
# Expect: report shows the changed function in regressions or
# improvements section with appropriate sign.

# Auto-find-prior
cargo run -p lp-cli --release -- profile examples/basic --diff
# Same diff appears below the new profile's summary.

# First-run sanity
rm -rf traces/
cargo run -p lp-cli --release -- profile examples/basic --diff
# Expect: "no prior matching run; skipping diff" — no error.

# JSON output
cargo run -p lp-cli --release -- profile diff \
  traces/<a> traces/<b> --format json | jq '.summary.total_cycles_delta'
# Expect: numeric value parseable by jq.

# Composability
cargo run -p lp-cli --release -- profile examples/basic --collect cpu,alloc
cargo run -p lp-cli --release -- profile examples/basic --collect cpu,alloc --diff
# Diff report includes both "CPU diff" and "Alloc diff" sections.

# Compatibility warning
cargo run -p lp-cli --release -- profile examples/basic --cycle-model uniform
cargo run -p lp-cli --release -- profile diff \
  traces/<esp32c6-sess> traces/<uniform-sess>
# Expect: warning about cycle_model mismatch but report produces.
```

## Estimated Scope

- New code: ~600-800 LOC.
  - Diff data model + computation: ~250-300.
  - Auto-find-prior: ~80-120.
  - Output formatters (text + JSON): ~200-250.
  - CLI args + handler integration: ~100-150.
- Tests: ~300-400 LOC.
- Files touched: ~6-8.

## Agent Execution Notes

Implementation order:

1. Read `lp-cli/src/commands/profile/handler.rs` and the m0 diff stub
   to understand existing structure.
2. Read `cpu-profile.json` schema (from m2) to understand input data.
3. Implement `ProfileDiff` data model + computation pure-functionally.
   Heavy unit tests with synthetic input pairs.
4. Implement text output formatter. Iterate on layout against a
   real diff between two `examples/basic` runs.
5. Implement JSON output formatter (small; serde derive most of it).
6. Implement auto-find-prior logic. Test against synthetic `traces/`
   layout and against real workspace traces.
7. Wire `--diff [PATH]` into `profile` handler.
8. Replace m0 stub with real `profile diff` subcommand.
9. End-to-end test: two real runs + diff. Verify a known cycle
   change shows up in the report.
