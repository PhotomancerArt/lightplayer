# Milestone 2: CPU Collector + Outputs

**Status: landed** — see the implementation plan at
[`docs/plans/2026-04-19-cpu-profile-m2-cpu-collector/`](../../plans/2026-04-19-cpu-profile-m2-cpu-collector/)
(after closeout, the same tree is archived under `docs/plans-old/2026-04-19-cpu-profile-m2-cpu-collector/`).

## Goal

Add per-function cycle attribution on top of m1's gate/event
infrastructure. Ship `lp-cli profile --collect cpu` (the default) that
produces a callgrind-style data model from a shadow-stack walk over the
emulator's instruction stream, plus three output files: `report.txt`
(text top-N), `cpu-profile.speedscope.json` (interactive flame chart),
and `cpu-profile.json` (canonical, diff-able).

This is the milestone where the user's primary question becomes
answerable: *"I changed X. Where did the cycles go?"*

## Suggested Plan Name

`profile-m2-cpu-collector`

## Scope

### In scope

- **`InstClass` extension** in `lp-riscv-emu/src/emu/cycle_model.rs`.
  New variants:
  - `JalCall` (`JAL rd, _` with `rd != x0`)
  - `JalTail` (`JAL x0, _`)
  - `JalrCall` (`JALR rd, _, _` with `rd != x0`)
  - `JalrReturn` (canonical `ret`: `JALR x0, x1, 0`; compressed
    `c.jr ra`)
  - `JalrIndirect` (other `JALR x0, _, _` patterns)

  Old `InstClass::Jal` / `InstClass::Jalr` removed; decode site
  updated to produce the new variants. Cost mapping in
  `CycleModel::Esp32C6` matches existing numbers (Jal=2, Jalr=3) for
  all new variants — no microarchitectural differentiation.

- **Decode-site update.** The instruction decoder (where `InstClass`
  is currently produced from instruction words) inspects `rd` /
  `rs1` for `JAL` / `JALR` and selects the correct variant.
  Compressed-instruction decoding handles `c.jr ra` → `JalrReturn`,
  `c.jalr` → `JalrCall`, etc. Existing emulator semantics unchanged
  — only the classifier output changes.

- **`CpuCollector`** in new file `lp-riscv-emu/src/profile/cpu.rs`.
  Internal callgrind-style data model:

  ```rust
  pub struct CpuCollector {
      shadow_stack: Vec<Frame>,
      func_stats: HashMap<u32, FuncStats>,
      call_edges: HashMap<(u32, u32), CallEdge>,
      gate: Box<dyn FnMut(&PerfEvent) -> GateAction + Send>,
      active: bool,
      total_cycles_attributed: u64,
      cycle_model_label: &'static str,
  }

  struct Frame {
      callee_pc: u32,
      caller_pc: u32,
      cycles_at_entry: u64,
      self_cycles_at_entry: u64,
  }

  struct FuncStats {
      self_cycles: u64,
      inclusive_cycles: u64,
      calls_in: u64,
      calls_out: u64,
  }

  struct CallEdge {
      count: u64,
      inclusive_cycles: u64,
  }
  ```

- **Hot-path integration.** In `run_inner_fast` and
  `run_inner_logging`, after `self.cycle_count += inst_cost`,
  dispatch to `profile_session.on_instruction(pc, inst_class,
  inst_cost)`. `CpuCollector::on_instruction`:
  1. If `!self.active`, return.
  2. Bump self-cycles for top-of-stack PC by `inst_cost`. (No
     stack? Track as orphan/root cycles.)
  3. Match on `inst_class`:
     - `JalCall` / `JalrCall` — push new frame with current PC as
       caller and target PC as callee (target known from preceding
       decode/execute step; passed via the dispatch call signature).
     - `JalrReturn` — pop top frame; fold inclusive-cycle delta into
       `func_stats[callee]` and `call_edges[(caller, callee)]`.
     - `JalTail` / `JalrIndirect` — pop+push: replace top frame
       (preserves caller, replaces callee).
     - Other variants — no shadow-stack change.

- **Tail-call and orphaned-return handling.**
  - Tail calls: pop-then-push semantics on shadow stack. Caller stays
    in attribution chain; new callee replaces the leaf.
  - Orphaned returns (return without matching call): silently no-op.
    Happens at top of stack at end of run, and theoretically with
    `setjmp`-style code (not used in current shader path).
  - Empty-stack cycle accumulation: tracked under a synthetic root
    `func_stats[0x0]` named `<root>` in reports.

