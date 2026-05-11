# m1 — perf events — summary

### What was built

- New top-level workspace directory `lp-base/` for cross-cutting foundational crates; documented prefix-free naming convention (`lp-perf`, future `lpfs`, etc.).
- New `lp-base/lp-perf` crate with `#![no_std]` macros (`emit_begin!`, `emit_end!`, `emit_instant!`), event-name constants (`EVENT_FRAME`, `EVENT_SHADER_COMPILE`, `EVENT_SHADER_LINK`, `EVENT_PROJECT_LOAD`), `PerfEventKind`, and three build-time selectable sinks: `noop` (default), `syscall` (RV32 ECALL with host stub), `log`.
- New syscall constant `SYSCALL_PERF_EVENT = 10` in `lp-riscv-emu-shared` (re-exported from crate root). Reserved `SYSCALL_JIT_MAP_LOAD = 11` and `SYSCALL_JIT_MAP_UNLOAD = 12` for m5.
- Engine instrumentation: `EVENT_FRAME` wraps `ProjectRuntime::tick`; `EVENT_PROJECT_LOAD` wraps `ProjectRuntime::new`; `EVENT_SHADER_COMPILE` wraps `ShaderRuntime::compile_shader` (split into `compile_shader_inner`); `EVENT_SHADER_LINK` wraps `link_jit` in `lpvm-native`.
- Firmware wiring: `fw-emu` enables `lp-perf/syscall`; `fw-esp32` defaults to noop.
- Host profile types in `lp-riscv-emu/src/profile/`: `PerfEvent`, `PerfEventKind` (with Chrome-trace-style `as_str()` returning `"B"`/`"E"`/`"I"`), `intern_known_name()` over `KNOWN_EVENT_NAMES`, `EventsCollector` writing `events.jsonl` (one `{cycle, name, kind}` JSON object per line), `Gate` trait, `GateAction` enum, `HaltReason::ProfileStop`, `ProfileSession::{set_gate, on_perf_event, take_halt_reason, pending_halt_reason}`.
- Emulator run-loop integration: `StepResult::ProfileStop`, `FrameOutcome::{Yielded, ProfileStop, Halted}`, `Riscv32Emulator::{run_until_yield_or_stop, set_profile_gate}`, `profile_stop_pending` flag, `SYSCALL_PERF_EVENT` handler in both `run_loops.rs` (run path) and `execution.rs` (step path) sharing logic via `handle_perf_event_syscall`.
- New `ProfileMode` enum (`SteadyRender` default, `Compile`, `Startup`, `All`) and per-mode `Gate` state machines in `lp-cli/src/commands/profile/mode/`. `STEADY_RENDER_WARMUP_FRAMES = 2`, `STEADY_RENDER_CAPTURE_FRAMES = 4` exposed as `pub const`.
- CLI refactor:
  - `--frames` removed; `--mode <…>` (default `steady-render`) and `--max-cycles N` (default 200_000_000) added.
  - Default `--collect` changed from `alloc` to `events`; both supported (comma-separated).
  - `handler.rs` slimmed; new `workload.rs` (frame-driving loop calling `run_until_yield_or_stop`, plus best-effort `try_stop_projects`) and `output.rs` (`build_initial_metadata` + `update_metadata_finish`).
  - Trace-dir naming: `profiles/<timestamp>--<workload>--<mode_slug>[--<note>]`.
- `SessionMetadata` field changes: dropped `frames_requested`; added `mode`, `max_cycles`, `cycles_used`, `terminated_by` (`"profile_stop"` | `"max_cycles"` | `"guest_halt"`). `schema_version` stays at 1.
- m0 frame-loop bug fixed: CLI workload now actually executes guest steps (`emu.run_until_yield_or_stop(MAX_STEPS_PER_FRAME)`) instead of only advancing the simulated clock.
- New e2e test `lp-cli/tests/profile_events_steady_render_smoke.rs` (uses `--mode startup` for fast termination, verifies `meta.json` shape, `events.jsonl` schema, presence of `frame` events, `report.txt`).
- Updated `lp-fw/fw-tests/tests/profile_alloc_emu.rs` for new metadata shape.
- Removed leftover `print!("{buf}")` from `ProfileSession::finish` (was duplicating the report file to stdout).

