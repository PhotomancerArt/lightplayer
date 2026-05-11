# CPU Profile Roadmap — Notes

## Scope

Add a deterministic, function-attributed CPU/cycle profiler to `fw-emu` so we
can answer "where do cycles go on device?" without relying on wall-clock FPS or
host-machine profiling. Mirror the existing `mem-profile` / `heap-summary`
shape: an emulator-side tracer + a `lp-cli` driver + a host-side analyzer.

Primary user question this enables:

> "I changed X expecting Y improvement. Did Y happen, and if not, where did
> the cycles actually go?"

Secondary user questions:

- "What functions dominate a frame for `examples/basic` (rainbow.shader)?"
- "Is the hot path in shader code, in the JIT runtime glue, or in the
  allocator?"
- "Did this change move work between functions, or did it create new work?"

Out of scope for this roadmap:

- On-device (fw-esp32) profiling. Hardware perf counters / sampled stack
  collection on the ESP32-C6 is a separate effort.
- Modeling ICache misses, branch mispredict, variable DIV cycles, PSRAM/flash
  latency. The default `CycleModel::Esp32C6` (per-class fixed costs) is
  considered "good enough for A/B" and refinements are a future-work item.
- Profiling the host-side compile pipeline (use `samply` / `cargo flamegraph`
  for that — orthogonal tool, different code path).

## Current State

### What exists

- `lp-riscv-emu` already tracks `instruction_count` and `cycle_count` per
  guest instruction. `CycleModel::Esp32C6` is the default and assigns per-class
  costs (Alu=1, Load=2, BranchTaken=2, Jal=2, Jalr=3, DivRem=32, System=4,
  Fence=4, Atomic=4). Updated inline in both `run_inner_fast` and
  `run_inner_logging` (`run_loops.rs:95` and `:199`).
- `InstClass` already separates `Jal` and `Jalr` from `Alu`, so call/return
  detection at the run-loop level has the structural information it needs
  without re-decoding.
- `mem-profile` end-to-end pattern is the template:
  - `lp-cli/src/commands/mem_profile/handler.rs` builds fw-emu with the
    `alloc-trace` feature, loads its ELF, drives an `LpClient` against
    `SerialEmuClientTransport`, and ticks `emu.advance_time(40)` per frame.
  - `lp-riscv-emu/src/alloc_trace.rs` is the host-side tracer (JSONL writer
    keyed off a syscall in the run loop, with `meta.json` carrying the symbol
    list extracted from the ELF).
  - `lp-cli/src/commands/heap_summary/` is the analyzer that consumes the
    trace dir and produces a textual report.
  - The same mechanism appends a `report.txt` next to the trace.
- `lp-riscv-elf::load_elf` returns `symbol_map` and `symbol_list`;
  `mem-profile` already serializes `symbol_list` into `meta.json` as
  `TraceSymbol { addr, size, name }`.
- `BinaryBuildConfig::with_backtrace_support(true)` adds
  `-C force-frame-pointers=yes` to RUSTFLAGS, used today for alloc-trace and
  panic backtraces.
- `Riscv32Emulator::unwind_backtrace` walks the s0/fp chain via guest memory.
  Used by `SYSCALL_ALLOC_TRACE` and panic handling. **Note:** this only works
  for code that maintains a frame pointer — Rust code with the rustflag set,
  yes; JIT'd RV32 produced by `lpvm-native::rt_jit`, **no**.
- `examples/basic` is `rainbow.shader` (5 palette functions, PSRD noise, 241
  LEDs) — the same workload used in `docs/design/native/perf-report/`.
- `fw-tests/tests/scene_render_emu.rs` already drives multi-frame execution
  in a way that closely mirrors what we want.

### What doesn't exist

- No host-side `CpuProfiler` analogous to `AllocTracer`.
- No call/return event extraction from the emulator run loop.
- No per-function cycle attribution (only a global `cycle_count`).
- No way for the host to know where JIT'd shader code lives in guest memory —
  the JIT buffer's PC range is opaque, so PC samples in shader code show up as
  unsymbolized "unknown 0x80....". Static ELF symbols cover only firmware
  Rust code.
- No `lp-cli cpu-profile` / `cpu-summary` / `cpu-diff` commands.
- No flame-chart / Speedscope / folded-stack output anywhere in the project.

### Closely related, possibly reusable

- `alloc_trace` + `heap_summary` are the most direct precedent. The new
  profiler mirrors the directory layout (`meta.json`, JSONL, `report.txt`),
  the build path (fw-emu feature flag), and the analyzer pattern (`lp-cli
  heap-summary`).
- `unwind_backtrace` may serve as a debug fallback / sanity check, but is
  *not* the primary mechanism for stack reconstruction (we synthesize stacks
  from call/return events instead — see Q1).
- `cycle_model.rs` is the right place to extend if we ever want to refine
  per-class costs.

## Questions

Each question includes context and a suggested answer. To be resolved with
the user one at a time, results recorded back into this file.

### Q1: How should the profiler reconstruct the call stack?

**Resolved: Option 1 — call/return instruction-shape detection.**

Detect call/return purely from instruction encoding:
- call: `JAL rd, _` with `rd != x0`, or `JALR rd, _, _` with `rd != x0`
- return: `JALR x0, x1, 0` (canonical `ret`); compressed `c.jr ra`
- tail call: `JAL x0, _`, or `JALR x0, rs, _` with `rs != ra` — modeled as
  pop+push on the shadow stack

Push/pop a host-side shadow stack on every call/return; accumulate cycles to
top-of-stack on every instruction. Equivalent to Valgrind's callgrind data
model.

