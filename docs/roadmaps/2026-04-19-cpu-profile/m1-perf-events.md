# Milestone 1: Perf-Event System + ProfileMode

## Goal

Establish the perf-event infrastructure that gates every collector from
m2 onward and serves as the cross-platform substrate for m4's
emu/device correlation. Engine code in `lp-engine` emits named events
at semantic boundaries (`frame`, `shader-compile`, `shader-link`,
`project-load`); `fw-emu` routes them through `SYSCALL_PERF_EVENT` to
an `EventsCollector` that writes `events.jsonl`. `ProfileMode` is a
Rust enum of small state machines that consume the event stream and
decide when profiling is active.

Standalone deliverable: `lp-cli profile --collect events --mode
steady-render` produces a perf-event timeline of `examples/basic`'s
steady-state render. No CPU attribution yet — that's m2.

## Suggested Plan Name

`profile-m1-perf-events`

## Scope

### In scope

- **`PerfEventSink` trait** in new module `lp-engine/src/perf.rs`:

  ```rust
  pub trait PerfEventSink: Send {
      fn emit(&mut self, name: &'static str, kind: PerfEventKind);
  }

  #[repr(u8)]
  pub enum PerfEventKind { Begin = 1, End = 2, Instant = 0 }
  ```

  Plus canonical event-name constants so emitters and consumers agree
  on identifiers:

  ```rust
  pub const EVENT_FRAME: &str = "frame";
  pub const EVENT_SHADER_COMPILE: &str = "shader-compile";
  pub const EVENT_SHADER_LINK: &str = "shader-link";
  pub const EVENT_PROJECT_LOAD: &str = "project-load";
  ```

  A `NoopPerfSink` default impl is provided so consumers without a
  real sink (e.g. fw-esp32 in this milestone, before m4) keep
  building.

- **Initial event emission points** in `lp-engine`:
  - `frame` Begin/End — wrapping the per-frame render loop.
  - `shader-compile` Begin/End — wrapping the LPIR build pipeline
    (parse + optimize + lower + link).
  - `shader-link` Begin/End — wrapping `lpvm-native::link_jit`
    specifically (nested inside `shader-compile`).
  - `project-load` Begin/End — wrapping initial project parse + asset
    load.

  Final emission-point list refined during implementation; the
  architectural commitment is that names are centrally defined in
  `lp-engine` and emission happens at semantically meaningful
  boundaries.

- **Sink injection.** Engine accepts a `Box<dyn PerfEventSink>` (or
  generic) at init. Default is `NoopPerfSink`; `fw-emu` overrides with
  `EmuPerfSink`. Concrete injection mechanism (config struct vs
  generic vs trait object) decided at implementation; the trait
  contract is what's locked.

- **`SYSCALL_PERF_EVENT` constant** in `lp-riscv-emu-shared`. ABI:
  `(name_ptr: u32, name_len: u32, kind: u32)`. Reserve room for a
  future `arg: u32` parameter — handler reads only the three current
  args; future emissions can pass a fourth without breaking the ABI.

- **Reserve future syscall constants** in `lp-riscv-emu-shared`:
  `SYSCALL_JIT_MAP_LOAD`, `SYSCALL_JIT_MAP_UNLOAD`. m5 will implement
  the first; reserving now avoids backward-compat surprises.

- **`EmuPerfSink`** in new file `lp-fw/fw-emu/src/perf_sink.rs`. Calls
  `SYSCALL_PERF_EVENT` from the host-emulation environment. String
  pointers are static `&'static str` so guest memory layout is
  trivial.

- **Syscall handler** in `lp-riscv-emu/src/emu/emulator/run_loops.rs`.
  Reads name string from guest memory (using existing guest-string
  helpers used by other syscalls), constructs a host-side `PerfEvent`,
  dispatches to `ProfileSession::on_perf_event`.

- **`PerfEvent` host-side type** in new file
  `lp-riscv-emu/src/profile/perf_event.rs`:

  ```rust
  pub struct PerfEvent {
      pub cycle: u64,
      pub name: String,
      pub kind: PerfEventKind,
  }
  ```

- **`EventsCollector`** in new file
  `lp-riscv-emu/src/profile/events.rs`. Implements `Collector` from
  m0. `on_perf_event` appends to `events.jsonl`:

  ```json
  {"t": 12345678, "n": "frame", "k": "B"}
  ```

  where `t` is `cycle_count`, `k` is `B`/`E`/`I`. `meta.json` gains
  `"clock_source": "emu_estimated"` (locks the schema for m4's
  hardware ingestion).

