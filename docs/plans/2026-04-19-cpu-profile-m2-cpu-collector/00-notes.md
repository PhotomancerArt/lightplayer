# CPU Profile m2 — CPU collector + outputs — Notes

## Scope

Implement Milestone 2 (`docs/roadmaps/2026-04-19-cpu-profile/m2-cpu-collector.md`):
the per-function cycle attribution layer on top of m1's gate/event substrate.

Standalone deliverable:
`lp-cli profile examples/basic` (default `--collect cpu --mode steady-render`)
produces a callgrind-style flame chart of the steady-state render — the user's
"I changed X. Where did the cycles go?" question becomes answerable.

Concretely m2 ships:

- **`InstClass` extension** in `lp-riscv-emu/src/emu/cycle_model.rs`: replace
  the existing `Jal`/`Jalr` variants with five call/return-aware variants
  produced at decode time:
  - `JalCall` (`JAL rd, _` with `rd != x0`)
  - `JalTail` (`JAL x0, _`)
  - `JalrCall` (`JALR rd, _, _` with `rd != x0`)
  - `JalrReturn` (canonical `ret`: `JALR x0, x1, 0`; compressed `c.jr ra`)
  - `JalrIndirect` (other `JALR x0, _, _` patterns)

  Cost mapping in `CycleModel::Esp32C6` keeps existing numbers for all five
  (Jal=2 → JalCall/JalTail=2; Jalr=3 → JalrCall/JalrReturn/JalrIndirect=3) —
  no microarchitectural differentiation.

- **Decode-site update** in `lp-riscv-emu/src/emu/executor/jump.rs` and
  `executor/compressed.rs`: inspect `rd`/`rs1` for JAL/JALR (and the
  compressed equivalents `c.j`, `c.jal`, `c.jr`, `c.jalr`) to pick the
  right new variant. Existing semantics unchanged — only the classifier
  output changes.

- **`Collector::on_instruction` signature widening** in
  `lp-riscv-emu/src/profile/mod.rs`: add `target_pc: u32` so the
  CpuCollector can attribute calls without re-decoding. Passed through
  from the run loop's `ExecutionResult` (which already computes the
  target PC for jumps via `new_pc`).

- **Per-instruction dispatch hook** in `lp-riscv-emu/src/emu/emulator/run_loops.rs`:
  after `self.cycle_count += inst_cost`, call
  `profile_session.dispatch_instruction(pc, target_pc, class, cost)`.
  No-op when no session present; constant per-instruction overhead in
  the worst case (one inlined `Option<&mut>` check + one call).

- **`CpuCollector`** in new file `lp-riscv-emu/src/profile/cpu.rs`.
  Internal callgrind-style data model:

  ```rust
  pub struct CpuCollector {
      shadow_stack: Vec<Frame>,
      func_stats:   HashMap<u32, FuncStats>,
      call_edges:   HashMap<(u32, u32), CallEdge>,
      active:       bool,
      total_cycles_attributed: u64,
      cycle_model_label: &'static str,
  }
  struct Frame    { callee_pc: u32, caller_pc: u32, cycles_at_entry: u64, self_cycles_at_entry: u64 }
  struct FuncStats{ self_cycles: u64, inclusive_cycles: u64, calls_in: u64, calls_out: u64 }
  struct CallEdge { count: u64, inclusive_cycles: u64 }
  ```

  Attribution rules (per roadmap):
  - Self cycles: bump top-of-stack PC by `inst_cost` (orphan/root if no
    stack — credited under synthetic `0x0` "<root>").
  - `JalCall`/`JalrCall` → push frame.
  - `JalrReturn` → pop top frame; fold inclusive cycles into
    `func_stats[callee]` and `call_edges[(caller, callee)]`.
  - `JalTail`/`JalrIndirect` → pop+push (replace top frame).
  - Other variants → no shadow-stack change.

- **Gate→active wiring**. `CpuCollector` honors `Enable`/`Disable` from
  the `ProfileMode` gate (m1's `GateAction` carries them; m1 collectors
  ignore). Implementation: `CpuCollector` either reads gate state from a
  shared cell, or m2 extends `Collector::on_perf_event` so each collector
  sees the gate transitions and flips its own `active: bool`. Decision
  in Q-block below.

- **Speedscope JSON writer** in new file
  `lp-cli/src/commands/profile/output_speedscope.rs`. Writes
  `cpu-profile.speedscope.json` in Speedscope's "evented" format. PC →
  name resolved via static ELF symbols from `meta.json`. JIT'd code
  shows as `<jit:0xADDR>` placeholder.

- **Canonical `cpu-profile.json` writer** in new file
  `lp-cli/src/commands/profile/output_cpu_json.rs`. Schema-versioned;
  m3's diff source of truth. Shape per roadmap.

