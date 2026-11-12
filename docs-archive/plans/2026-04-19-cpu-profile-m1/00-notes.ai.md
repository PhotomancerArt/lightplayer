# CPU Profile m1 — Perf-Event System + ProfileMode — Notes

## Scope

Implement Milestone 1 (`docs/roadmaps/2026-04-19-cpu-profile/m1-perf-events.md`):
the perf-event substrate and `ProfileMode` gate on top of the m0 foundation.

Standalone deliverable:
`lp-cli profile examples/basic --collect events --mode steady-render` produces
a perf-event timeline of the steady-state render. No CPU attribution yet
(that's m2).

Concretely m1 ships:

- **New `lp-base/` workspace directory** for cross-cutting foundational
  crates (no group prefix on contents — the prefix-free naming *is*
  the convention's signal). Brief `lp-base/README.md` documenting the
  rule. Future home of the eventual `lpfs` extraction.
- **`lp-base/lp-perf` crate** — `#![no_std]`, tiny. Macros
  `emit_begin!`/`emit_end!`/`emit_instant!` that compile to no-ops by
  default. Canonical event-name constants (`EVENT_FRAME`,
  `EVENT_SHADER_COMPILE`, `EVENT_SHADER_LINK`, `EVENT_PROJECT_LOAD`,
  …). `PerfEventKind` enum.
  - **Sink selection is build-time via cargo features**, not runtime
    dispatch — closer to `defmt` than to `log`/`tracing`. No
    `OnceLock`, no global init, no `set_sink()`.
  - Features: `syscall` (rv32, ECALL with `SYSCALL_PERF_EVENT` —
    deps `lp-riscv-emu-shared` for the constant), `log` (host-side
    dev/native — deps `log` crate). Default = noop.
- **Engine emission points** for `frame`, `shader-compile`,
  `project-load` — `lp-engine` deps `lp-perf` (no features), call sites
  use macros so non-profile builds emit zero instructions. Final list
  refined during impl. `shader-link` location see Q4-revised below.
- **`SYSCALL_PERF_EVENT`** in `lp-riscv-emu-shared` (ABI: `(name_ptr,
  name_len, kind)` with room reserved for a 4th `arg: u32`). Sole
  consumer: `lp-perf`'s syscall sink.
- **Reserved** `SYSCALL_JIT_MAP_LOAD`, `SYSCALL_JIT_MAP_UNLOAD` constants
  (m5 implements; reserve now to avoid number collisions).
- **fw-emu wiring**: `fw-emu` deps `lp-perf` with `features =
  ["syscall"]`. No `EmuPerfSink` source file needed — the syscall sink
  lives inside `lp-perf/src/sinks/syscall.rs`. fw-esp32 deps `lp-perf`
  with `features = ["log"]` (or default noop).
- **Syscall handler** in `lp-riscv-emu/src/emu/emulator/run_loops.rs`
  reading the name string from guest memory, stamping host
  `cycle_count`, dispatching to `ProfileSession::on_perf_event`.
- **`PerfEvent` host type** in `lp-riscv-emu/src/profile/perf_event.rs`
  (cycle, name, kind) — replaces the m0 stub.
- **`EventsCollector`** in `lp-riscv-emu/src/profile/events.rs` —
  `Collector` impl writing `events.jsonl`. Adds `clock_source:
  "emu_estimated"` to `meta.json`.
- **`ProfileMode` enum** in `lp-cli/src/commands/profile/mode.rs` with four
  parameter-less variants (`SteadyRender`, `Compile`, `Startup`, `All`)
  and a `GateAction { Enable, Disable, NoChange, Stop }` enum. Each
  variant is a small state machine over the perf-event stream.
- **Mode → session wiring**: `ProfileSession` takes a closure /
  trait-object gate that consumes events and returns `GateAction`;
  `Stop` propagates back to the run loop and terminates cleanly.
- **CLI surface expansion** (additive, with one removal):
  - `--collect` accepts `events` (still accepts `alloc`).
  - `--mode {steady-render|compile|startup|all}` (default
    `steady-render`).
  - `--max-cycles N` safety cap.
  - `--frames` removed (mode handles termination).
- **Profile dir name** now includes the mode segment.
- **Fix m0 frame-driving bug**: `advance_time(40)` is only a clock
  bump, not an emulation step. m0's `for i in 0..frames {
  advance_time(40) }` never gives the firmware a tick to run the
  frame. Harmless for alloc trace (interesting allocs all happen
  during `project_load`, which drives the emulator via the transport)
  but blocks any steady-state profiling. m1 rewrites the workload
  loop to actually drive frames via `run_until_yield_or_stop`.
- **Tests**: per-mode state-machine unit tests, `EventsCollector` write
  test, end-to-end integration test for `--collect events --mode
  steady-render`.

Out of scope (downstream milestones):

- `CpuCollector`, `InstClass` extension, shadow stack, hot-path
  attribution, speedscope, `--cycle-model` — m2.
- `--diff [PATH]` flag and functional `profile diff` — m3.
- `HardwarePerfSink` and console parser — m4.
- `SYSCALL_JIT_MAP_LOAD` impl + symbolizer — m5.
- Per-event `arg: u32` payload (ABI room reserved).
- `--raw-events` opt-in.
- `docs/design/native/fw-profile/` doc home — m6.

## Current state

### What m0 left in place

- `lp-riscv-emu/src/profile/mod.rs` ships:
  - `Collector` trait with `on_perf_event(&mut self, _evt: &PerfEvent)`
    (default no-op) and `on_instruction(...)` (default no-op).
  - **Stub** `pub struct PerfEvent {}` (m1 fills in).
  - **Stub** `pub struct InstClass {}` (m2 fills in).
  - `ProfileSession` with `dispatch_syscall`, `finish` (returns
    `Vec<(String, u64)>`), and `meta.json` writing (top-level shared
    fields + per-collector `collectors.<name>` block).
  - `SyscallAction { Pass, Handled, Halt(HaltReason) }` and `HaltReason {
    Oom { size } }`. `HaltReason` will need `ModeStop` (or similar) in
    m1.
  - `EmuCtx`, `FinishCtx`, `SessionMetadata` (with `clock_source:
    &'static str`, currently always `"emu_estimated"`, and
    `frames_requested: u32`).