- **`ProfileMode` enum** in new file
  `lp-cli/src/commands/profile/mode.rs`:

  ```rust
  pub enum ProfileMode { SteadyRender, Compile, Startup, All }

  pub enum GateAction { Enable, Disable, NoChange, Stop }
  ```

  Each variant implements `fn observe(&mut self, evt: &PerfEvent) ->
  GateAction`. Hardcoded behavior, no parameters:
  - `SteadyRender`: wait for first `shader-compile` End, then skip 2
    `frame` pairs, then capture next 4 `frame` pairs, then `Stop`.
  - `Compile`: enable on first `shader-compile` Begin, disable on its
    matching End, then `Stop`.
  - `Startup`: active from t=0; disable on first `frame` End, then
    `Stop`.
  - `All`: always active until external cap.

- **Mode → emulator wiring.** `lp-cli` constructs the `ProfileMode`
  state machine and passes it to the emulator builder. The session
  keeps a `Box<dyn FnMut(&PerfEvent) -> GateAction>` and consults it
  on every perf event. `GateAction::Stop` propagates through
  `ProfileSession` to the run loop, terminating the run cleanly.

- **CLI surface expansion** (additive over m0):

  ```
  lp-cli profile [DIR=examples/basic]
                 [--collect events]                       # default; cpu/alloc still valid
                 [--mode {steady-render,compile,startup,all}=steady-render]
                 [--note STR]
                 [--max-cycles N=200_000_000]             # safety cap
  ```

  `--frames` from m0 removed (mode handles termination).

- **Trace dir name now includes mode**:
  `traces/<timestamp>--<workload>--<mode>[--<note>]/`. m3's `--diff`
  (no arg) lookup will key on the workload+mode portion of the dir
  name.

- **Tests.**
  - Unit test per `ProfileMode` variant: feed synthetic event
    sequences, assert expected `GateAction` transitions, including
    edge cases (e.g. `SteadyRender` with no `shader-compile` event
    ever observed → never enables, eventually hits `--max-cycles`).
  - Unit test for `EventsCollector`: synthetic event sequence
    produces expected `events.jsonl` output.
  - Integration test: `lp-cli profile examples/basic --collect events
    --mode steady-render` produces a valid `events.jsonl` containing
    the expected `frame`/`shader-compile`/etc events in plausible
    order.

### Out of scope

- `CpuCollector`, `InstClass` extension, shadow stack, hot-path
  attribution — all m2.
- Speedscope / `cpu-profile.json` writers — m2.
- `--cycle-model` flag — m2.
- `--diff [PATH]` and `lp-cli profile diff` impl — m3 (stub from m0
  stays).
- `HardwarePerfSink` and device console parser — m4.
- Per-event `arg: u32` payload — deferred (ABI room reserved).
- JIT symbol overlay — m5.
- `--raw-events` opt-in mode — deferred follow-up.

## Key Decisions

- **Engine owns emission, not firmware shell.** Same emission points
  fire on emu and on real hardware (once m4 lands). This is what
  makes the cross-platform correlation work: identical event stream
  from identical engine code paths.

- **Stringly-typed event names with central constants.** Cheaper than
  enums (no shared crate dependency between `lp-engine` and tools);
  drift mitigated by `pub const EVENT_*: &str` in `lp-engine` so all
  call sites reference the same identifier.

- **`ProfileMode` carries no parameters.** Hardcoded behavior per
  variant — `SteadyRender` knows internally it skips 2 frames and
  captures 4. If a different policy is needed, edit the variant or add
  a new one. Audience is project developers; this is the right
  ergonomic shape.

- **`GateAction::Stop` is a first-class action.** Cleaner than a
  separate "should-stop" boolean. The mode state machine speaks for
  the whole session.

- **No `--warmup` flag.** Mode state machines fully encode warmup
  behavior. If a different warmup is needed, change the mode (or add
  a variant).

- **ABI room reserved for `arg: u32` on syscall.** Handler reads three
  args today; emission can pass a fourth later without ABI break.

- **Reserve `SYSCALL_JIT_MAP_LOAD/UNLOAD` numbers now.** m5
  implements; reserving in m1 avoids any chance of m2-m4 grabbing the
  same numbers.

## Deliverables

### `lp-engine` crate
- New: `lp-engine/src/perf.rs` — `PerfEventSink` trait,
  `PerfEventKind`, `NoopPerfSink`, canonical event-name constants.
- Updated: engine entry points / render loop / shader-compile pipeline
  / project-load pipeline — `sink.emit(...)` calls at semantic
  boundaries. Sink injected via existing engine-config or
  dependency-injection path.

### `lp-riscv-emu-shared` crate
- New constant: `SYSCALL_PERF_EVENT`.
- Reserved constants: `SYSCALL_JIT_MAP_LOAD`, `SYSCALL_JIT_MAP_UNLOAD`
  (with `// reserved for m5` comment).

### `lp-riscv-emu` crate
- New: `lp-riscv-emu/src/profile/perf_event.rs` — `PerfEvent`
  host-side type.