- **PC → name symbolizer** in new file
  `lp-cli/src/commands/profile/symbolize.rs`. Pure function
  `(pc, &[TraceSymbol]) -> Cow<'_, str>` that returns either the static
  ELF symbol name (binary search) or `<jit:0xADDR>` for unknown PCs in
  the JIT region or `<unknown 0xADDR>` otherwise. m5 will extend with
  dynamic-symbol overlay; m2 ships the static path.

- **CPU report section** — appended to `report.txt` by
  `CpuCollector::report_section`. Top-20 by self_cycles + top-20 by
  inclusive_cycles. Format per roadmap.

- **`--cycle-model` flag** in `lp-cli/src/commands/profile/args.rs`:
  ```
  --cycle-model {esp32c6,uniform}=esp32c6
  ```
  Mapped to existing `CycleModel` enum (`Esp32C6` / `InstructionCount`).
  Plumbed through `Riscv32Emulator::with_cycle_model`/`set_cycle_model`
  during emulator construction.

- **`--collect cpu` becomes the default** when `--collect` is omitted.
  Previously (m1) the default flipped from `alloc` → `events`; m2 flips
  it again from `events` → `cpu`. `events` is auto-included whenever any
  collector is enabled (the gate needs them — but per m1 Q5 the gate is
  internal and "events" controls only on-disk persistence; m2 keeps that
  invariant).