### Decisions for future reference

#### Global `lp-perf` crate replaced threaded `PerfEventSink`

- **Decision:** Perf events are emitted via cfg-gated macros in a global `lp-perf` crate, not via a `Rc<dyn PerfEventSink>` threaded through `ProjectRuntime` / `ShaderRuntime` / `LpGraphics`.
- **Why:** The threaded approach forced `RefCell`s, leaked into trait signatures (`compile_shader(&self, sink: …)`), and was awkward to reach from `lpvm-native` which doesn't see `ProjectRuntime`. Macros compile to no-ops in non-instrumented builds and to a single ECALL in fw-emu.
- **Rejected alternatives:** Threaded sink (too invasive); per-emission-site feature flags (combinatorial explosion); emitting only at `ProjectRuntime` boundary (loses sub-frame events like `shader-link` inside `lpvm-native`).
- **Revisit when:** We need per-instance sink routing (e.g. multiple concurrent profile sessions in a single guest), which would force interior mutability + dispatch table.

#### `lp-base/` directory + prefix-free naming convention

- **Decision:** Cross-cutting foundational crates live under `lp-base/` and use prefix-free names (`lp-perf`, future `lpfs`). The lack of a group prefix is the convention's signal.
- **Why:** Needed a home for `lp-perf` that wasn't owned by `lp-core`, `lp-shader`, or `lp-fw`. Anticipates the planned rename of `lp-core/*` to `lpc-*` and of other groups to `lp{shader,fw,riscv}-` style prefixes.
- **Rejected alternatives:** `lp-common/` (collides with planned `lpc-` prefix); `lp-util/lpu-*` (too vague, "util" attracts cruft).
- **Revisit when:** We end up with so many `lp-base/*` crates that further sub-grouping becomes useful.

#### `GateAction::Enable` / `Disable` are noop in m1

- **Decision:** Only `GateAction::Stop` has runtime effect in m1. `Enable` / `Disable` are accepted but only logged at trace level.
- **Why:** Real enable/disable semantics need per-collector "active" state and a host-side commit/discard for buffered samples — too much surface for m1, which is about wiring the event channel and stop signal. m2 (CPU collector) is the right place to land the real semantics together with the first collector that benefits.
- **Rejected alternatives:** Implement now (overengineers m1); make them errors (false-negative for modes that already want to express the intent).
- **Revisit when:** Starting m2; the CPU collector will need active/inactive windows.

#### `PerfEventKind::as_str()` uses Chrome trace single letters

- **Decision:** `events.jsonl` `kind` field is `"B"` / `"E"` / `"I"` (matching Chrome `chrome://tracing` event-phase naming) instead of `"Begin"` / `"End"` / `"Instant"`.
- **Why:** Sets us up for direct compatibility with the Chrome trace event format if we want to render timelines in `chrome://tracing` or Perfetto without a translation layer.
- **Rejected alternatives:** Long names (more readable in raw jsonl, but no consumer benefit and forces a translation step for the tracing UIs).
- **Revisit when:** A planned consumer wants the long form (unlikely; tracing UIs are the obvious consumers).

#### `SYSCALL_PERF_EVENT` handled in both `run_loops.rs` and `execution.rs`

- **Decision:** The handler exists in both the run-path (`handle_syscall`) and the step-path (`step_inner`) and shares logic via `handle_perf_event_syscall`.
- **Why:** Tests and the call-function API drive `step()` directly; without coverage in `step_inner`, perf ECALLs would be reported as unknown syscalls in those code paths.
- **Rejected alternatives:** Have `step()` route through the run-loop syscall dispatcher (couples otherwise distinct paths); skip `step()` coverage (breaks any future `step()`-driven test that runs the engine).
- **Revisit when:** The two paths get unified into a single dispatcher.