- `lp-riscv-emu/src/profile/alloc.rs` — `AllocCollector` impl. No
  changes required other than possibly honouring gate state (TBD).
- `Riscv32Emulator` carries `Option<ProfileSession>`, with
  `with_profile_session(...)` and `finish_profile_session()` builders.
- `lp-cli/src/commands/profile/`:
  - `args.rs`: `--collect alloc` (default), `--frames N` (default 10),
    `--note STR`. Sub-subcommand `diff` is a stub.
  - `handler.rs`: builds fw-emu with feature `profile`, loads ELF,
    constructs `AllocCollector`, drives the workload via
    `LpClient::project_load` and `advance_time(40)` × frames.
  - `diff_stub.rs`: prints message + exits 2.
- `cargo` feature `profile` exists in `fw-emu` and `lp-riscv-emu-guest`
  (renamed from `alloc-trace`).
- Trace dir layout: `profiles/<timestamp>--<workload>[--<note>]/`
  containing `meta.json`, `heap-trace.jsonl` (when alloc collector
  ran), `report.txt`.

### Engine surfaces relevant to emission

- `lp-engine` is `#![no_std]` always. Public surface includes
  `ProjectRuntime`, `LpGraphics`, `LpShader`, `NodeInitContext`,
  `RenderContext`, …
- **`ProjectRuntime::tick(&mut self, delta_ms: u32)`** is the per-frame
  entry point — exact wrap site for `frame` Begin/End.
- **`ProjectRuntime::new`** (called from `lp-server::project::Project::new`
  → `LpServer::new`) calls `load_from_filesystem(...)`.