- **CPU collector wiring in `lp-cli`** — handler.rs validates `cpu` as a
  collector name, instantiates `CpuCollector::new()`, plumbs the cycle
  model, and at finish calls the speedscope + canonical JSON writers
  (both pure functions reading from `CpuCollector`'s data model).

- **Tests**:
  - Unit tests for new `InstClass` decoder variants (handcrafted
    instruction-word fixtures for each of the five new variants, plus
    compressed-instruction equivalents).
  - Unit tests for `CpuCollector::on_instruction` over hand-built event
    sequences: simple call/return, nested 3-deep, tail call, orphaned
    return, root cycles, gate disable.
  - Unit test for speedscope JSON writer: small fixture, output parses
    back as valid JSON, structure matches Speedscope's "evented" schema.
  - Unit test for canonical JSON writer: round-trip through serde,
    `schema_version: 1` present.
  - Unit test for symbolizer: hits, JIT-region miss, unknown miss.
  - Integration test: `lp-cli profile examples/basic --collect cpu`
    produces all four expected files; `cpu-profile.json` parses;
    `report.txt` contains "CPU summary" section; `total_cycles`
    plausible (within order of magnitude of expected).
  - Integration test: `--collect cpu,alloc` produces both
    `cpu-profile.json` and `heap-trace.jsonl`.

Out of scope (downstream milestones / explicitly dropped in roadmap):

- `--diff [PATH]` and `lp-cli profile diff` impl — m3.
- `HardwarePerfSink` and device console parser — m4.
- JIT symbol overlay — m5. (JIT'd code is `<jit:0xADDR>` placeholder.)
- Per-event `arg: u32` payload — deferred (ABI room reserved in m1).
- `--raw-events` opt-in — deferred.
- Refining `Esp32C6` cycle costs — m4 (data-driven).
- Folded-stack output, callgrind text format — explicitly dropped per
  roadmap.
- Documentation home — m6.

## Current state

### What m0 + m1 (in progress) leave in place

- `lp-riscv-emu/src/profile/mod.rs` — `Collector` trait, `ProfileSession`,
  `EmuCtx`, `FinishCtx`, `SessionMetadata`, `SyscallAction`,
  `HaltReason`. m1 adds `Gate` trait + `GateAction { Enable, Disable,
  NoChange, Stop }`, `set_gate`, `on_perf_event` dispatch,
  `take_halt_reason`/`pending_halt_reason`. m1 also fills in
  `PerfEvent` (cycle, name, kind) — but `pub struct InstClass {}` stays
  as a stub (m2 fills in).

- `Collector` trait declares `on_instruction(_pc: u32, _kind: InstClass,
  _cycles: u32)` (default no-op) — but the m1 plan does NOT add a
  hot-path dispatch site for it. Only `on_syscall` and `on_perf_event`
  are wired through the run loop in m1. m2 owns the per-instruction
  dispatch path entirely.

- `lp-base/lp-perf` (m1) — workspace-wide tracing macros with cfg-gated
  sinks. m2 doesn't touch this crate; it just uses the `EVENT_*`
  constants from the `KNOWN_EVENT_NAMES` set already declared in
  `lp-riscv-emu/src/profile/perf_event.rs` (m1).

- `lp-cli/src/commands/profile/` after m1:
  - `args.rs` — `--collect` (default `events`), `--mode`, `--max-cycles`,
    `--note`. `--frames` removed.
  - `handler.rs` — slim orchestrator (~80–120 LOC).
  - `workload.rs` — frame-driving loop with `run_until_yield_or_stop`.
  - `output.rs` — `meta.json` + `report.txt` writers.
  - `mode/` — `ProfileMode` + per-mode `Gate` impls.
  - `diff_stub.rs` — unchanged.

- `Riscv32Emulator` carries `Option<ProfileSession>` with
  `with_profile_session(...)` and `finish_profile_session()`. m1 adds
  `set_profile_gate(...)` accessor. `cycle_count` field accessor exists
  (`emu.cycle_count()`).

- Trace dir layout (m1):
  `profiles/<timestamp>--<workload>--<mode>[--<note>]/` containing
  `meta.json`, `events.jsonl` (when `events` collector ran),
  `heap-trace.jsonl` (when `alloc` collector ran), `report.txt`.

- Cargo features: `profile` exists in `fw-emu` and `lp-riscv-emu-guest`
  (renamed in m0 from `alloc-trace`). No new feature in m2 — `cpu` is
  host-side only; the guest doesn't change.

### Surfaces relevant to m2

- **Decode site for JAL / JALR**:
  - `lp-riscv-emu/src/emu/executor/jump.rs::decode_execute_jal` (sets
    `class: InstClass::Jal`).
  - `lp-riscv-emu/src/emu/executor/jump.rs::decode_execute_jalr` (sets
    `class: InstClass::Jalr`).
  - `lp-riscv-emu/src/emu/executor/compressed.rs` — `execute_c_jal`,
    `execute_c_j`, `execute_c_jr`, `execute_c_jalr` all set
    `class: InstClass::Jal` or `Jalr`. m2 updates each.

- **Hot-path dispatch site**: `lp-riscv-emu/src/emu/emulator/run_loops.rs::run_inner_fast`
  and `run_inner_logging`. Both have an identical sequence:
  ```rust
  self.cycle_count += self.cycle_model.cycles_for(exec_result.class) as u64;
  // [m2] insert dispatch_instruction here
  ```
  `ExecutionResult.new_pc: Option<u32>` already carries the target PC
  for JAL/JALR — m2 pipes it directly into `target_pc`.

- **Existing `CycleModel` enum** (`InstructionCount`, `Esp32C6` —
  default). The roadmap calls `InstructionCount` "uniform". The
  `--cycle-model` CLI flag is a thin mapping.

- **Symbol source**: ELF symbols loaded via `lp-riscv-elf::load_elf` →
  `LoadInfo.symbol_list: Vec<{name, addr, size}>`. Already serialized
  into `meta.json` under `symbols` (m0). m2's symbolize module either
  reads from `meta.json` post-finish or holds a clone in memory during
  the run.

- **JIT region**: `lpvm-native` allocates rv32 instruction bytes on the
  guest heap and jumps to them via `rv32_jalr_a0_a7`. From the
  emulator's POV these are PCs in RAM (≥ `0x8000_0000`), not in the
  static code region. Symbolize treats any PC in the RAM range that
  isn't a known stack-frame return-address pattern as JIT; `<jit:0xADDR>`
  is the placeholder.

### Cargo deps to confirm

- `lp-riscv-emu` already depends on `serde_json` (via `alloc.rs`). No
  new deps required for `cpu.rs`.
- `lp-cli` already depends on `serde_json` and `chrono`. Speedscope JSON
  writer and canonical JSON writer use `serde_json::json!`. No new deps.

## Open questions to resolve

Each question presented one at a time in chat. Resolutions land back in
"Resolved questions" above with answer + rationale.

### Q1: Where does `CpuCollector` learn the gate's `active` state?

Roadmap says: `if !self.active { return; }` is the disabled-cost branch.
m1 plumbs `GateAction::Enable`/`Disable` through `on_perf_event` and
explicitly *no-ops them on collectors* in m1, deferring "real gating" to
m2 (m1 Q6).

Three options:
- **(a)** Each collector grows its own `active: bool` and listens to
  `GateAction` in its own `on_perf_event`. To do this, the collector
  needs to *see the gate's decision*, not just the raw event. Options:
  (a1) `ProfileSession` calls a new `Collector::on_gate_action(&mut
  self, GateAction)` after running the gate; (a2) `ProfileSession`
  exposes a shared `Cell<bool>` / atomic that collectors read.
- **(b)** `ProfileSession` itself carries a single `enabled: bool`
  state machine and sets a flag the dispatch path consults — bypasses
  per-collector dispatch entirely when disabled.
- **(c)** `CpuCollector` inspects `PerfEvent`s directly and replicates
  the mode's logic — rejected (couples collector to mode semantics).

**Suggested**: option **(a1)** — extend `Collector` with
`fn on_gate_action(&mut self, _action: GateAction) {}` (default no-op).
`ProfileSession::on_perf_event` runs the gate, then calls
`on_gate_action` on each collector with the action. `CpuCollector`
overrides to flip its `active: bool`. Cost: one extra trait method;
zero runtime cost when collectors don't override. Keeps every collector
in charge of its own gating policy (e.g., `AllocCollector` may want to
keep its m0 "always record" behavior; `CpuCollector` honors the gate).
Symmetric with the existing per-collector pattern.

Alternative (b) is more centralized but conflates "is the session
running" with "is the cpu collector capturing samples" — they'll
diverge once we have `cpu-log` collectors etc. that may want to be
gated independently.

### Q2: How does `CpuCollector` know the cycle model label for `meta.json`?

`cpu-profile.json` and `meta.json` should carry the active cycle model
("esp32c6" or "uniform") so diffs / reports can verify both runs used
the same cost assumptions.

Three options:
- **(a)** Add a `cycle_model: String` field to `SessionMetadata` (m1
  shape) and have the CLI populate it from `--cycle-model`. The
  `CpuCollector` doesn't need to know it directly — `meta.json` is the
  source of truth, and the canonical JSON writer reads it from
  `meta.json` at finish.
- **(b)** Pass the label into `CpuCollector::new(label: &'static str)`.
  Stored in the collector's data model and emitted into the canonical
  JSON.
- **(c)** Both — `meta.json` carries it; `CpuCollector` carries it too
  (so the report section can show it without re-reading `meta.json`).

**Suggested**: option **(c)** — bake into both. `meta.json` carries it
for tooling; `CpuCollector::new(label)` carries it for the report
section's "CPU summary (cycle_model=esp32c6, …)" banner. Cost is one
`&'static str` field per collector. The label is `"esp32c6"` or
`"uniform"` — a `match` in the CLI handler maps from `CycleModel`
enum.

(Add `cycle_model: String` to `SessionMetadata` as a m2 field-add.
Schema version stays 1 per m1 Q9 policy — nothing real consumes
`meta.json` yet.)

### Q3: Where does the "static ELF symbols" sourcing live for symbolize?

Two options:
- **(a)** Symbolize reads the symbol list from `meta.json` after the
  session finishes. Pros: single source of truth on disk; m5 just
  appends overlay symbols to a separate data structure. Cons: requires
  meta.json to be finalized before any output writer runs.
- **(b)** The handler holds the symbol list in memory and passes it
  into the output writers directly (parallel to the `meta.json` write).

**Suggested**: option **(b)** for m2 — simpler control flow, no
read-back from disk. The output writers take `&[TraceSymbol]` slices
from the same source the handler used to build `SessionMetadata`. m5
will add a `&[DynamicSymbol]` overlay alongside the static slice.

### Q4: PC → name resolution: by `addr` only, or by `[addr, addr+size)` interval?

Roadmap doesn't specify, but `TraceSymbol` carries `size`. Two options:
- **(a)** **Interval** lookup — sort symbols by `addr`, binary-search
  for the largest `addr ≤ pc`, return name if `pc < addr + size` else
  unknown. Standard symbolizer behavior; correctly attributes
  mid-function PCs to the function (e.g. PCs in the prologue, after a
  call return).
- **(b)** **Exact** lookup — return name only if `pc == addr` of some
  symbol; everything else is unknown. Useless for practical
  attribution because shadow-stack callee PCs are call targets but
  most "self" PCs are mid-function.

**Suggested**: option **(a)**, interval lookup. Build a sorted
`Vec<(addr, addr+size, name)>` once at startup; binary search per
lookup. O(log N) per PC. `CpuCollector::on_instruction` runs at every
instruction, but it only stores raw PCs — symbolization happens at
finish, once per unique PC in `func_stats` / `call_edges`. The
symbolize cost is bounded by the size of the stats hash, not by
total instructions executed.

### Q5: Where does the JIT region boundary live, and how is it detected?

The roadmap says JIT'd code symbolizes as `<jit:0xADDR>`. To pick that
classification we need to identify "this PC is in the JIT region".
Options:
- **(a)** Any PC ≥ `RAM_START` (`0x8000_0000`) that didn't hit a static
  ELF symbol → `<jit:0xADDR>`. Any PC outside RAM that didn't hit →
  `<unknown 0xADDR>`. Simple; no extra metadata needed.
- **(b)** Track the JIT base/size via syscall (m5's
  `SYSCALL_JIT_MAP_LOAD`); m2 leaves the JIT region "unknown" because
  m5 hasn't shipped. Means m2's flame chart is mostly `<unknown 0x…>`.
- **(c)** Hardcode a "JIT region" range in the symbolizer based on
  inspection of where lpvm-native allocates JIT'd code today.
  Brittle — JIT lives on the heap, no fixed range.

**Suggested**: option **(a)**. Any PC in RAM with no static-ELF match is
JIT (the only way the guest reaches a RAM PC is via `JALR` into a
JIT'd module — RAM is otherwise data). Cheap, no extra metadata, no
m5 dependency. m5 will overlay specific JIT-symbol names on top
without changing this default.

### Q6: How big should we expect `func_stats` and `call_edges` to grow?

Affects choice of map type and whether we need to think about memory
bounds.

`func_stats` is keyed by callee PC (call target) — bounded by # of
distinct functions called during the captured window. For
`examples/basic`'s steady-render: low thousands at most (engine + JIT'd
shader entries).

`call_edges` is keyed by `(caller_pc, callee_pc)` — bounded above by
distinct call sites. Could be larger than `func_stats` but still small
in absolute terms (low tens of thousands worst case).

Both are populated only on call/return events, not per-instruction —
total events for a 4-frame steady-render capture is on the order of
millions of cycles ÷ tens of cycles per call = ~100k events.

**Suggested**: standard `std::collections::HashMap`. No need for FxHash
or capacity hints in m2. Revisit if profiling overhead becomes an
issue (would land in a perf milestone, not in m2).

### Q7: Inside `CpuCollector::on_instruction`, what's the per-call dispatch shape?

Per the roadmap sketch, the dispatch is a `match inst_class { … }` that
pushes/pops the shadow stack. But every instruction also does
"bump self-cycles for top of stack." That's the part on the *true* hot
path (every instruction; only call/return are sparse).

Two shapes:
- **(a)** Single function:
  ```rust
  fn on_instruction(&mut self, pc: u32, target_pc: u32, class: InstClass, cycles: u32) {
      if !self.active { return; }
      // Bump self cycles
      let frame = self.shadow_stack.last_mut();
      let stat_pc = frame.map(|f| f.callee_pc).unwrap_or(0); // root
      self.func_stats.entry(stat_pc).or_default().self_cycles += cycles as u64;
      self.total_cycles_attributed += cycles as u64;
      // Stack maintenance
      match class {
          InstClass::JalCall | InstClass::JalrCall => self.push_frame(pc, target_pc, cycles),
          InstClass::JalrReturn                    => self.pop_frame(cycles),
          InstClass::JalTail | InstClass::JalrIndirect => self.replace_top_frame(pc, target_pc, cycles),
          _ => {}
      }
  }
  ```
- **(b)** Split: a fast "hot" function for the common case (no
  call/return) inlined into the dispatch, and a slow path for the
  call/return cases. Slightly faster but more code; not necessary
  unless profiling shows it matters.

**Suggested**: option **(a)** — single function, single match. Modern
branch predictors handle the unlikely arms cheaply. Profile in a future
perf milestone if it shows up.

### Q8: Do we need a `cpu` cargo feature on `lp-riscv-emu`?

m0 introduced a `profile` feature on `fw-emu` and `lp-riscv-emu-guest`
(guest-side allocator hooks). The host-side `lp-riscv-emu` doesn't
gate on a feature for `AllocCollector`. The new `CpuCollector` is also
host-side only — it processes data the run loop already produces.

**Suggested**: no new cargo feature. `CpuCollector` lives behind
`#[cfg(feature = "std")]` like `AllocCollector` (in
`lp-riscv-emu/src/profile/cpu.rs`); always compiled in when std is
available; opt-in at runtime via `--collect cpu`.

The per-instruction dispatch hook is also unconditional — it's a single
`Option<&mut ProfileSession>` check in the hot loop, identical cost to
the existing `Option<&mut Memory>` style fields. If the overhead ever
matters, it's a perf milestone problem.

### Q9: Should `dispatch_instruction` be on `Collector` (per-collector) or on `ProfileSession` (single fan-out point)?

m1 has `Collector::on_perf_event` (default no-op) called by
`ProfileSession::on_perf_event` for every collector. m0 declares
`Collector::on_instruction` (default no-op) but never calls it.

Two shapes:
- **(a)** `ProfileSession` adds a method `dispatch_instruction(pc,
  target_pc, class, cycles)` that loops over collectors and calls
  `Collector::on_instruction` on each. Symmetric with `on_perf_event`.
- **(b)** Skip the indirection — `ProfileSession` holds a direct
  `Option<CpuCollector>` field (because there's only ever one), and
  the run-loop call site checks it directly.

**Suggested**: option **(a)** — keep the trait fan-out shape. Cost is
one virtual call per collector per instruction. With a single
`CpuCollector` enabled, that's one `&mut dyn Collector` indirect call
per instruction — measurable but small (maybe 1–2 ns; the run loop is
dominated by decode + execute). And it preserves the architecture for
future per-instruction collectors (`cpu-log`, `ir-stats`).

Alternative (b) saves the virtual call but adds a special-case field;
if the overhead ever shows up in benchmarks, m6 can refactor toward
(b) cleanly because the trait method already documents the contract.

### Q10: Default `--collect` flips from `events` (m1) to `cpu` (m2). What's the composition rule?

Per roadmap: `--collect cpu` becomes the default. `events` is
auto-included whenever any other collector is enabled (the gate needs
them — but per m1 Q5 the gate is internal and always runs; "events"
controls only whether `events.jsonl` lands on disk).

Two interpretations:
- **(a)** Default `--collect cpu` writes `cpu-profile.json`,
  `cpu-profile.speedscope.json`, `report.txt` — but **not**
  `events.jsonl`. Roadmap text "events is auto-included whenever any
  other collector is enabled" means the gate runs internally; the file
  is not written unless `events` is in `--collect` explicitly.
- **(b)** Default `--collect cpu` *also* writes `events.jsonl` for
  free, because cpu-profile users will always want the timeline
  alongside their flame chart.

**Suggested**: option **(b)** — diverge from m1's "explicit selection
for output" rule for the cpu collector specifically. The events
timeline is always useful when looking at a flame chart (it tells you
where the warmup ended, where each frame started). One-line policy
flip in handler.rs: if `cpu` in `--collect`, also enable `events`. If
the user wants to opt out, they can pass `--collect cpu` with an
explicit `--no-events` flag — but that's a deferred polish; for m2,
auto-include events when cpu is enabled.

(Alternatively: if user explicitly says `--collect cpu` only, write
events.jsonl anyway; if they say `--collect alloc` only, don't. The
asymmetry mirrors the asymmetry in audience expectations — cpu users
want a timeline; alloc users have a different mental model.)

### Q11: What does `report.txt` look like when both `cpu` and `alloc` collectors run?

m0 + m1 establishes one `=== <Title> ===` banner per collector,
sequential. m2's `CpuCollector::report_section` emits the top-N
table.

**Suggested**: keep the m1 shape — collectors append in registration
order. Default `--collect cpu` produces:

```
=== Perf Events ===
events written: 42
path: events.jsonl

=== CPU summary ===
mode=steady-render, cycles=11,000,000, frames=4, cycle_model=esp32c6
Top 20 by self cycles:
     ...
Top 20 by inclusive cycles:
     ...
```

`--collect cpu,alloc` adds the alloc banner at the end. No special
ordering logic; collector order in `--collect` determines display
order.

### Q12: How does the canonical `cpu-profile.json` format hex addresses?

Roadmap shows `"0x80001234"` (lowercase `0x`, padded to 8 hex digits)
for func_stats keys. JSON keys must be strings — that's natural.
`call_edges` is an array of objects with `caller`/`callee` string
fields — same format.

**Suggested**: `format!("0x{:08x}", pc)` — lowercase, zero-padded to 8
digits. Matches the roadmap example. Self-consistent with how PCs
appear in `meta.json` symbol entries (let's verify — m0 ships
TraceSymbol's `addr: u32` as a JSON number, not a hex string; that's
fine because they're metadata, not lookup keys).

### Q13: Speedscope "evented" format — what's the minimal correct JSON to produce?

Per Speedscope docs (https://github.com/jlfwong/speedscope/wiki/Importing-from-custom-sources),
the "evented" profile format has this top-level shape:

```json
{
  "$schema": "https://www.speedscope.app/file-format-schema.json",
  "exporter": "lp-cli profile m2",
  "name": "examples/basic --mode steady-render",
  "activeProfileIndex": 0,
  "profiles": [{
    "type": "evented",
    "name": "main",
    "unit": "none",
    "startValue": 0,
    "endValue": <total_cycles>,
    "events": [
      { "type": "O", "frame": 0, "at": 0 },
      { "type": "O", "frame": 1, "at": 100 },
      { "type": "C", "frame": 1, "at": 200 },
      { "type": "C", "frame": 0, "at": 300 }
    ]
  }],
  "shared": {
    "frames": [
      { "name": "render::frame" },
      { "name": "shader::rainbow::palette_warm" }
    ]
  }
}
```

m2 must reconstruct synthetic open/close events from the call_edges
data model. Two approaches:
- **(a)** Faithful flame chart from real timeline — requires keeping
  the *event-ordered* call/return history per-frame, not just
  aggregated counts. That's a much bigger data model than what the
  roadmap pinned ("callgrind-style data").
- **(b)** **Synthetic** events from aggregated `call_edges` —
  reconstructed at finish by walking the edges in some order. Loses
  per-frame ordering, but the flame chart still shows hot paths
  correctly (because the cumulative shape is what humans look at).

Roadmap text leans toward (b): "Conversion happens at finish from the
callgrind data model: for each call edge, emit synthetic open/close
events." So roadmap-aligned answer is **(b)**.

**Suggested**: option **(b)**. The flame chart will be shape-correct
(top-N callees roll up into top-N callers) but won't reflect actual
chronological order. That's a known and accepted limitation of
callgrind→speedscope conversion — `kcachegrind` users live with it.

If we later want true chronological flame charts, that's a separate
collector (`cpu-log`) which streams every call/return event to a
JSONL file. Out of scope for m2.

### Q14: Where does the per-instruction dispatch happen — `run_inner_fast`, `run_inner_logging`, both, or via the cycle bump?

m1's Phase 5 design recommends not adding any per-instruction work in
`run_loops.rs` (only the syscall path). m2 has to add it. Options:
- **(a)** Inline dispatch in both `run_inner_fast` and
  `run_inner_logging`, identical code.
- **(b)** Inline only in `run_inner_fast`; logging path skips dispatch
  (logging is for debug, not profiling). Means `lp-cli profile`
  cannot be combined with `--log-level`.
- **(c)** Factor the cycle-bump + dispatch into a tiny inline helper
  called from both paths.

**Suggested**: option **(c)** — small inline `fn` like
`bump_cycles_and_dispatch(...)` that both run loops call. Keeps the
two paths in sync without code duplication. The compiler should inline
it on the hot path. Logging path keeps profiling enabled because
`lp-cli profile --collect cpu` should never silently lose data based
on a log level.

### Q15: How big do we expect `cpu-profile.speedscope.json` to be for `examples/basic` 4-frame steady-render?

Rough estimate: ~10M cycles × ~1 call per ~10 cycles = ~1M call/return
events. At ~50 bytes each (open/close JSON object) that's ~50MB. Too
big.

But — synthetic events from `call_edges` instead of real timeline (per
Q13) means: events count = `call_edges.len() * 2` (one open, one
close per edge). With ~10k unique call edges, that's ~20k events at
~50B = ~1MB. Reasonable.

**Suggested**: confirm the speedscope writer emits per-edge synthetic
events, not per-call events. The roadmap pins this as the data model.
Document the limitation in the writer module's doc comment so future
contributors don't try to "fix" it by streaming real events
(that's the cpu-log collector).

### Q16: Should `cpu-profile.json` include the full per-PC table or only the entries with non-zero attribution?

`func_stats` only contains PCs that had at least one instruction
attributed (every entry has `self_cycles > 0` or was pushed/popped
through). So this is naturally bounded. No filtering needed.

**Suggested**: no filtering — emit every entry that landed in
`func_stats` and `call_edges`. The roadmap example shows top-N
truncation only for the *report* (top-20 by self/inclusive); the
`cpu-profile.json` is the full data model so m3's diff has everything
to compare.

## Resolved questions

### R1: Gate → collector wiring

**New `Collector::on_gate_action(&mut self, GateAction)` trait method**
(default no-op). `ProfileSession::on_perf_event` runs the gate, fans
out the event to every collector via `on_perf_event`, then fans out
the resulting `GateAction` via `on_gate_action`. `CpuCollector`
overrides to flip its `active: bool`. Other collectors (alloc) keep
the default no-op and stay always-on.

Rationale: per-collector gating policy. `AllocCollector` was always-on
in m0 — flipping it to gated would surprise users. Future collectors
(`cpu-log`) will want their own lifecycle hooks (e.g., flush on
disable). Trait fan-out shape is symmetric with `on_perf_event`. Cost
is one extra trait method + one extra loop iteration per perf event
(perf events are sparse; not on hot path).

### R9: `dispatch_instruction` shape

**Trait fan-out** — `ProfileSession::dispatch_instruction` loops over
collectors and calls `Collector::on_instruction(pc, target_pc, class,
cycles)` on each. Default no-op for collectors that don't care
(`AllocCollector`, `EventsCollector`).

Rationale: architecture-symmetric with `on_perf_event` and
`on_syscall` fan-out. The cpu-log collector is on the roadmap (the
"detailed view" future work), so a second per-instruction collector is
already known. Virtual-call cost is ~20-50ms over a 4-frame capture
(~10M instructions) — invisible in a tool that runs for seconds. If
benchmarks ever flag it, the right fix is `#[inline(always)]` +
monomorphization, not a special-case field on `ProfileSession`.

### R-INIT: Initial active state + synthetic profile events

`CpuCollector` starts with `active: false`. Capture begins only when
the gate fires `GateAction::Enable`.

To make this work uniformly across modes, `ProfileSession` emits two
synthetic perf events through its normal `on_perf_event` path:

- **`profile:start`** at session boot (`PerfEventKind::Instant`,
  `cycle: 0`). Lands in `events.jsonl` as a visible marker; gives
  every gate a uniform "boot" hook.
- **`profile:end`** at session shutdown (`Instant`, `cycle: <final>`).
  Purely for the events.jsonl timeline; collectors are already
  finalizing.

Per-gate `profile:start` semantics (m2 updates m1's gate impls):

| Mode           | profile:start action | Stop trigger                              |
| -------------- | --------------------- | ----------------------------------------- |
| steady-render  | NoChange              | `Capturing` after STEADY_RENDER_CAPTURE_FRAMES (m1 unchanged) |
| compile        | Enable                | First EVENT_SHADER_COMPILE End            |
| startup        | Enable                | First EVENT_FRAME End                     |
| all            | Enable                | None (relies on `--max-cycles`)           |

**Cross-milestone ownership**: m2 owns the `profile:start`/`profile:end`
event emission, the `EVENT_PROFILE_START`/`EVENT_PROFILE_END`
constants, and the gate-impl updates. Rationale: m1 is in flight;
default-false is m2's design choice; the boot event is required by
that choice; therefore m2 owns the whole package.

### R14: Per-instruction dispatch site in run loops

**Inline helper called from both `run_inner_fast` and
`run_inner_logging`**:

```rust
#[inline(always)]
fn after_execute(&mut self, pc: u32, exec_result: &ExecutionResult) {
    let class = exec_result.class;
    let cost = self.cycle_model.cycles_for(class) as u32;
    self.cycle_count += cost as u64;
    if let Some(profile) = self.profile_session.as_mut() {
        let target_pc = exec_result.new_pc
            .unwrap_or(pc.wrapping_add(exec_result.inst_size as u32));
        profile.dispatch_instruction(pc, target_pc, class, cost);
    }
}
```

Both run loops call `self.after_execute(pc, &exec_result)` at the same
point (immediately after `decode_execute` returns).

Rationale: DRY — m1 already has both loops; adding parallel m2 logic
in two spots is exactly what drifts. `#[inline(always)]` makes the
compiled code identical to inlining at each site. Logging path keeps
profile data, so `lp-cli profile --log-level debug --collect cpu` is
not a silent footgun. `target_pc` derivation
(`new_pc.unwrap_or(pc + inst_size)`) lives in one place.

### R2: Cycle-model label sourcing

**Both** — `cycle_model: String` field added to `SessionMetadata` (for
`meta.json`) and `CpuCollector::new(label: &'static str)` carries it
in-memory for the report banner. CLI handler maps `--cycle-model
{esp32c6,uniform}` → `("esp32c6"|"uniform", CycleModel::{Esp32C6,InstructionCount})`.

### R3: Symbol sourcing for symbolize at finish

**In-memory** — handler holds a `Vec<TraceSymbol>` and passes
`&[TraceSymbol]` slices into the output writers (speedscope + canonical
JSON). No `meta.json` read-back. m5 will add a parallel
`&[DynamicSymbol]` overlay.

### R4: PC → name resolution

**Interval lookup** — sort symbols by `addr` once at startup, binary
search for the largest `addr ≤ pc`, return the name if `pc < addr +
size`, else miss. O(log N) per unique PC; symbolization happens at
finish over `func_stats`/`call_edges` keys, not per-instruction.

### R5: JIT region detection

**Heuristic** — any PC ≥ `RAM_START` (`0x8000_0000`) with no static-ELF
hit → `<jit:0xADDR>`. Anything else with no hit → `<unknown 0xADDR>`.
No m5 dependency. m5 will overlay specific JIT-symbol names without
changing this default fallback.

### R6: Map type

**`std::collections::HashMap`** — no FxHash, no capacity hints. Bounded
at low tens of thousands of entries for a 4-frame steady-render
capture. Revisit only if a perf milestone flags it.

### R7: `CpuCollector::on_instruction` dispatch shape

**Single function, single match** — self-cycle bump unconditional;
call/return/tail/indirect handled in match arms; rare arms cheap on
modern branch predictors.

### R8: Cargo feature for cpu collector

**No new feature** — `cpu.rs` lives behind `#[cfg(feature = "std")]`
like `alloc.rs`; opt-in at runtime via `--collect cpu`. Per-instruction
dispatch hook in run loops is unconditional (matches existing
`Option<&mut ProfileSession>` pattern).

### R10: Default `--collect cpu` auto-includes `events.jsonl`

**Yes** — auto-include `events` collector whenever `cpu` is enabled.
Flame charts always want a timeline beside them. Asymmetry intentional:
`--collect alloc` does *not* auto-include events.

### R11: `report.txt` section ordering

**Registration order from `--collect`** — collectors append in the
order they appear on `--collect`. No special ordering logic.

### R12: Hex format for PC keys in `cpu-profile.json`

**`format!("0x{:08x}", pc)`** — lowercase, zero-padded to 8 digits.
Applies to `func_stats` keys and to the `caller`/`callee` string fields
in `call_edges` entries.

### R13 + R15: Speedscope events shape and file size

**Synthetic events from `call_edges`** at finish. The data model is
aggregated (callgrind-style), not chronological — so events are
fabricated by walking call edges DFS:

```text
for each edge (caller -> callee):
    emit { O, frame_id(callee), at: cursor }
    cursor += call_edges[(caller, callee)].inclusive_cycles
    emit { C, frame_id(callee), at: cursor }
```

Result: shape-correct flame chart (every callee bar has correct
cumulative width relative to its parent), but the x-axis is
**synthetic cycles**, not wall-clock. Multiple non-contiguous calls to
the same function smash together into one bar.

**Limitation** — documented in the speedscope writer's module-level doc
comment so future contributors don't try to "fix" it by streaming real
events. The fix is a separate `cpu-log` collector that streams every
call/return event to a JSONL file. Out of scope for m2; tracked as
future work in m6 or beyond.

File size: ~1MB for `examples/basic` 4-frame steady-render (~10k
unique edges × 2 events × ~50B). Real-timeline approach would have
been ~50MB. Speedscope+browser handle 1MB comfortably.

### R16: `cpu-profile.json` filtering

**No filtering** — emit every entry that landed in `func_stats` /
`call_edges`. Top-N truncation only in the `report.txt` section. m3's
diff needs the full data model.
