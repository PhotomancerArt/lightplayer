# profile — Unified Firmware Profiling Toolkit — Overview

## Motivation

Performance work on `fw-esp32` has hit a wall: changes that *should*
improve performance aren't producing the expected gains, and we can't
tell why. Existing tooling — `mem-profile` for heap, ad-hoc cycle
counters for time — is split across narrow commands and doesn't answer
the question that actually matters:

> "I changed X expecting Y improvement. Did Y happen, and if not, where
> did the cycles actually go?"

This roadmap delivers a unified profiling system built around the
existing `lp-riscv-emu` cycle model. It produces per-function cycle
attribution (callgrind-style data + flame chart), composes with
allocation tracing, supports A/B diffs against prior runs, and crucially
lays the foundation for **emu-vs-device correlation**: the same perf
events emitted by the engine on emulator and on real hardware,
allowing us to verify that emu-side optimization wins translate to
device wins.

A secondary outcome is a clean refactor: `mem-profile` and
`heap-summary` get folded into the new unified `profile` command,
eliminating the artificial split between profiling axes.

## Key Design Decisions

### 1. One unified command: `lp-cli profile`

Replaces `mem-profile`, `heap-summary`, and the original proposed
`cpu-profile` / `cpu-summary` / `cpu-diff` trio. Memory and CPU
profiling are the same task with different observation axes; one
command, with composable collectors, is the right shape. Single-word
subcommand following git/cargo/kubectl convention.

```
lp-cli profile [DIR=examples/basic]
               [--collect cpu]
               [--mode steady-render]
               [--diff [PATH]]
               [--cycle-model esp32c6]
               [--note STR]
               [--max-cycles N]

lp-cli profile diff <trace-dir-a> <trace-dir-b>
                    [--top N=20]
                    [--threshold-pct F=1.0]
                    [--threshold-cycles N=1000]
```

`mem-profile` and `heap-summary` are removed (no production users to
migrate).

### 2. Collectors — composable, share one gate

Each "thing being measured" is a `Collector` trait implementation:
`cpu`, `alloc`, `events` initially; `cpu-log`, `syscalls`, `ir-stats`
slot in later without command-surface churn. All enabled collectors
share the same mode/gate state machine, so "what allocated during the
hot frame?" becomes a join on `cycle_count`, not a guess.

### 3. Call/return detection via instruction-shape, not frame pointers

The CPU collector reconstructs the call stack by inspecting `JAL` /
`JALR` encodings (`rd != x0` is a call, `JALR x0, x1, 0` is a return,
`JAL x0, _` is a tail call). Host-side shadow stack mirrors the guest's
call structure. Equivalent to Valgrind's callgrind data model.

Chosen over FP-walk sampling for exact call counts (signal we'd lose
with sampling), lower per-event cost, and cleaner diff fidelity.
Instruction-shape detection happens at decode time via new
`InstClass::JalCall` / `JalTail` / `JalrCall` / `JalrReturn` /
`JalrIndirect` variants. lpvm-native's frame-pointer discipline is
preserved (already true) and remains a viable debug fallback if needed.

### 4. Perf-event system in `lp-engine`, with two sinks

Engine code emits named events (`frame`, `shader-compile`,
`shader-link`, `project-load`) via a `PerfEventSink` trait. Two
implementations:

- **`EmuPerfSink`** — calls a syscall, recorded into `events.jsonl`.
- **`HardwarePerfSink`** — prints `[perf] <cycles> <name> <kind>` to
  console using the hardware cycle counter (e.g. `mcycle` on
  ESP32-C6).

Same engine code, same event vocabulary, both targets. This is the
foundation for emu-vs-device correlation: console output from a device
run is parsed back into the same `events.jsonl` shape.

### 5. Profile mode is a Rust enum, not a DSL

```rust
pub enum ProfileMode { SteadyRender, Compile, Startup, All }
```

Each variant is a small state machine over the perf-event stream.
Hardcoded behavior, no parameters — `SteadyRender` knows internally to
"wait for first `shader-compile` end, skip 2 `frame` pairs, capture
next 4, then stop." Adding a mode = adding a variant. Audience is
project developers; this is the right ergonomic shape.

### 6. Three outputs per CPU-collector run

In every trace dir produced by a CPU-collector run:

- **`report.txt`** — short top-N text summary, printed to stdout *and*
  written to disk. Agent / CI-readable.
- **`cpu-profile.speedscope.json`** — flame chart. Drag-and-drop into
  https://speedscope.app, no install.
- **`cpu-profile.json`** — canonical aggregated callgrind-style dump.
  Schema-versioned. The diff source of truth.

Plus `events.jsonl` (always when any collector runs) and
`alloc-trace.jsonl` (when alloc collector runs). Folded-stack and
callgrind text formats explicitly out of scope.

### 7. JIT'd shader code symbolized via `SYSCALL_JIT_MAP_LOAD`

The JIT runtime tells the host about each linked module's name table.
Host maintains a dynamic-symbol overlay alongside ELF symbols. Wire
format reserves room for an `_UNLOAD` syscall (timestamped on
`cycle_count` for future interval lookup) but only `LOAD` is
implemented in the JIT-symbols milestone. Bonus payoff: panic
backtraces and alloc-trace also get real shader function names.

### 8. Cycle model: `Esp32C6` as-is, refine only with empirical data

Existing per-class costs (Alu=1, Load=2, BranchTaken=2, Jal=2, Jalr=3,
DivRem=32, System=4, Fence=4, Atomic=4) used unchanged.
`--cycle-model {esp32c6,uniform}` flag for one-line sanity checks.
Refinements happen in m3 (the hardware-correlation milestone), driven
by measured emu-vs-device divergence — never by intuition about
microarchitecture.