- **`ProjectRuntime::load_nodes` + `init_nodes`** discover and
  initialize nodes — together they form the back half of
  "project-load". In `lp-cli profile`'s flow today, `lp-server` triggers
  these via `client.project_load(...)` over the serial transport, so
  they fire *inside the guest*.
- **`ShaderRuntime::compile_shader`** calls `LpGraphics::compile_shader`
  — exact wrap site for `shader-compile` Begin/End. Also re-entered from
  `update_config` and `handle_fs_change` paths.
- **`lpvm-native::link_jit`** is called inside `lpvm-native::rt_jit::
  compiler::compile`, behind the `LpGraphics::compile_shader` boundary.
  No `lp-engine`-shaped sink reaches there today (lpvm-native does not
  depend on lp-engine).
- `fw-emu` constructs `LpServer::new(...)` in
  `lp-fw/fw-emu/src/main.rs::_lp_main`. That's where any sink injection
  has to be wired.
- `LpServer::new(...)` and `Project::new(...)` and `ProjectRuntime::
  new(...)` all take **6** positional args today; all three signatures
  would need to grow if a sink parameter is plumbed positionally.
  `lp-server` host tests and emu init both pay for that. There are 4
  call sites of `LpServer::new` (3 tests, 1 fw-emu).

### Emulator surfaces relevant to events

- Run loop is in `lp-riscv-emu/src/emu/emulator/run_loops.rs`
  (`run_inner_fast` / `run_inner_logging`). Syscalls dispatch via
  `handle_syscall` (already used by m0 for `SYSCALL_ALLOC_TRACE`).
- `StepResult` (in `emu/emulator/types.rs`) currently has `Continue,
  Syscall, Halted, Trap, Panic, Oom, FuelExhausted`. m1 needs a way
  to tell the higher loop "stop because mode said so" — either a new
  variant (e.g. `ProfileModeStop`) or reuse `Halted`.
- `Riscv32Emulator::advance_time(40)` is the host-side wrapper used
  by `lp-cli profile` to step a frame's worth of cycles. It internally
  loops the run loop until time is up. We need it to bail when the
  emulator returns the stop result.

### Shared crate dependencies

- `lp-shared` (host+rv32 utilities) is already a dep of lp-engine and
  many other crates — a candidate home for a shared sink trait if we
  decide to share between engine and lpvm-native.
- `lps-shared` is shader-side shared types; not a great fit for engine
  perf concepts.

## Resolved questions

### Q1: Trace directory naming — `profiles/` with `--<mode>` segment

**Resolved:** keep `profiles/` from m0; insert `--<mode>` segment after
the workload segment and before the optional `--<note>`.

Final shape: `profiles/<timestamp>--<workload>--<mode>[--<note>]/`.
Mode is always present (default `steady-render`). Roadmap m1 text
(which still says `traces/`) is stale; we don't edit the roadmap, we
just deviate in m1.

This keys m3's `--diff` (no-arg) auto-find logic on the
workload+mode pair embedded in the dir name.

### Q2: Sink injection — superseded by global `lp-perf` crate

**Originally** asked how a `PerfEventSink` instance reaches
`ProjectRuntime` through `LpServer` / `Project`. **Superseded**: there
is no instance to thread. `lp-perf` is a workspace-wide crate with
build-time-selected sinks (cargo features); engine code calls macros
that compile down to a syscall, a `log::trace!`, or nothing depending
on which features the *binary* enables. No runtime dispatch, no
`Rc<dyn …>` storage on runtime types, no constructor changes.

See "Resolved meta-decision" below for the full rationale.

### Q3: `PerfEventSink` shape — superseded by global `lp-perf` crate

**Originally** asked about `&self` vs `&mut self` and ownership of a
sink trait object. **Superseded**: there is no trait at the engine
boundary. The `lp-perf` macros are free functions; their `__emit`
implementation is chosen at build time by cargo features. The host's
syscall sink doesn't even need internal state (it's just an ECALL).
The hardware sink (m4) will use atomics inside the `cfg(feature =
"hw")` impl — no mutable self required.