**Correction made during discussion:** lpvm-native *does* maintain frame
pointer discipline for non-leaf / spilling / callee-saved functions
(`lp-shader/lpvm-native/src/abi/frame.rs:71`,
`isa/rv32/emit.rs:282-289`), with `[fp-4] = ra` and `[fp-8] = prev_fp`,
matching the layout `Riscv32Emulator::unwind_backtrace` reads. The earlier
claim that "JIT'd code doesn't have FP discipline" was wrong. This means
FP-walk sampling (Option 2) was actually viable too. We still chose
Option 1 because:

- **Exact call counts are signal.** The 2026-04-10 perf bug
  ("`O(n)` lookup × 240 LEDs per frame") would have screamed "called 240
  times per frame" under Option 1. Under Option 2 it would appear only as
  "dominates samples" without the why.
- **Per-event cost is lower.** Option 1 = ~1 branch + 1 add per instruction
  + small hashmap update on JAL/JALR (~5% of instructions). Option 2 = N
  memory reads per sample.
- **Diff mode is cleaner with exact counts**, no sub-sample-period wobble.

User decision: "it has to be option 1 — we have complete control over the
code we're running, so we can assume the best case and not worry about a
fallback at least initially". So no FP-walk fallback in scope; tail calls
and indirect calls handled in the shadow-stack maintenance logic; longjmp /
unwind reconciliation deferred (not used by current shader path).

**Implications for milestones:**
- m1 needs a "call/return kind" classifier next to `InstClass`, applied
  inline in `run_inner_fast`. The existing `InstClass::Jal` / `InstClass::Jalr`
  isn't sufficient because it doesn't separate `rd != x0` from `rd == x0`.
- Shadow stack is host-side, lives on `CpuProfiler`, not in the guest.
- No firmware change required for m1.

### Q2: Aggregate in the emulator, or stream raw events?

**Resolved: aggregate by default, raw events as opt-in (`--raw-events`),
follow prior art for both data model and on-disk format.**

User direction: "prior art should guide us here."

**Data model — adopt callgrind's.** Maintain on `CpuProfiler`:
- `HashMap<(caller_pc, callee_pc), CallEdge>` with
  `{ count, inclusive_cycles }`
- `HashMap<pc, FuncStats>` with `{ self_cycles, calls_in, calls_out }`
- Shadow stack frames hold `(callee_pc, cycles_at_entry,
  self_cycles_at_entry)`. On return, fold the inclusive-cycles delta into
  the function and the caller→callee edge.

This is exactly the data Valgrind's callgrind emits and exactly what every
flame-chart tool (speedscope, FlameGraph, kcachegrind) consumes. No
invention.