## Alternatives Considered

- **`fw-host` with native Rust profilers**: rejected. Can't accurately
  profile the JIT'd RV32 machine code that dominates rainbow.shader's
  hot path; ISA and architectural differences invalidate host-side
  cycle attribution.
- **FP-walk stack sampling**: rejected as primary. Loses exact call
  counts (the 2026-04-10 perf bug — "O(n) lookup × 240 LEDs per frame"
  — would have appeared as "dominates samples" without the count).
  lpvm-native does maintain frame pointers, so this remains a viable
  debug fallback if needed.
- **Streaming raw events (per-instruction or per-call)**: deferred
  behind reserved `--raw-events` flag. Aggregated callgrind data is
  sufficient for flame charts and diffs at fraction of the volume.
- **Predicate DSL for profile gating**: rejected. Named modes in a
  Rust enum are clearer for the project-developer audience and
  trivially extensible.
- **Always-on alloc tracing during cpu-profile**: rejected.
  Composable opt-in via `--collect cpu,alloc` keeps tight CI loops
  cheap.
- **`cpu-profile` / `mem-profile` separate commands**: rejected during
  planning. They're the same shape; unifying behind `profile` removes
  accidental complexity.
- **`perf` or `fw-profile` as command name**: rejected. `perf` invites
  Linux-perf mental models that don't apply (sampling, hardware
  counters); `fw-profile` adds a dash to a command typed all day with
  no namespace benefit.

## Risks

- **m1 scope expanded**: perf-event system was originally going to be
  a separate milestone but is now in m1 as baseline. Estimated
  complexity is significant but bounded; user accepted "I don't mind
  it taking a little longer."
- **`mem-profile` removal**: clean cut (no production users), but any
  internal scripts referencing it will need updates as part of m0.
- **Run loop tech debt**: per-instruction inline `Option<&mut
  Collector>` checks scale poorly past ~3 collectors. Acknowledged as
  deferred; revisit when the second wave of collectors arrives, not
  now.
- **JIT symbol staleness during hot-reload**: m4 ships only `LOAD`
  (not `_UNLOAD`). If hot-reload lands before `_UNLOAD` is
  implemented, stale symbols could shadow new modules. Mitigated by
  asserting current shader pipeline doesn't unload.
- **Cycle-model accuracy unverified**: `Esp32C6` model is hand-tuned,
  not validated against real hardware. m3's correlation report is
  what closes this gap — until then, we're trusting that A/B *deltas*
  are meaningful even if absolutes are off.

## Milestones

- **[m0 — Foundation refactor](./m0-foundation.md).** `Collector` trait,
  unified `ProfileSession`, unified trace dir layout. Port `alloc-trace`
  into the new shape. `lp-cli profile --collect alloc` ships at parity
  with old `mem-profile`. `mem-profile` / `heap-summary` removed. *No
  new functionality; pure restructuring.*
- **[m1 — Perf-event system + ProfileMode](./m1-perf-events.md).**
  `PerfEventSink` trait in `lp-engine` + `EmuPerfSink` in `fw-emu` +
  initial event emission points (`frame`, `shader-compile`,
  `shader-link`, `project-load`). `SYSCALL_PERF_EVENT`.
  `EventsCollector` writes `events.jsonl`. `ProfileMode` enum + state
  machines, `--mode` flag. Standalone deliverable: `lp-cli profile
  --collect events --mode steady-render` produces a perf-event
  timeline. Prerequisite for both m2 (CPU) and m4 (Hardware).
- **[m2 — CPU collector + outputs](./m2-cpu-collector.md).**
  `InstClass` extension with call/return-aware variants. `CpuCollector`
  (shadow stack + callgrind data model). Per-instruction hot-path
  integration. Speedscope JSON writer + canonical `cpu-profile.json`
  writer + CPU section in `report.txt`. `--cycle-model` flag.
  Standalone deliverable: `lp-cli profile --collect cpu` produces the
  full flame chart.
- **[m3 — Diff](./m3-diff.md).** `lp-cli profile diff` standalone
  subcommand. `--diff [PATH]` flag on `profile` (auto-find most recent
  matching prior run). Threshold flags. Default sort: regressions
  first.
- **[m4 — Hardware perf-log + correlation](./m4-hardware-correlation.md).**
  `HardwarePerfSink` for `fw-esp32`. `lp-cli` ingests device console
  output into a device trace dir of the same shape. `perf-log-diff`
  mode. Four-corner correlation report. Cycle-model refinements (if
  any) live here, driven by data.
- **[m5 — JIT symbols](./m5-jit-symbols.md).** `SYSCALL_JIT_MAP_LOAD`
  + symbolizer overlay in `lp-riscv-emu`. Dynamic symbols merged with
  static ELF symbols at attribution time. Reserve `_UNLOAD` syscall
  number, don't implement.
- **[m6 — Cleanup / validation](./m6-validation-docs.md).** End-to-end
  runs against `examples/basic`. Synthetic A/B suite. Workflow doc at
  `docs/design/native/fw-profile/`. Scaffolding removal.

## Dependencies

- `lp-riscv-emu` cycle model (existing, unchanged).
- `alloc-trace` machinery (existing, refactored into Collector trait
  in m0).
- `BinaryBuildConfig::with_backtrace_support` (existing).
- ELF symbol loading via `lp-riscv-elf::load_elf` (existing).
- `examples/basic` workload (existing, used for validation).
- No external dependencies beyond what's already in the workspace.