See "Resolved meta-decision" below.

### Q4-revised: Where does `shader-link` Begin/End get emitted?

**Originally** worried about plumbing a sink into lpvm-native without
an lp-engine ↔ lpvm-native dep. **With `lp-base/lp-perf`**: lpvm-native
deps `lp-perf` directly (cheap; tiny `#![no_std]` crate; no transitive
dep on lp-engine), calls `lp_perf::emit_begin!(EVENT_SHADER_LINK)`
around `link_jit`. Nested cleanly inside `shader-compile`.

**Resolved**: instrument in m1. lpvm-native gains a `lp-perf` dep
(default features) and emits `EVENT_SHADER_LINK` Begin/End around
`link_jit`. The original m1 deferral was driven by the dep-edge
problem, which the global-crate decision eliminates. The
emission-point list is more useful complete from the start, and m5
already has plenty else to do.

### Resolved meta-decision: `lp-base/lp-perf` global crate

**Resolved**: replace the threaded-`PerfEventSink` design with a
workspace-wide `lp-base/lp-perf` crate offering tracing macros whose
implementation is selected at build time via cargo features.

Layout:

```
lp-base/
├── README.md          # "foundational cross-cutting crates;
│                      #  prefix-free naming = no domain owner"
└── lp-perf/
    ├── Cargo.toml     # no_std; features: syscall, log; default = noop
    └── src/
        ├── lib.rs     # macros: emit_begin!/emit_end!/emit_instant!,
        │              # event-name consts, PerfEventKind
        └── sinks/
            ├── noop.rs
            ├── syscall.rs   # cfg(feature = "syscall") — deps
            │                # lp-riscv-emu-shared for SYSCALL_PERF_EVENT
            └── log.rs       # cfg(feature = "log") — uses log crate
```

Cargo wiring:

- `lp-engine` → `lp-perf = { path = "../../lp-base/lp-perf", default-features = false }`
- `lpvm-native` → same (consumed for `shader-link`)
- `fw-emu` → `lp-perf = { path = "../../lp-base/lp-perf", features = ["syscall"] }`
- `fw-esp32` → `lp-perf = { path = "../../lp-base/lp-perf", features = ["log"] }` (or default)

Macro shape (sketch):

```rust
#[macro_export]
macro_rules! emit_begin {
    ($name:expr) => { $crate::__emit($name, $crate::PerfEventKind::Begin) };
}

#[cfg(not(feature = "syscall"))]
#[inline(always)]
pub fn __emit(_name: &'static str, _kind: PerfEventKind) {}

#[cfg(feature = "syscall")]
#[inline(always)]
pub fn __emit(name: &'static str, kind: PerfEventKind) {
    // ECALL SYSCALL_PERF_EVENT, name.as_ptr(), name.len(), kind as u32
}
```

Workspace convention established alongside this crate:

| Dir          | Group prefix      | Notes                                         |
|--------------|-------------------|-----------------------------------------------|
| `lp-core/`   | `lpc-` (planned)  | future rename, out of m1 scope                |
| `lp-shader/` | `lps-`/`lpvm-`/…  | already established                           |
| `lp-fw/`     | `fw-`             | already established                           |
| `lp-riscv/`  | `lpr-` (planned)  | future                                        |
| `lp-base/`   | *none*            | absence of group prefix = cross-cutting infra |
| (root)       | *none*            | apps: `lp-cli`, `lp-app`, `lpfx`              |

**Why this is better than threading**

1. **Eliminates Q2 entirely** — no `Option<Rc<dyn PerfEventSink>>` on
   `ProjectRuntime`/`Project`/`LpServer`, no `with_perf_sink` builder,
   no constructor churn.
2. **Eliminates Q3 entirely** — no `&self` vs `&mut self` debate, no
   threading through `&dyn NodeInitContext`.