**On-disk format.** Two files in the trace dir, mirroring `mem-profile`:
- `meta.json` — `{ schema_version, cycle_model, frames, warmup, dynamic_symbols, static_symbols, ... }` (same shape as `mem-profile`'s meta).
- `cpu-profile.json` — aggregated dump. JSON, not a custom binary, because:
  - The aggregated payload is small (kilobytes for typical runs, low MB worst case).
  - Matches the `alloc-trace.jsonl` precedent of human-inspectable output.
  - Easier to diff / cat / spot-check during development.

We do **not** invent a new wire format. If we ever want kcachegrind support,
add a `--format callgrind` exporter in `cpu-summary` later — it reads our
JSON and writes their text format. Same for speedscope JSON output.

**Raw mode (`--raw-events`).** Defer past m1, but reserve the flag name now
so the directory layout doesn't churn later. When implemented: a binary
record file `cpu-events.bin` with fixed-width records `{ kind: u8, _pad: u8,
target_pc: u32, return_pc: u32, cycle_count: u64 }` (16 bytes). Binary,
not JSON, because raw mode by definition is high-volume and we'd be
serialization-bound. Same precedent as perf.data.

**Implications for milestones:**
- m1: aggregator + JSON dump only. No raw mode.
- Output-format milestone (m2 in current draft) reads the JSON and produces
  speedscope / folded-stack / etc.
- Raw mode lives in a follow-up plan (or m1.5) — not in scope for the
  initial roadmap.

### Q3: How do we attribute cycles to JIT'd shader code?

**Resolved: Option 1 — `SYSCALL_JIT_MAP` from the runtime, in its own
milestone (not m1). Design the wire format for load+unload now,
implement only load in the first cut.**

User direction: "one of the big advantages of building our own perf tool
is that we _can_ easily get those symbols ... for now we can assume we
aren't recompiling shaders, though that is something we'd have to think
about later ... we should design for the future, but don't go too far
making it perfect now."

**Wire format (designed for load + unload from the start).**

Two syscall numbers, both reserved now, only the first implemented in the
JIT-symbols milestone:

- `SYSCALL_JIT_MAP_LOAD`: runtime calls after `link_jit` succeeds.
  args = `(base_addr: u32, len: u32, count: u32, ptr_to_entries: u32)`
  where each entry is `{ offset: u32, size: u32, name_ptr: u32,
  name_len: u32 }`. Host appends to dynamic-symbol overlay tagged with
  `loaded_at_cycle = self.cycle_count`.
- `SYSCALL_JIT_MAP_UNLOAD`: runtime calls before freeing a JIT module
  buffer. args = `(base_addr: u32)`. Host marks all entries with that
  `base_addr` as `unloaded_at_cycle = self.cycle_count`.

Symbol lookup at attribution time is then an interval query: "find symbol
covering `pc` whose `[loaded_at_cycle, unloaded_at_cycle)` contains the
current cycle". Cycle count is the same monotonic counter we already use
for cycle attribution — same clock, no synchronization issues.

**Scope simplification for first cut:** the symbols milestone implements
only `LOAD` and assumes modules live forever. Lookup degrades to a flat
`HashMap<pc_range, symbol>`. That's fine for current shader pipeline,
which links once at startup and never unloads. The `UNLOAD` syscall and
interval-query lookup land when (a) we add hot-reload support, or (b)
we observe a real collision in a profile.

**Implementation footprint** (symbols milestone, not m1):
- ~30 LOC in `lp-shader/lpvm-native/src/rt_jit/module.rs` to issue the
  syscall after link_jit and pass the function table.
- New constants `SYSCALL_JIT_MAP_LOAD`, `SYSCALL_JIT_MAP_UNLOAD` in
  `lp-riscv-emu-shared`.
- ~50 LOC handler in `run_loops.rs` that reads the entry array out of
  guest memory and records into a `JitSymbols` overlay on the emulator.
- Symbolizer in `lp-cli` consults static (ELF) and dynamic (overlay)
  symbol tables, in that order.

**Bonus payoff:** once `SYSCALL_JIT_MAP_LOAD` exists, panic backtraces
and `alloc-trace` reports also start showing real shader function names,
not just `<unknown 0x80a4f10>`. So this milestone has value beyond the
profiler.

**Implications for milestones:**
- m1 ships with JIT'd code as a single anonymous range labelled
  `<jit:0x80a40000+0x12000>` or similar — informative ("80% of cycles
  are in shader code") even without per-function names.
- A later milestone (currently slotted as m3) adds `SYSCALL_JIT_MAP_LOAD`
  + the symbolizer overlay. Per-shader-function attribution lights up.
- `UNLOAD` syscall is *reserved* (constant allocated, doc'd) but not
  implemented until needed.

### Q4: What output formats do we ship?

**Resolved: three outputs, no more.** *(Command shape "cpu-profile"
referenced below was superseded by Q8 — read as `fw-profile --collect
cpu`. Output format and file-shape decisions still hold.)*

User direction: "I see three basic needs initially: short, readable text
summary (sanity check, agent-readable, printed in console);
speedscope-compatible file for human deep-diving; diff-able file of some
sort for comparison between runs."

The three outputs and where each one is produced:

1. **`report.txt`** — short top-N text summary. Printed to console at the
   end of `cpu-profile` and also written into the trace dir. Format mirrors
   `heap-summary`: top-20 by self cycles, top-20 by inclusive cycles, total
   cycles, frames, warmup. Optimized for "agent reads CI log and tells the
   user what changed." Always shipped. Always shown.

2. **`cpu-profile.speedscope.json`** — Speedscope's "evented" format
   (https://github.com/jlfwong/speedscope/wiki/Importing-from-custom-sources).
   Drag-and-drop into https://speedscope.app, no install. The flame chart
   for human deep-diving. Always shipped.

3. **`cpu-profile.json`** — the canonical aggregated callgrind-style dump
   from Q2. Stable schema, semver'd via `meta.json`'s `schema_version`.
   This is the diff-able file. `cpu-diff <a> <b>` reads two of these and
   produces a regression/improvement report. Always shipped (it *is* the
   primary trace artifact).

Output files in the trace dir:

```
traces/cpu/<timestamp>-<note>/
  meta.json
  cpu-profile.json              ← diff source of truth
  cpu-profile.speedscope.json   ← human deep-dive
  report.txt                    ← agent / CI sanity check
```

**Explicitly dropped from scope:**
- Folded-stack format (`main;render;palette 12345`). Speedscope covers the
  flame-chart use case; folded-stack is redundant. Add later if someone
  wants `flamegraph.pl` integration.
- Callgrind text format (`kcachegrind`). Heavy install, niche audience.
  Add later if someone needs source-annotated views.
- HTML self-contained report. Speedscope already is one.

**Implications for milestones:**
- m1 must produce all three files. They're not separable — the speedscope
  serializer and report.txt printer both consume the in-memory callgrind
  structure that the profiler already has to build.
- The diff *command* (`cpu-diff`) is its own milestone. The diff-able
  *file* (`cpu-profile.json`) ships in m1.
- Schema for `cpu-profile.json` needs a version field from day one
  (`schema_version: 1` in `meta.json`) so future format changes don't
  invalidate old traces silently.

### Q5: What's the workload, and how do we scope what we measure?

**Resolved: workload defaults to `examples/basic`. Scoping is done via a
perf-event system that lives in `lp-engine` and ships in m1 as baseline.**

User direction: "lp-cli owns the state machine, it's provided somehow to
the emulator in config, dyn trait, function pointer, or something like
that. Named modes is exactly right. Keep the code in Rust, we don't need
or want a DSL right now ... I do think this is baseline though, we should
start with it ... I actually like these being in the main lp-engine
itself. In the emulator we can call the syscall, on real hardware we can
print to the console with exact time codes, which gives us real data on
device we can correlate with our emu perfs."

**Workload.** `examples/basic` (rainbow.shader on 241 LEDs). Matches
existing `mem-profile` precedent and the workload on which 2026-04-10
perf bugs were caught. Overridable as a positional arg to `cpu-profile`.

**Architecture: three layers, clean separation.**

1. **`lp-engine`** owns event *emission*. New trait:

   ```rust
   pub trait PerfEventSink {
       fn emit(&mut self, name: &'static str, kind: PerfEventKind);
   }
   pub enum PerfEventKind { Begin, End, Instant }
   ```

   Engine code calls `sink.emit("frame", Begin)` at known boundaries. The
   *engine itself* is what's instrumented, not the firmware shell. So the
   same emission points fire on emu and on device. Canonical event names
   live in `lp-engine` (`pub const EVENT_FRAME: &str = "frame";` etc.) so
   downstream tools agree on identifiers.

2. **Two sinks, one trait:**

   - `EmuPerfSink` (in `fw-emu`): calls `SYSCALL_PERF_EVENT(name_ptr,
     name_len, kind: u8)`. Host records `(cycle_count, name, kind)` into
     `events.jsonl` *and* feeds the event to the gate.
   - `HardwarePerfSink` (in `fw-esp32` and other device targets): prints
     `[perf] <cycles> <name> <kind>` to the console using the platform's
     cycle counter (e.g. `mcycle` CSR on ESP32-C6, or system tick).
     Format is intentionally grep'able.

   This is the leverage point: **events are the same on emu and on
   device**. Console output from a real-hardware run can be parsed into
   the same event-stream representation as `events.jsonl`, which gives
   us a direct emu-vs-device correlation for every event boundary.
   Cycle-model accuracy becomes a *measurable, bounded* question instead
   of a hand-wave.

3. **`lp-cli`** owns event *consumption* and gating. Profile mode is a
   Rust enum (no DSL, ever) — and crucially, **modes carry no
   parameters**:

   ```rust
   pub enum ProfileMode {
       SteadyRender,
       Compile,
       Startup,
       All,
   }
   ```

   Each variant implements a small state machine with hardcoded
   behavior:
   `fn observe(&mut self, event: &PerfEvent) -> GateAction { Enable |
   Disable | NoChange | Stop }`. The state machine encodes the *whole*
   policy — warmup, what to capture, when to stop. No `--warmup` flag,
   no other knobs. e.g. `SteadyRender` is internally "wait for first
   `shader-compile` End, skip N `frame` pairs, capture M `frame`
   pairs, then Stop." If the policy needs to change, edit the state
   machine.

   Adding a new mode = adding an enum variant + its state machine.
   Audience is project developers, not external users; this is the
   right ergonomic shape. Per user: "if the state machine wants to say
   'wait until shader-linked, then wait for 2 frame-end, then start
   recording for 4 frame-end, then stop' — that's great, we don't need
   a mode-config system yet."

   The mode is passed to the emulator as part of `cpu-profile`
   configuration. Likely shape: `Box<dyn FnMut(&PerfEvent) -> GateAction>`
   or a small trait object — final binding decided at implementation.
   The emulator records all events unconditionally and calls the closure
   to decide whether attribution is currently active.

**Initial event vocabulary** (defined in `lp-engine`):

- `frame` (Begin/End) — one render pass through all LEDs.
- `shader-link` (Begin/End) — `lpvm-native::link_jit` window.
- `shader-compile` (Begin/End) — full LPIR build pipeline (parse +
  optimize + lower + link), encloses one or more `shader-link`s.
- `project-load` (Begin/End) — initial project parse / asset load.

Final list refined during m1 implementation as we sprinkle emission
points; the architectural commitment is that events are owned by the
engine and named centrally.

**Default modes** (all parameter-free; tuning happens in code):

- `SteadyRender` (default): wait for first `shader-compile` End, then
  skip first 2 `frame` pairs, then capture next 4 `frame` pairs, then
  Stop. Numbers chosen as starting point; refined as we measure.
- `Compile`: active during `shader-compile` regions, Stop after first
  `shader-compile` End completes.
- `Startup`: active from `t=0` until first `frame` End, then Stop.
  Captures project-load + first-compile + first-render together.
- `All`: always active; needs an external time/event cap to stop.

**Implications for milestones:**

- **m1 (profiler core)** now includes: profiler + callgrind data model
  + `PerfEventSink` trait in `lp-engine` + `EmuPerfSink` in `fw-emu` +
  `events.jsonl` recording + `ProfileMode` enum + initial event
  emission points in `lp-engine`. Larger than originally scoped, but
  user explicitly accepted the extra time: "I don't mind it taking a
  little longer ... it's a short milestone of adding the perf points
  to the system."
- **Hardware sink milestone** (separate, follow-up): `HardwarePerfSink`
  for `fw-esp32` + console-output parser in `lp-cli` to ingest
  device-side event streams + emu-vs-device correlation report. This
  is the "validate the cycle model against reality" payoff.
- No `--warmup` CLI flag. The mode's state machine fully encodes
  warmup behavior. If you need different warmup, you change the mode
  (or add a new variant).
- The mode's `Stop` action is the run terminator for the profile: when
  the state machine returns `Stop`, the profiler dumps and `lp-cli`
  exits the run loop. No `--frames` cap needed for well-defined modes.
  A safety `--max-cycles N` cap is added separately for runaway
  protection.

**Deferred / future-work:**

- Per-event `arg: u32` payload (e.g. shader ID on `shader-link` End).
  Reserve room in the syscall ABI but don't wire up emission in m1.
- `--gate` DSL. Explicitly out of scope, possibly forever.
- Begin/End mismatch detection beyond a console warning.

### Q6: What does the CLI surface look like?

**Resolved: two commands. `cpu-profile` does everything; `cpu-diff`
exists for ad-hoc cross-run comparisons.** *(Command names superseded
by Q8 — `cpu-profile` → `fw-profile --collect cpu,...`, `cpu-diff` →
`fw-diff`. Surface-shape decisions — single profile command + separate
diff command, no separate summary command, mode-state-machine handles
warmup, `--diff [PATH]` auto-finds prior matching run — all carried
forward.)*

User direction: "fewer commands is better here. The cpu-profile /
cpu-summary thing may not be needed initially. We never actually really
used the separate summary command before. I'd rather cpu-profile
generate the data directly, controlled by flags. My instinct is to have
a `--diff` flag that just auto-diffs against a previous run in the
summary."

**Naming:** `cpu-profile` (not `perf-profile`, not `cycle-profile`). Per
user: "yes it's not really wall-clock time, but it's our best
*estimation* of wall clock time. If we build a better simulated
time-estimator in the emulator, we'll use it here." Matches the term
the rest of the ecosystem uses (Linux perf, Go pprof, samply,
instruments).

**Surface:**

```
lp-cli cpu-profile [DIR=examples/basic]
                   [--mode {steady-render,compile,startup,all}=steady-render]
                   [--note STR]
                   [--diff [PATH]]
                   [--max-cycles N]            # safety cap, large default
                   [--include-alloc-trace]     # see Q8

lp-cli cpu-diff <trace-dir-a> <trace-dir-b> [--top N=20]
                                            [--threshold-pct F=1.0]
                                            [--threshold-cycles N=1000]
```

That's it. No separate summary command — `cpu-profile` always prints
the top-N text summary to stdout and writes `report.txt` into the trace
dir on completion. The state machine inside the chosen `--mode` decides
when to stop, so no `--frames` or `--warmup` flags.

**`--diff [PATH]` semantics:**

- `--diff` (no arg): find the most recent prior trace dir for the same
  workload+mode (matched on the dir name embedded with workload+mode)
  and run a diff against it after the new profile completes. Print the
  diff to stdout below the regular summary.
- `--diff <path>`: diff against that explicit trace dir.
- Absent: just the new profile's summary, no diff.

This makes "did my change improve render perf?" a one-shot:
`lp-cli cpu-profile --diff` runs a fresh profile and tells you what
changed vs the previous run. The standalone `cpu-diff` exists for the
secondary "compare two specific trace dirs from history" use case.

**Trace dir naming:** `traces/cpu/<timestamp>-<workload>-<mode>[-<note>]/`
e.g. `traces/cpu/20260419-153022-basic-steady-render/`. Workload+mode
in the dir name is what makes `--diff` (no arg) able to find a
matching prior run.

**Output written into the trace dir** (per Q4):
- `meta.json`
- `cpu-profile.json` (canonical, diff source of truth)
- `cpu-profile.speedscope.json`
- `report.txt`

When `--include-alloc-trace` is passed (subject to Q8 resolution), the
alloc-trace files are written into the same dir alongside.

**Implications for milestones:**
- m1: `cpu-profile` (without `--diff`) + `cpu-diff` standalone command.
- The diff milestone wires up `--diff [PATH]` on `cpu-profile` (the
  auto-find-previous logic). Standalone `cpu-diff` is the building
  block; the convenience flag is sugar on top.
- No `cpu-summary` command exists anywhere in the roadmap.

**Deferred / future-work:**
- `cpu-diff --format json` for CI integration. Trivial to add when CI
  asks for it.
- `cpu-profile --list-modes` discoverability flag. Cheap; decide at
  implementation time.

### Q7: Cycle model — keep `Esp32C6` default, refine later, or add hooks now?

**Resolved: `Esp32C6` as-is for m1, `--cycle-model {esp32c6,uniform}`
flag for sanity-checking, refine the model only when empirical data
demands it.**

User direction: "right now we should use Esp32C6 as is — A/B and
hotspots are what we're looking for. We will find out if the model is
wrong empirically. If we find a hotspot using this tool, fix it, we can
run on hardware and see what happens."

**Decisions:**
- m1 uses `CycleModel::Esp32C6` (the existing default). No new variants,
  no microarchitecture refinements.
- `cpu-profile --cycle-model {esp32c6,uniform}=esp32c6` gives the
  developer a one-line sanity check: if `uniform` (1 cycle per
  instruction) and `esp32c6` produce wildly different hotspot rankings,
  someone is doing pathological things with DIV/atomics/etc. that the
  cost classes will exaggerate. Plumbing for this already exists in
  `lp-riscv-emu`, so cost is ~0 LOC.
- No model refinements happen in this roadmap. They happen later, in
  the cross-platform correlation milestone (see Q7.5), driven by real
  divergence data — never by intuition about ESP32-C6
  microarchitecture.

**Discipline this enforces:** we never refine the cycle model on a
hunch. We refine it only when device measurements show the model is
wrong on a workload we actually run. This avoids the "we made the
model 'more accurate' and now A/B comparisons are noisier because we
added stochastic elements" failure mode.

### Q7.5: Cross-platform perf-log and emu/device correlation

This question emerged from Q7's discussion. The core observation: the
event log we already designed in Q5 is not just an internal artifact for
gating — it's the **primary cross-platform output** of the whole system,
and correlation between emu-delta and device-delta is the headline
validation feature.

User framing: "What I'm much more interested in for near-term future
work is running the same perf test on real hardware. We wouldn't get
the call data, but we would get perf-event timing ... in state A: take
a baseline, both in emulator with full details, and on device with
just the perf-log. Change code: take a new snapshot. We can then see
what the correlation between the emu and hardware change is. That seems
like the real magic."

**The four-corner workflow this unlocks:**

|              | State A (baseline)              | State B (after change)         |
| ------------ | ------------------------------- | ------------------------------ |
| **Emu**      | full callgrind + perf-log + symbols | full callgrind + perf-log + symbols |
| **Device**   | perf-log only (event durations) | perf-log only (event durations) |

Three diffs come out of this:

1. **Emu-A vs Emu-B**: full attribution. "Function `palette_warm` got
   12% faster, here's why." (Already covered by `cpu-diff`.)
2. **Device-A vs Device-B**: real-world perf delta on real hardware,
   per event boundary. "Frame got 8% faster on device." (New;
   delivered by hardware milestone.)
3. **(Emu-B − Emu-A) vs (Device-B − Device-A)**: *correlation*. "Emu
   predicted -12% for the frame, device measured -8% — model
   under-predicts by ~4pp on this kind of change." (The killer
   feature.)

Diff #3 is what gives us confidence that emu-side optimization work
*translates* to device wins. Without it we're guessing. With it, the
emu becomes a *trustworthy* development surface.

**Implications for the design (back-pressure into earlier decisions):**

- **`events.jsonl` is promoted from internal artifact to primary
  output.** Always written, schema-stable, semver'd via
  `meta.json.schema_version`. Listed alongside `cpu-profile.json` in
  the trace dir's primary outputs.
- **Schema must be platform-neutral from day one.** Records are
  `{ t: u64, name: String, kind: u8 }` where `t` is "monotonic
  source-native cycle counter" — emu cycle_count on emu, `mcycle` on
  ESP32-C6, system tick on other targets. Absolute units differ;
  monotonicity and within-trace consistency are what matter for
  correlation.
- **`meta.json` declares `clock_source`** (e.g. `"emu_estimated"`,
  `"esp32c6_mcycle"`) so consumers know what `t` means and can
  unit-convert when needed.
- **Trace dirs from emu and from device are interchangeable to most
  tools.** A device trace dir contains `meta.json` + `events.jsonl`
  + `report.txt` (event-pair durations) but no `cpu-profile.json` or
  speedscope file. Tools key off "what files are present" to know
  what's available.

**New milestone in scope: hardware correlation milestone.** Includes:
- `HardwarePerfSink` for `fw-esp32` (and the trait abstraction in
  `lp-engine` so other targets can implement it).
- `lp-cli perf-log --device <port>` (or similar) that captures the
  device's console output during a session and parses it into a
  device trace dir with the same shape as an emu trace dir.
- `perf-log-diff` or `cpu-diff --perf-log-only` mode that diffs two
  trace dirs using only `events.jsonl` (works for device-vs-device
  and emu-vs-emu).
- **Correlation report**: takes four trace dirs (emu_a, emu_b,
  device_a, device_b) and reports per-event-boundary how well
  emu-delta predicts device-delta. Output: a small table of
  `{event, emu_delta_pct, device_delta_pct, agreement}` plus a
  summary "emu over/under-predicts by X% on average."

**Order of milestones now looks like:**

1. m1 — profiler core + perf-event system (events.jsonl as a primary
   output).
2. m_diff — `cpu-diff` command + `--diff` flag on `cpu-profile`.
3. m_hardware — `HardwarePerfSink` + console parser + `perf-log-diff`
   + correlation report.
4. m_jit_symbols — `SYSCALL_JIT_MAP_LOAD` + symbolizer overlay.
5. m_cleanup — validation, docs, scaffolding removal.

m_hardware moved up ahead of m_jit_symbols because (a) it's where the
unique-to-this-system value lives — call symbolization is table stakes,
emu/device correlation is the moat — and (b) it doesn't depend on JIT
symbols (event-pair durations don't need PC attribution).

**Implications back into m1:**
- `events.jsonl` schema is locked in m1, including `t`/`clock_source`
  fields, even though m1 only emits `clock_source: "emu_estimated"`.
- Trace dir layout is finalized in m1 such that m_hardware can write
  device trace dirs of the same shape without back-compat changes.

### Q8: One unified `fw-profile` command, with composable collectors

**Resolved: collapse `mem-profile`, `heap-summary`, and the proposed
`cpu-profile` / `cpu-summary` / `cpu-diff` into a single `fw-profile`
command (plus `fw-diff`). All tracers become "collectors" composed in
one run, sharing one trace dir, one mode/gate, and one diff surface.
`mem-profile` is removed (no users to migrate).**

User direction: "why are memory and cpu profiling different tasks?
They're really not. All the same things apply to both: wanting to
start and stop the profiler? Correlate between profiles? ... All this
points to a single lp-cli command for all this, probably called
fw-profile or similar. Since we're adding a new thing, this is the
time to refactor, we don't have any users of mem-profile, so we can
change without concern."

**The shape:**

```
lp-cli fw-profile [DIR=examples/basic]
                  [--collect <list>=cpu]
                  [--mode {steady-render,compile,startup,all}=steady-render]
                  [--cycle-model {esp32c6,uniform}=esp32c6]
                  [--note STR]
                  [--diff [PATH]]
                  [--max-cycles N]

lp-cli fw-diff <trace-dir-a> <trace-dir-b> [--top N=20]
                                           [--threshold-pct F=1.0]
                                           [--threshold-cycles N=1000]
```

**Collectors (the unit of composition):**

Each collector is a trait implementation that:
- Hooks into the run loop or specific syscalls.
- Maintains its own host-side state.
- Writes its own files into the shared trace dir on flush.
- Contributes a section to the combined `report.txt`.

Initial collectors:

- `cpu` — call/return shadow stack + cycle attribution. Writes
  `cpu-profile.json`, `cpu-profile.speedscope.json`, contributes
  "CPU summary" section to `report.txt`.
- `alloc` — alloc/dealloc events (replaces what `mem-profile` did).
  Writes `alloc-trace.jsonl`, contributes "Heap summary" section.
- `events` — perf-event log. Writes `events.jsonl`. Auto-enabled
  whenever any other collector is enabled (because the mode/gate
  state machine consumes events). Independently selectable too: if
  you want *only* the perf-log (useful for the hardware case where
  that's all you can get), `--collect events` is enough.

Future collectors plug into the same trait without command surface
churn:
- `cpu-log` — sequential CPU execution log (came up debugging another
  agent's issue; not designed in this roadmap, but the slot exists).
- `syscalls` — full syscall trace.
- `ir-stats` — per-shader-function dynamic counts at the LPIR level.
- ...whatever comes up.

**Composition semantics:**

- All enabled collectors share the same `--mode` gate. When the state
  machine flips on, all collectors start recording; when it flips off,
  all stop. This is what makes "what allocated during the hot frame?"
  trivially answerable — both collectors observed the same window.
- Cycle counter is shared across collectors (a single monotonic source
  per run). Every recorded event/sample is timestamped with
  `cycle_count` — making cross-collector correlation a join, not a
  guess.
- Trace dir contents key off "what files are present." Tools that
  consume the dir adapt to whatever collectors ran.

**Trace dir layout (unified):**

```
traces/<timestamp>-<workload>-<mode>[-<note>]/
  meta.json                       # schema, cycle model, collectors run, clock_source
  events.jsonl                    # if `events` collector ran (almost always)
  cpu-profile.json                # if `cpu` collector ran
  cpu-profile.speedscope.json     # if `cpu` collector ran
  alloc-trace.jsonl               # if `alloc` collector ran
  report.txt                      # combined summary, section per collector
```

No more `traces/cpu/` or `traces/mem/` subdirs. Just `traces/`. The
trace dir is the unit of "a profiling session"; what's *in* it depends
on what collectors ran.

**Module layout (refactored):**

- `lp-riscv-emu/src/profile/` — new module containing:
  - `mod.rs` — `Collector` trait, registry, dispatch from run loop.
  - `cpu.rs` — CPU collector (was the planned `cpu_profile.rs`).
  - `alloc.rs` — Alloc collector (was `alloc_trace.rs`, ported to the
    new trait).
  - `events.rs` — Perf-event collector.
- `Riscv32Emulator`: single `Option<ProfileSession>` field replacing
  the per-tracer fields. `ProfileSession` holds the enabled
  collectors and the gate state machine.
- Builder: `with_profile_session(trace_dir, collectors, mode_fn,
  metadata)`.
- `feature = "std"` gates the whole profile module.
- `fw-emu` feature `profile` (renamed from `alloc-trace`/`cpu-profile`)
  gates any firmware-side help needed (perf-event syscall handler).

**Migration impact:**

- `mem-profile` command: removed.
- `heap-summary` command: removed (its summary becomes a section in
  `report.txt` written by `fw-profile`).
- `lp-riscv-emu/src/alloc_trace.rs`: refactored into
  `profile/alloc.rs` implementing the `Collector` trait. Same data,
  same wire format for `alloc-trace.jsonl`, just plumbed through the
  unified session.
- Existing `mem-profile` callers in tests/CI: none in production use
  per user. Internal test scripts updated to `fw-profile --collect
  alloc`.
- Existing `traces/mem/` and `traces/cpu/` (if any): obsolete. No
  back-compat — clean cut.

**Implications for milestones:**

The roadmap restructures around the unified command. Revised milestone
graph:

1. **m0 — Foundation refactor.** Define `Collector` trait, unified
   `ProfileSession`, unified trace dir layout. Port `alloc-trace` into
   the new shape. Implement `fw-profile --collect alloc` to functional
   parity with old `mem-profile`. Remove `mem-profile` and
   `heap-summary` commands. **No new functionality**, just the
   plumbing rearranged. Ships a working `fw-profile --collect alloc`
   as proof.
2. **m1 — CPU collector + perf-event system.** Adds `cpu` and `events`
   collectors. Adds `--mode` and the `ProfileMode` enum. Adds
   `events.jsonl`, `cpu-profile.json`, `cpu-profile.speedscope.json`.
   Engine-side `PerfEventSink` trait + `EmuPerfSink` + initial event
   emission points in `lp-engine`.
3. **m2 — Diff.** `fw-diff` command + `--diff [PATH]` flag on
   `fw-profile` (auto-find most recent matching prior run).
4. **m3 — Hardware perf-log + correlation.** `HardwarePerfSink` for
   `fw-esp32`. `lp-cli` ingests device console output into a device
   trace dir of the same shape. Correlation report (the four-corner
   diff). Cycle-model refinement work, if any, lives here too —
   driven by data.
5. **m4 — JIT symbols.** `SYSCALL_JIT_MAP_LOAD` + symbolizer overlay.
   Benefits all collectors that record PCs.
6. **m5 — Cleanup / validation.** End-to-end runs, doc'd workflow,
   scaffolding removal.

m0 splits out the refactor as its own shippable point. Even if m1
slips, m0 alone delivers value: cleaner command surface, unified
trace dir layout, foundation for the rest.

**Open question deferred to implementation:** exact CLI shape of
`--collect` (comma-separated list vs repeated flag vs aliases like
`--cpu`/`--alloc`). All workable; pick one in m0 and stick with it.

### Q9: How tightly do we couple the CPU collector to `cycle_count`?

**Resolved: inline in the fast loop behind `Option<&mut CpuCollector>`.
Extend `InstClass` with call/return-aware variants so detection happens
at decode time, not in the collector.**

User direction: "I think option 1 is right. We will likely have to
rethink how the various hot path loops work at some point as we add
more profiling options. But this is good for now."

**Decisions:**

- **Placement**: inline in `run_inner_fast` and `run_inner_logging`,
  right after `self.cycle_count += ...`. Same pattern `alloc-trace`
  uses for its syscall hook. Disabled cost: one always-not-taken
  branch per instruction (negligible — branch predictor handles it).
  Enabled cost: per-instruction self-cycles update (hashmap bump on
  top-of-stack PC) plus full call/return shadow-stack work on
  JAL/JALR (~5% of instructions). Estimated 2-4× slowdown when
  enabled, which is fine — `fw-profile --collect cpu` is the
  developer's inner loop, not CI's.

- **Decode-time classification**: extend `InstClass` with new variants:
  - `JalCall` — `JAL rd, _` with `rd != x0`
  - `JalTail` — `JAL x0, _` (unconditional jump / tail call)
  - `JalrCall` — `JALR rd, _, _` with `rd != x0`
  - `JalrReturn` — canonical `ret` (`JALR x0, x1, 0`); also covers
    compressed `c.jr ra`
  - `JalrIndirect` — other `JALR x0, rs, _` patterns (treated as tail
    call for shadow-stack purposes)

  Costs map to the same numbers as the current `Jal`/`Jalr` unless
  evidence emerges to differentiate. The collector switches on the
  variant directly; no instruction-word re-decoding in the hot path.

- **Acknowledged tech debt**: as more collectors and profiling
  variants land (e.g. `cpu-log`, `syscalls`), inlining everything as
  `Option<...>` checks will get ugly. The current pattern is fine for
  m1's three collectors. When the second wave of collectors arrives,
  expect to revisit the run-loop architecture (callback-driven? code
  generation? specialized loops?) — but defer that decision to when
  there's a concrete second wave to design against, not now.

**Implications for milestones:**
- `InstClass` extension is part of m1's CPU collector work.
- `run_inner_fast` and `run_inner_logging` get expanded match arms
  for the new variants in m1.
- No collector-trait dispatch on the per-instruction hot path. The
  `Collector` trait drives lifecycle (start/stop/flush) and gate
  callbacks on perf events; per-instruction work goes through the
  inline `Option<...>` pattern.

### Q10: Final milestone — what does cleanup/validation look like?

**Resolved: end-to-end validation, synthetic A/B regression catch,
mode-state-machine validation, fresh documentation home at
`docs/design/native/fw-profile/`, scaffolding removal.**

User direction: "the latter for docs, and yes this feels about right."

**End-to-end sanity validation:**
- `fw-profile --collect cpu` on `examples/basic` (rainbow.shader, 241
  LEDs). Examine `report.txt` — top hotspots plausible?
- Open speedscope file in browser. Sanity-check flame chart: does the
  call structure match the engine's known frame loop shape? No
  obviously-broken stacks (orphans, impossible recursion).
- `fw-profile --collect cpu,alloc` — both outputs land in the same
  trace dir, both reports section into the same `report.txt`.
- `fw-profile --collect events` (events-only) — produces the minimal
  "just events" trace dir shape that m3's hardware path will mirror.

**Synthetic A/B validation (highest-value test):**
- Revert the 2026-04-10 "`O(n)` lookup × 240 LEDs per frame" fix
  locally on a branch.
- `fw-profile --diff` on reverted state vs fixed state.
- The diff report must clearly identify the relevant function as the
  regression site, with cycle delta proportional to workload. If it
  doesn't, the tool isn't done — design flaw worth catching here, not
  in production use.

**Mode-state-machine validation:**
- Each `ProfileMode` variant captures its intended window:
  - `--mode compile` excludes frame work
  - `--mode steady-render` excludes compile work
  - `--mode startup` includes both
  - `--mode all` includes everything
- Gate's `Stop` action terminates the run cleanly (no hang).

**Documentation:** new comprehensive home at
`docs/design/native/fw-profile/`. Sections:
- When to use each mode.
- How to read the outputs (`report.txt`, speedscope, diff).
- Common pitfalls ("compile cycles dwarf render — use
  `--mode steady-render`").
- `--cycle-model uniform` as a sanity check.
- **What this tool does NOT measure**: ICache, branch-mispredict,
  PSRAM latency, real wall-clock. Set expectations explicitly.
- Cross-reference to existing `docs/design/native/perf-report/` as
  historical context.

**Scaffolding removal:**
- `dbg!` / temporary println from earlier milestones.
- Placeholder TODOs invalidated by later milestones (e.g. the
  "<jit:0x...>" anonymous symbol fallback once m4 lands).
- Intermediate test fixtures promoted to integration tests or
  deleted.

**Deferred (could be a follow-up):**
- Committed baseline trace (`traces/baseline-basic-steady-render/`)
  for `fw-profile --diff baseline` and CI regression-gate use. Cheap
  but not strictly needed for m5 to ship.

**Implications for milestones:**
- m5 is mostly verification, doc-writing, and small cleanups. No
  feature work.
- The synthetic A/B test should be scripted / committed so it can be
  re-run on demand and ideally in CI.

---

## All questions resolved

Q1-Q10 are fully answered. Ready to draft `overview.md` and the
individual milestone files (m0-m5), then `decisions.md`.

### Naming finalization (post-Q10)

After Q10, the user pushed for git-style single-word commands. Final
naming, used in `overview.md` and all milestone files:

- `fw-profile` → `lp-cli profile`
- `fw-diff` → `lp-cli profile diff` (sub-subcommand under `profile`)
- `cpu-profile` (any earlier mentions) → `lp-cli profile --collect cpu`
- All other concepts (`Collector`, `ProfileSession`, `ProfileMode`,
  trace dir layout, file names) unchanged.

References to old names in Q1–Q10 above are preserved as historical
record of how the design evolved; canonical names live in
`overview.md` and the milestone files.