- New: `lp-riscv-emu/src/profile/events.rs` — `EventsCollector`
  implementing `Collector`.
- Updated: `lp-riscv-emu/src/profile/mod.rs` — `ProfileSession` gains
  `on_perf_event` dispatch + gate-action handling + `Stop` plumbing.
- Updated: `lp-riscv-emu/src/emu/emulator/run_loops.rs` —
  `SYSCALL_PERF_EVENT` handler.
- Updated: `lp-riscv-emu/src/lib.rs` — export `PerfEvent`,
  `EventsCollector`, `GateAction`.

### `fw-emu` crate
- New: `lp-fw/fw-emu/src/perf_sink.rs` — `EmuPerfSink` impl.
- Updated: `lp-fw/fw-emu/src/main.rs` — wire `EmuPerfSink` into the
  engine's `PerfEventSink` slot during init.

### `lp-cli` crate
- New: `lp-cli/src/commands/profile/mode.rs` — `ProfileMode` enum,
  state machines, gate closure builder.
- Updated: `lp-cli/src/commands/profile/args.rs` — `--mode`,
  `--max-cycles` flags. `--collect` accepts `events` (and still
  `alloc` from m0). `--frames` removed.
- Updated: `lp-cli/src/commands/profile/handler.rs` — instantiates
  `EventsCollector` when `events` in `--collect`; passes mode closure
  to emulator; honors `Stop`.

### Tests
- Unit tests for each `ProfileMode` state machine.
- Unit test for `EventsCollector` write path.
- Integration test for `lp-cli profile --collect events --mode
  steady-render`.

## Dependencies

- m0 — Foundation refactor must be complete (`Collector` trait,
  `ProfileSession`, trace dir layout, `lp-cli profile` command).

## Validation

```bash
# Workspace builds (lp-engine sink injection touches multiple consumers)
cargo build --workspace

# Unit tests
cargo test -p lp-engine
cargo test -p lp-riscv-emu
cargo test -p lp-cli

# fw-esp32 still builds (uses NoopPerfSink default until m4)
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf \
  --profile release-esp32 --features esp32c6,server

# End-to-end: produce a perf-event timeline of examples/basic
cargo run -p lp-cli --release -- profile examples/basic \
  --collect events --mode steady-render
# Expected outputs in traces/<sess>/:
#   meta.json (with clock_source="emu_estimated")
#   events.jsonl (frame/shader-compile/shader-link/project-load events,
#                 only those between gate Enable and gate Disable)
#   report.txt (with "Events summary": event counts, mode, total cycles)

# Mode behavior check
cargo run -p lp-cli --release -- profile examples/basic \
  --collect events --mode compile
# Trace dir name contains "--compile"; events.jsonl shows shader-compile
# region only.

# m0 regression: alloc still works
cargo run -p lp-cli --release -- profile examples/basic --collect alloc
# heap-trace.jsonl produced as before.
```

## Estimated Scope

- New code: ~800-1100 LOC across `lp-engine` (~150),
  `lp-riscv-emu-shared` (~30), `lp-riscv-emu` (~300), `fw-emu` (~80),
  `lp-cli` (~400-500).
- Tests: ~300-400 LOC.
- Files touched: ~15-20.

## Agent Execution Notes

This milestone is suitable for one agent session, possibly two. Work
in this order to keep each step testable:

1. Read `lp-engine`'s current entry points to identify where the four
   event boundaries naturally sit. Check `lp-engine`'s public API for
   how config / DI is currently done so sink injection fits naturally.
2. Add `PerfEventSink` trait + `NoopPerfSink` + canonical name
   constants in `lp-engine/src/perf.rs`. Confirm workspace still
   builds.
3. Sprinkle `sink.emit(...)` calls at the four event boundaries.
   Confirm workspace still builds with `NoopPerfSink`.
4. Add `SYSCALL_PERF_EVENT` constant to `lp-riscv-emu-shared`.
   Reserve `SYSCALL_JIT_MAP_LOAD/UNLOAD`.
5. Implement `EmuPerfSink` in `fw-emu` and wire it into engine init.
   Confirm fw-emu builds.
6. Add syscall handler in `run_loops.rs`. Confirm a synthetic
   syscall from a small test produces a `PerfEvent` on the host
   side.
7. Implement `EventsCollector`. Wire into `ProfileSession`'s
   collector list when `events` is in `--collect`. Test with a
   contrived run.
8. Implement `ProfileMode` state machines as pure functions over
   event sequences. Heavy unit tests here — these are the trickiest
   logic in the milestone.
9. Wire mode closure → emulator via the session builder. Plumb
   `GateAction::Stop` through `ProfileSession` to the run loop.
10. End-to-end test: `lp-cli profile examples/basic --collect events
    --mode steady-render`. Inspect `events.jsonl`. Verify gate timing.