3. **Resolves Q4 cleanly** — lpvm-native gets the sink "for free"
   without an lp-engine dep edge.
4. **Better than runtime no-op**: cfg-gated calls compile to *zero
   instructions* in non-profile builds. Threaded sink would always
   pay for `if let Some(sink) = …` even in release.
5. **Idiomatic Rust** — same model as `defmt` (cfg-time sink choice).
   `log`/`tracing` use runtime registration because their binaries
   need to *change* sinks at runtime; ours don't.
6. **Establishes `lp-base/`** as the home for future cross-cutting
   foundational crates (`lpfs` extraction is the obvious next
   inhabitant).

**Caveats / things that still need care**

- **Cycle stamping**: the *guest* doesn't include cycle in the syscall
  payload (it doesn't know host cycle anyway). The host syscall
  handler in `run_loops.rs` stamps `emulator.cycle_count()` when it
  receives the event. Same place we wanted that logic regardless.
- **Mode gating stays host-side** in `ProfileSession`. The guest emits
  unconditionally; the host's gate state machine decides what to
  record. No round-trip needed to push gate state into the guest.
  Slight overhead in the guest (ECALL + 12B payload per emission),
  but only when the `profile` feature is on; non-profile builds emit
  nothing.
- **Event-name string lifetime**: `&'static str` enforced by the macro
  shape — prevents accidental dynamic strings.
- **Sink choice is per-binary**, not per-runtime-invocation. Means a
  single fw-emu binary can't switch between syscall and log sinks at
  runtime. Acceptable: fw-emu only ever runs under the host emulator,
  so syscall is the only sensible sink for it.
- **Unit tests of mode state machines**: instantiate `ProfileSession`
  directly with synthetic `PerfEvent`s; never go through the global
  emit path. End-to-end tests run the CLI (one process per test). No
  test isolation concern.

**Out of scope, deferred**

- `lp-core/* → lpc-*` rename (mechanical workspace-wide refactor;
  separate follow-up).
- `lpfs` extraction into `lp-base/lpfs/` (separate follow-up).

## Open questions to resolve

Each question is presented to the user one at a time in chat with the
suggested course of action. Resolutions are recorded back into the
"Resolved questions" section above with answer + rationale.

### Q1: Trace directory naming — keep `profiles/` from m0, add `--<mode>` segment?

The roadmap m1 file spells the layout as
`traces/<timestamp>--<workload>--<mode>[--<note>]/`, but m0 deliberately
chose `profiles/` (user direction). Mode-name needs to land in the dir
name so m3's `--diff` (no arg) auto-find logic can match by
workload+mode.

**Suggested**: keep `profiles/`, add `--<mode>` after the workload
segment. Final shape:
`profiles/<timestamp>--<workload>--<mode>[--<note>]/`. Mode is always
present (default `steady-render`). Roadmap text is stale; we update m1
notes (not the roadmap) to reflect the kept-from-m0 choice.

### Q5: Default `--collect` and how `events` composes with other collectors

**Resolved**: option (c) — implicit-events for the gate, explicit
selection for output.

- `ProfileSession` always installs an internal event stream and feeds
  every `PerfEvent` through the `ProfileMode` gate, regardless of
  `--collect`. Required for any mode-driven termination.
- The `events` token in `--collect` controls whether `events.jsonl`
  is *persisted to disk*, nothing more.
- Default `--collect` flips from `alloc` (m0) to `events` (m1). No-flag
  invocation `lp-cli profile <project>` writes `events.jsonl` only.
- `--collect alloc` enables alloc only — writes `heap-trace.jsonl`,
  does **not** write `events.jsonl`. Existing m0 invocations
  (`--collect alloc`) keep doing exactly what they did, with no extra
  output files.
- `--collect events,alloc` writes both files.

Mental model stays crisp: "`--collect X` enables collector X's
outputs." The gate is internal and invisible to the user. If we later
find we always want `events.jsonl` alongside any collector for
debuggability, that's a one-line policy flip.

### Q6: What does Enable/Disable do, exactly?

**Resolved**: option (c) — Stop-only in m1, defer real gating to m2.

- `GateAction` enum lands with all four variants (`Enable`, `Disable`,
  `NoChange`, `Stop`) so m2 doesn't need to widen it.
- `ProfileSession` honors `Stop` (propagates to run loop, see Q7) and
  `NoChange` (no-op).
- `Enable` and `Disable` from the mode state machine are recorded as
  `GateTransition` events into the events stream and otherwise have
  **no effect on collectors** in m1. Collectors record from t=0 to
  Stop. The transitions are still useful in `events.jsonl` because
  they document *when the mode would have entered/left the hot
  window*, which is meaningful for inspecting the timeline.
- `AllocCollector` keeps its m0 "always record" behavior. No gating
  added.
- m2's `CpuCollector` is what turns Enable/Disable into real
  `enabled: bool` gating, because that's when the "skip 2 warmup
  frames, capture 4 hot frames" semantics actually matters for sample
  storage cost. m2 also re-evaluates whether `AllocCollector` should
  honor the gate then (with the concrete use case of "what allocated
  during the hot frame?" in hand).

Rationale: m1's only collector that benefits from gating is the one
we *don't want* to gate (events should record fully so the timeline
captures warmup + transitions). Adding gating to alloc speculatively
is a behavioral change without a use case. Less surface area in m1.

### Q7: How does `GateAction::Stop` propagate to the run loop?

**Resolved**: option (a) variant — discrete `ProfileStop` variants in
both `HaltReason` and `StepResult`, plus a *sibling* method
`run_until_yield_or_stop` on `Riscv32Emulator`. Existing
`run_until_yield` signature stays unchanged so
`SerialEmuClientTransport` is untouched.

**Important m0 bug discovered while resolving this — also fixed in
m1**: `advance_time(40)` is *only* a clock bump, not an emulation
step. The m0 workload loop `for i in 0..frames { advance_time(40) }`
never gives the firmware a tick to consume the time deltas — so post-
project-load frames don't actually run. Harmless for m0's alloc trace
(allocations of interest happen *during* project_load, which drives
the emulator via the transport), but blocks any steady-state
profiling. m1 must fix this regardless of Stop semantics, so we fold
the fix into m1.

Concrete shape:

- `HaltReason::ProfileStop` — new variant in
  `lp-riscv-emu/src/profile/mod.rs`, alongside m0's `Oom { size }`.
  Returned by the `SYSCALL_PERF_EVENT` syscall handler when the gate
  yields `Stop`.
- `StepResult::ProfileStop` — new discrete variant in
  `emu/emulator/types.rs`. Run loop translates
  `SyscallAction::Halt(HaltReason::ProfileStop)` into this. Discrete
  rather than folded into `Halted` so error/exit logic can
  distinguish "guest called exit" from "profile mode terminated".
- `Riscv32Emulator::run_until_yield_or_stop(max_steps) ->
  Result<FrameOutcome, EmulatorError>` — new sibling method.
  `FrameOutcome { Yielded(SyscallInfo), ProfileStopped }`. Called by
  the CLI's per-frame work; not by `SerialEmuClientTransport` (which
  keeps using the unchanged `run_until_yield`).