- **`ProfileSession::on_instruction` dispatch.** Already declared as
  no-op default in m0's trait. m2 wires the dispatch through from the
  run loop. Dispatch signature carries `(pc, target_pc_for_branches,
  inst_class, cycles)` — the `target_pc_for_branches` lets
  `CpuCollector` know the callee PC for JAL/JALR without
  re-decoding.

- **Speedscope JSON writer.** In new file
  `lp-cli/src/commands/profile/output_speedscope.rs`. Writes
  `cpu-profile.speedscope.json` in Speedscope's "evented" format
  (https://github.com/jlfwong/speedscope/wiki/Importing-from-custom-sources).
  Conversion happens at finish from the callgrind data model: for
  each call edge, emit synthetic open/close events. PC → name
  resolved via static ELF symbols from `meta.json`. JIT'd code
  shows as `<jit:0x...>` placeholder until m5.

- **Canonical `cpu-profile.json` writer.** In new file
  `lp-cli/src/commands/profile/output_cpu_json.rs`. Writes the full
  callgrind data model:

  ```json
  {
    "schema_version": 1,
    "cycle_model": "esp32c6",
    "total_cycles_attributed": 12345678,
    "active_cycles": 11000000,
    "mode": "steady-render",
    "frames_captured": 4,
    "func_stats": {
      "0x80001234": {"name": "render::frame", "self_cycles": 100, "inclusive_cycles": 1000, "calls_in": 4, "calls_out": 12},
      ...
    },
    "call_edges": [
      {"caller": "0x80001234", "callee": "0x80001500", "count": 4, "inclusive_cycles": 800},
      ...
    ]
  }
  ```

  This is the diff source of truth for m3.

- **CPU report section.** `CpuCollector::report_section` writes
  top-20 by self_cycles + top-20 by inclusive_cycles. Format:

  ```
  CPU summary (mode=steady-render, cycles=11,000,000, frames=4)
  ----------------------------------------------------------------
  Top 20 by self cycles:
       2,184,512  19.4%  shader::rainbow::palette_warm
       1,832,004  16.3%  lpvm_native::rt_jit::dispatch
         944,221   8.4%  led_driver::push_pixel
         ...
  Top 20 by inclusive cycles:
       8,200,114  74.5%  render::frame
         ...
  ```

  PC → name via static ELF symbols. Unknown PCs format as
  `<unknown 0xADDR>`; JIT region as `<jit:0xADDR>`.

- **`--cycle-model` flag**:

  ```
  lp-cli profile [DIR] [--cycle-model {esp32c6,uniform}=esp32c6] ...
  ```

  Plumbing already exists in `lp-riscv-emu` (the `CycleModel` enum is
  there). `uniform` mode is the developer sanity check: every
  instruction = 1 cycle. If `uniform` and `esp32c6` produce wildly
  different hotspot rankings, someone's doing pathological things
  with DIV/atomics that the cost classes are exaggerating.

- **`--collect cpu` becomes the default** when `--collect` is omitted.
  Previously (m1) the default was `events`; now `cpu` is the primary
  use case. `events` is auto-included whenever any other collector is
  enabled (the gate needs them).

- **Trace dir contents** with `--collect cpu`:
  ```
  traces/<sess>/
    meta.json
    events.jsonl
    cpu-profile.json
    cpu-profile.speedscope.json
    report.txt
  ```

  With `--collect cpu,alloc`: also `heap-trace.jsonl`.

- **Tests.**
  - Unit test for `InstClass` decoder: each new variant has
    handcrafted instruction-word fixtures.
  - Unit test for `CpuCollector` with a hand-built sequence of
    `(pc, inst_class, target_pc, cycles)` events:
    - Simple call/return → expected `func_stats` + `call_edges`.
    - Nested calls (3 deep) → expected attribution.
    - Tail call → caller credit preserved.
    - Orphaned return → no panic, no-op.
  - Unit test for speedscope JSON writer: small fixture, output
    parses back as valid JSON, structure matches Speedscope's
    "evented" schema.
  - Unit test for canonical JSON writer: round-trips through
    serde, schema_version=1 present.
  - Integration test: `lp-cli profile examples/basic --collect cpu`
    produces all four expected files; `cpu-profile.json` parses;
    `report.txt` contains "CPU summary" section; total_cycles
    plausible (within order of magnitude of expected).
  - Integration test: `--collect cpu,alloc` produces both
    `cpu-profile.json` and `heap-trace.jsonl` in same trace dir.

### Out of scope

- `--diff [PATH]` and `lp-cli profile diff` impl — m3.
- `HardwarePerfSink` and device console parser — m4.
- JIT symbol overlay — m5. (JIT'd code is `<jit:0x...>` placeholder.)
- Per-event `arg: u32` payload — deferred.
- `--raw-events` opt-in mode — deferred.
- Refining `Esp32C6` cycle costs — m4 (data-driven).
- Folded-stack output, callgrind text format — explicitly dropped.
- Documentation home — m6.

## Key Decisions

- **Decode-time classification, not collector re-decode.** Per Q9: new
  `InstClass` variants are produced once at decode time. The collector
  switches on the variant and never re-inspects instruction words.
  Keeps the hot path tight; cycle-model concerns stay in the cycle
  model.

- **Shadow stack is host-side only.** Per Q1: independent of the
  guest's actual stack. This is what makes JIT'd code attributable
  even without frame pointers.

- **Tail call = pop+push, not just push.** Preserves caller credit
  (semantically a tail call replaces the current frame, not adds a
  new one). Matches Valgrind's callgrind handling.

- **`<jit:0x...>` placeholder is acceptable for m2.** The flame chart
  still shows that 80% of cycles are in "the JIT region" — actionable.
  Per-shader-function names land in m5.

- **Disabled-cost is a single branch.** `if !self.active { return; }`
  in `CpuCollector::on_instruction`. Even when the collector is
  attached, gate-disabled cost is one well-predicted branch.

- **`uniform` cycle model is a debugging affordance**, not a default.
  Cheap to expose because the `CycleModel` enum already has it.

- **CPU is the default `--collect`**, not events. When users run `lp-cli
  profile`, they want a flame chart; events are always written
  alongside.

## Deliverables

### `lp-riscv-emu` crate
- New: `lp-riscv-emu/src/profile/cpu.rs` — `CpuCollector`.
- Updated: `lp-riscv-emu/src/emu/cycle_model.rs` — extended
  `InstClass`, updated `CycleModel::Esp32C6` cost lookup.
- Updated: instruction decoder — emit new `InstClass` variants based
  on `rd` / `rs1` inspection for JAL/JALR. Compressed-instruction
  paths updated.
- Updated: `lp-riscv-emu/src/emu/emulator/run_loops.rs` — dispatch
  `(pc, target_pc, inst_class, cycles)` to
  `profile_session.on_instruction(...)`.
- Updated: `lp-riscv-emu/src/profile/mod.rs` — `Collector::on_instruction`
  signature finalized; `ProfileSession::dispatch_instruction` helper
  fans out to enabled collectors.
- Updated: `lp-riscv-emu/src/lib.rs` — export `CpuCollector`.

### `lp-cli` crate
- New: `lp-cli/src/commands/profile/output_speedscope.rs` —
  callgrind → Speedscope evented JSON.
- New: `lp-cli/src/commands/profile/output_cpu_json.rs` — canonical
  callgrind JSON.
- New: `lp-cli/src/commands/profile/symbolize.rs` — PC → name
  resolution from `meta.json`'s symbol list. (Pure function; m5 will
  extend with dynamic-symbol overlay.)
- Updated: `lp-cli/src/commands/profile/args.rs` — `--cycle-model`
  flag; `--collect` default is `cpu`; `events` auto-added when any
  collector enabled.
- Updated: `lp-cli/src/commands/profile/handler.rs` — instantiates
  `CpuCollector`; passes cycle model selection to emulator builder;
  invokes output writers at finish.

### Tests
- Unit tests in `lp-riscv-emu/src/emu/cycle_model.rs` for new
  decoder variants (handcrafted instruction-word fixtures).
- Unit tests in `lp-riscv-emu/src/profile/cpu.rs` for shadow-stack
  scenarios.
- Unit tests in `lp-cli/src/commands/profile/output_*.rs` for output
  writers.
- Integration test in `lp-cli/tests/` for end-to-end `profile
  --collect cpu` against `examples/basic`.

## Dependencies

- m0 — Foundation refactor.
- m1 — Perf-event system + ProfileMode (CpuCollector consumes the
  gate; dispatch infrastructure in `ProfileSession` already exists).

## Validation

```bash
# Workspace builds
cargo build --workspace

# Unit tests
cargo test -p lp-riscv-emu
cargo test -p lp-cli

# Existing emulator semantics unchanged
cargo test -p lp-riscv-emu  # cycle counts unchanged for m0/m1 fixtures

# fw-esp32 still builds
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf \
  --profile release-esp32 --features esp32c6,server

# End-to-end: produce a CPU profile of examples/basic
cargo run -p lp-cli --release -- profile examples/basic
# Default --collect cpu --mode steady-render
# Expected outputs in traces/<sess>/:
#   meta.json (with cycle_model="esp32c6", clock_source="emu_estimated")
#   events.jsonl
#   cpu-profile.json (with schema_version=1, populated func_stats)
#   cpu-profile.speedscope.json
#   report.txt (with "CPU summary" section, top-20 by self+inclusive)

# Visual sanity check
# 1. Open cpu-profile.speedscope.json at https://speedscope.app
# 2. Inspect flame chart: JIT region present (as <jit:0x...>),
#    engine Rust functions named, no obviously-broken stacks.

# Sanity-check the cycle model
cargo run -p lp-cli --release -- profile examples/basic \
  --cycle-model uniform
# Compare top-20 with esp32c6 run. Major rank changes indicate
# DIV/atomics-heavy code being dramatically over- or under-cost.

# Composability check
cargo run -p lp-cli --release -- profile examples/basic \
  --collect cpu,alloc
# Both cpu-profile.json and heap-trace.jsonl in same trace dir.
# report.txt has both "CPU summary" and "Heap summary" sections.

# Mode behavior check
cargo run -p lp-cli --release -- profile examples/basic --mode compile
# CPU profile shows shader-compile work only; no frame work.
```

## Estimated Scope

- New code: ~1200-1500 LOC.
  - `CpuCollector`: ~400-500.
  - `InstClass` decoder updates: ~100-150.
  - Speedscope writer: ~150-200.
  - Canonical JSON writer: ~100-150.
  - Symbolize/report: ~150.
  - Hot-path dispatch wiring: ~50.
  - CLI args / handler updates: ~150.
- Tests: ~400-600 LOC.
- Files touched: ~15-20.

## Agent Execution Notes

Implementation order:

1. Read `lp-riscv-emu/src/emu/cycle_model.rs` and the decode site to
   understand current `InstClass` production.
2. Extend `InstClass` with new variants. Update decode site (uncompressed
   + compressed). Update `CycleModel::Esp32C6` cost mapping.
3. Run existing `lp-riscv-emu` tests — should pass unchanged
   (cost numbers identical).
4. Implement `CpuCollector` shadow-stack maintenance pure-functionally.
   Extensive unit tests with fixture event sequences before wiring to
   the run loop.
5. Update `Collector::on_instruction` signature to include
   `target_pc` (needed for call attribution). Wire dispatch in
   `run_inner_fast` and `run_inner_logging`.
6. Run small synthetic test: emulate a tiny binary with known call
   structure, assert `CpuCollector::func_stats` matches expectation.
7. Implement symbolizer (PC → name) using static ELF symbols.
8. Implement canonical JSON writer. Round-trip test.
9. Implement speedscope writer. Validate output against
   Speedscope's JSON schema, then visually at speedscope.app.
10. Implement CPU report section formatter.
11. Wire all of the above into `lp-cli/src/commands/profile/handler.rs`.
12. End-to-end test against `examples/basic`. Inspect flame chart for
    plausibility.

## Post-Landing: Measured Overhead & Follow-up Work

### Measured per-instruction overhead (2026-04-19)

Discovered while investigating slow integration tests on `examples/basic
--mode startup`:

| Run                          | Wall time      | Notes                          |
| ---                          | ---            | ---                            |
| `--collect alloc` (m1)       | ~176s, full    | Pre-m2 baseline.               |
| `--collect cpu` (m2)         | >16min, killed | Reached the same ~90M-cycle progress checkpoint as alloc. |

The cpu run was killed in-flight, so the lower bound on slowdown is
**~5-6×** versus the alloc baseline — the true ratio is likely larger.
Same workload, same `--mode startup` shutdown path, only the collector
differs.

### Where the overhead comes from

For every executed RISC-V instruction with `--collect cpu`:

1. `Riscv32Emulator::after_execute` performs an `Option::as_mut()` check
   on `profile_session` and computes `target_pc` from
   `ExecutionResult::new_pc`/`inst_size`.
2. Virtual call into `ProfileSession::dispatch_instruction`, then
   `Vec` iter + dyn-trait call into each `Collector::on_instruction`.
3. `CpuCollector::on_instruction_inner` performs **a `HashMap<u32, _>`
   lookup** (`SipHash`) to bump `func_stats[stat_pc].self_cycles`.
   This is the dominant cost — ~30-50ns per instruction on its own.

The HashMap-per-instruction lookup is the primary hotspot. Per-class
`match` and shadow-stack push/pop on call/return are negligible by
comparison.

### Important: events- and alloc-only runs also regressed slightly

Even when only `events` or `alloc` collectors are registered,
`dispatch_instruction` is now called per instruction. Their default
`on_instruction` is a no-op, but the `Option` check, `target_pc`
compute, vec iter, and per-collector dyn-call are still paid. Spot-check
`profile_events_steady_render_smoke` runtime if regressions matter for
m1 use cases.

When **no** collector is enabled, `profile_session` is `None` and
`after_execute` short-circuits at the `Option` check — zero overhead.

### Acceptable for now

- Callgrind itself slows programs by 50-100×; a ~5-10× cost for full
  per-PC attribution is in line with prior art.
- Default `--mode startup` workloads complete inside a single test
  timeout window; CI cost is finite and manageable.
- The per-instruction work is the same architecture every cycle-level
  profiler uses (per-PC accumulator behind a per-instruction dispatch).

### Follow-up optimization ideas (defer until measured pain)

The natural place to revisit these is when adding the second wave of
collectors (already noted under "Run loop tech debt" in
[overview.md → Risks](./overview.md#risks)) or if cpu profile turn-around
becomes a developer-experience problem.

1. **Replace `HashMap<u32, FuncStats>` with `Vec<FuncStats>` indexed by
   symbol-id.** Resolve PC → symbol-id once on `push_frame` (or lazily
   per-PC with a small LRU), then use array indexing on the hot path.
   Estimated 3-5× speedup on the cpu hot path; this is the single
   biggest win.
2. **Hoist a `cpu_active: bool` (or per-collector mask) onto
   `ProfileSession`** so events-only / alloc-only modes skip the inner
   `dispatch_instruction` loop entirely. Restores the m1 baseline for
   non-cpu collectors.
3. **Specialize the run loop** with a dedicated `run_with_profile`
   variant so the `Option` check happens once per loop iteration, not
   per instruction. Aligns with the JIT trampoline pattern already in
   use for the fast/logging split.
4. **Bucket call-site PCs to function-entry PCs cheaper than `Symbolizer`
   does today.** `PcSymbolizer::entry_lo_for_pc` is currently a
   per-instruction call when reporting; this could be a once-per-frame
   computation if symbol ranges are precomputed into a sorted `Vec`.

None of these are blockers for m3 (diff) or m4 (hardware correlation).
Reassess once we have the second-wave collectors (`cpu-log`, `syscalls`,
`ir-stats`) on the table or once `examples/basic --collect cpu`
turn-around becomes a dev-loop bottleneck.

### Test-infrastructure follow-up (action item)

The current end-to-end coverage in `lp-cli/tests/profile_cpu_smoke.rs`
boots the **full fw-emu stack** (cargo build + JIT compile + project
load + first-frame render + clean shutdown) three times in series.
Combined with the m2 per-instruction overhead above, each test takes
several minutes; the file as a whole gates the lp-cli test suite at
~10-15 min. This is the wrong shape for mainline CI.

**Decision (2026-04-19):** mainline tests for the cpu collector must
**not** boot fw-emu. Specialized end-to-end tests can exist, but they
must be isolated (e.g. `#[ignore]`-gated, run one at a time on demand
or in a separate nightly job).

What to do instead — exact prior art exists in
`lp-riscv-emu/tests/abi_tests.rs`, `stack_args_tests.rs`,
`guest_app_tests.rs`, `trap_tests.rs`:

- Compile a **tiny synthetic RV32 program** with cranelift (the helpers
  in `abi_tests.rs::compile_function` are already the template) with a
  known call structure (e.g. `main → foo → bar → ret`, plus a tail
  call and a recursion case).
- Run through `Riscv32Emulator` directly, with a `ProfileSession`
  carrying a `CpuCollector` (force-enabled or driven by a synthetic
  `profile:start` event — no `--mode startup` machinery needed).
- Assert against `CpuCollector` internals: `func_stats[foo_pc]`,
  `call_edges[(main_pc, foo_pc)]`, `inclusive_cycles` arithmetic.
  These tests run in milliseconds.

What stays as `#[ignore]`-gated end-to-end:

- The current `profile_cpu_smoke.rs` trio (`profile_cpu_default_smoke`,
  `profile_cpu_uniform_model`, `profile_cpu_with_alloc`) — useful for
  pre-release validation that the full stack still produces a parseable
  trace dir, but not for every `cargo test`. Mark `#[ignore]` and
  document the `cargo test -- --ignored --test-threads=1` invocation
  near the test module.

Out of scope for the m2 follow-up but worth flagging:

- The same "don't boot fw-emu in mainline tests" principle should also
  apply to **events** and **alloc** smoke tests
  (`profile_alloc_smoke`, `profile_events_steady_render_smoke`). They
  predate this decision and were tolerable at one collector each, but
  they're now ~3-min each in serial. Consider porting them to the
  small-program harness too as part of m6 cleanup.