- CLI per-frame work becomes (sketch):
  ```
  loop {
      let outcome = {
          let mut emu = emulator_arc.lock().unwrap();
          emu.advance_time(40);
          emu.run_until_yield_or_stop(MAX_STEPS_PER_FRAME)?
      };
      match outcome {
          FrameOutcome::Yielded(_) => {
              if cycle_count >= max_cycles { warn; break }
              continue;
          }
          FrameOutcome::ProfileStopped => break,
      }
  }
  ```
  This actually drives the firmware (each `run_until_yield_or_stop`
  call lets the firmware execute one server tick to its next
  `sys_yield`). Frames now mean what they say.
- Exit code 0 on `ProfileStopped` and on the `max_cycles` warning.

Why discrete variants over a polled flag (option b): the syscall
handler already returns a `SyscallAction`; piggy-backing on that path
is essentially free, vs. checking a flag every instruction in the
hot run loop. Also keeps "stop is a structured event in the
emulator's state machine" rather than an out-of-band side channel.

### Q8: `--max-cycles` enforcement point

**Resolved**: option (a) — CLI-side check between frames.

- After each `run_until_yield_or_stop` returns `Yielded`, the CLI
  checks `emulator.cycle_count() >= max_cycles`. If yes, emit a
  warning and break out of the workload loop. Same exit path as
  `ProfileStop`.
- Default `--max-cycles 200_000_000` per roadmap.
- Granularity is one frame's worth of cycles (≤ `MAX_STEPS_PER_FRAME`).
  Plenty fine for a safety cap; promote to fuel-based (option b) only
  if we discover a workload that genuinely needs sub-frame
  interruption.
- Exit code **0** — the trace is valid up to the cycle count;
  `--max-cycles` is a soft cap, not an error condition. The warning
  surfaces "did not complete normally" without a non-zero exit, since
  the produced `events.jsonl` is still useful for inspection.
- Warning shape (sketch):
  ```
  warning: --max-cycles 200000000 reached without ProfileMode
           terminating; workload may not exercise the gate condition.
  events recorded: <N>; partial trace written to <path>
  ```
- `max_cycles` value also recorded into `meta.json` (see Q9).

Option (c) (per-event check) ruled out because a runaway workload may
emit zero events; the cap wouldn't trip. Option (b) (run-loop fuel)
ruled out because fuel is per-call in the existing API and shifting
to absolute couples to existing fuel usage non-trivially.

### Q9: `frames_requested` in `SessionMetadata` — replace or keep?

**Resolved**: option (c). Rename + add cycle fields + add
`terminated_by`. `schema_version` stays at 1 for *all* m1 metadata
changes — nothing real consumes `meta.json` yet, and bumping the
version is reserved for true compatibility breaks once external
tooling exists.

Concrete top-level shape after m1 (per-collector `collectors.<name>`
blocks unchanged from m0):

```json
{
  "schema_version": 1,
  "timestamp": "2026-04-19T13:57:32Z",
  "workload": "examples/basic",
  "note": "optional",
  "mode": "steady-render",
  "max_cycles": 200000000,
  "cycles_used": 12345678,
  "terminated_by": "profile_stop",
  "clock_source": "emu_estimated",
  "collectors": { ... }
}
```

Field changes vs m0:

- `frames_requested: u32` → **removed**, replaced by `mode: String`.
- `max_cycles: u64` — **new**, mirrors `--max-cycles`.
- `cycles_used: u64` — **new**, `emulator.cycle_count()` at session
  finish. m3's diff uses this to detect workload drift between
  baseline and current.
- `terminated_by: String` — **new**, one of `"profile_stop" |
  "max_cycles" | "error"`. Useful for tooling and downstream
  analysis.

The single m0 test that asserts on `frames_requested` is updated to
assert on `mode`, `max_cycles`, `terminated_by` instead.

### Q10: Reserved syscall constants — where, and how to mark them?

**Resolved**: add three constants to `lp-riscv-emu-shared/src/
syscall.rs`, typed `i32` to match existing convention. Confirmed
1-9 are taken by m0; 10/11/12 are next-available.

```rust
/// Emit a perf event from guest to host.
/// ABI: a0=name_ptr, a1=name_len, a2=kind (0=Begin, 1=End, 2=Instant).
/// a3 reserved for a future `arg: u32` payload.
pub const SYSCALL_PERF_EVENT: i32 = 10;

/// Reserved for m5 JIT-symbol overlay (load).
/// Not yet implemented; reserving the number to avoid collision in m2-m4.
pub const SYSCALL_JIT_MAP_LOAD: i32 = 11;

/// Reserved for m5 JIT-symbol overlay (unload).
/// Not yet implemented; reserving the number to avoid collision in m2-m4.
pub const SYSCALL_JIT_MAP_UNLOAD: i32 = 12;
```

- No handler in `run_loops.rs` for the JIT ones — they fall through
  to "unknown syscall" naturally if invoked.
- No guest-side helpers in `lp-riscv-emu-guest/src/syscall.rs` for
  the JIT ones — m5 adds those.
- `SYSCALL_PERF_EVENT` does get both a host-side handler (in
  `run_loops.rs`, dispatching to `ProfileSession::on_perf_event`)
  and a guest-side issuer — but the issuer lives in
  `lp-base/lp-perf/src/sinks/syscall.rs` (cfg-gated by the `syscall`
  feature), not in `lp-riscv-emu-guest`. The m5-future helpers
  (`sys_jit_map_load` / `sys_jit_map_unload`) can live wherever m5
  decides; not pre-committing now.
- No top-of-file syscall-range table — per-constant doc comments are
  sufficient context.

### Q11: Mode state-machine numbers — bake in "skip 2, capture 4" now?

**Resolved**: option (b) — `pub const`s alongside each mode's state
machine. `SteadyRender` initial values:

```rust
pub const STEADY_RENDER_WARMUP_FRAMES: u32 = 2;
pub const STEADY_RENDER_CAPTURE_FRAMES: u32 = 4;
```

Same treatment for other modes' magic numbers as they get
implemented (`Compile`, `Startup`; `All` has none).

Benefits:
- Tests reference the constant rather than literal `2` — so when we
  tune the number, tests auto-update with the policy instead of
  silently passing with stale expectations.
- CLI startup banner can print the policy
  (`"steady-render: skip 2, capture 4 frames"`) sourced from these.
- m3's diff report can include the policy alongside the comparison.
- Zero cost — they're already constants, just `pub`.

Roadmap's "no parameters on mode variants" still holds — these are
build-time constants, not runtime knobs. The lever for changing
them is editing source, not a CLI flag.

### Q12: Handler layout — split or stay in one file?

**Resolved**: option (b) — pre-commit to a minimal split now. User
preference for small files.

Target layout under `lp-cli/src/commands/profile/`:

```
profile/
├── mod.rs              # module wiring (small)
├── args.rs             # CLI args (existing; extends in m1)
├── handler.rs          # orchestrator entry point (~150 lines target)
├── mode/               # NEW
│   ├── mod.rs          # ProfileMode enum, GateAction, gate trait
│   ├── steady_render.rs
│   ├── compile.rs
│   ├── startup.rs
│   └── all.rs
├── workload.rs         # NEW — frame-driving loop, max-cycles, stop
├── output.rs           # NEW — meta.json + report.txt writing
└── diff_stub.rs        # existing
```

Responsibilities:
- `handler.rs`: arg validation, profile-dir creation, ELF load,
  emulator construction, `ProfileSession` construction with
  collectors + mode, call `workload::run(...)`, call
  `output::write_report(...)`, exit code logic.
- `mode/`: `ProfileMode` enum, `GateAction { Enable, Disable,
  NoChange, Stop }`, the gate trait/closure shape, and one file per
  mode's state machine (each one-page-ish; testable in isolation).
- `workload.rs`: the per-frame loop that calls
  `advance_time(40)` + `run_until_yield_or_stop`, max-cycles check,
  `FrameOutcome` translation, `terminated_by` tracking.
- `output.rs`: `meta.json` serialization (top-level fields +
  per-collector blocks), `report.txt` formatting (per-collector
  banners established in m0).

`args.rs` stays a single file — already isolated, no need to split.

Cost: ~30 minutes of setup in phase 1. Benefit: m2's `CpuCollector`
integration adds to one file (`mode/` or `workload.rs`) instead of
ballooning a monolith; each component is independently unit-testable;
handler.rs stays scannable as "set up, run, write, exit".
