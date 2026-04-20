# CPU Profile m1 — Perf-Event System + ProfileMode — Design

This implements **m1** of the CPU profile roadmap
(`docs/roadmaps/2026-04-19-cpu-profile/m1-perf-events.md`) on top of
the m0 foundation (`Collector` trait, `ProfileSession`, unified profile
dir layout).

m1 adds the perf-event substrate and `ProfileMode` gate. Standalone
deliverable:

```
lp-cli profile examples/basic --collect events --mode steady-render
```

…produces a perf-event timeline of the steady-state render. No CPU
attribution yet (m2).

## Scope of work

In scope:

- New workspace directory `lp-base/` for cross-cutting foundational
  crates. `README.md` documents the convention (no group prefix on
  contents). Future home of the planned `lpfs` extraction.
- New crate `lp-base/lp-perf` — `#![no_std]`, tiny, sink choice via
  build-time cargo features:
  - Macros `emit_begin!`, `emit_end!`, `emit_instant!`.
  - Event-name constants (`EVENT_FRAME`, `EVENT_SHADER_COMPILE`,
    `EVENT_SHADER_LINK`, `EVENT_PROJECT_LOAD`).
  - `PerfEventKind` enum (`Begin`, `End`, `Instant`).
  - Sinks: `noop` (default), `syscall` (cfg `feature = "syscall"`,
    deps `lp-riscv-emu-shared` for the constant), `log` (cfg `feature
    = "log"`, deps `log` crate).
- Engine emission points:
  - `lp-engine` deps `lp-perf` (no features). Emits `EVENT_FRAME`,
    `EVENT_SHADER_COMPILE`, `EVENT_PROJECT_LOAD` from
    `ProjectRuntime::tick`, `ShaderRuntime::compile_shader`, and
    `ProjectRuntime::new` (the loader half).
  - `lpvm-native` deps `lp-perf` (no features). Emits
    `EVENT_SHADER_LINK` around `link_jit`.
- Syscall layer:
  - `SYSCALL_PERF_EVENT = 10` added to
    `lp-riscv-emu-shared/src/syscall.rs`. Reserved
    `SYSCALL_JIT_MAP_LOAD = 11` and `SYSCALL_JIT_MAP_UNLOAD = 12` for
    m5; no implementation, just constants with reserved-comment.
  - Host syscall handler in
    `lp-riscv-emu/src/emu/emulator/run_loops.rs` reads name string
    from guest memory, stamps host `cycle_count`, dispatches to
    `ProfileSession::on_perf_event`.
- fw-emu wires `lp-perf = { features = ["syscall"] }`. fw-esp32 wires
  default (noop) — no behavior change for esp32 in m1.
- Host-side profile additions:
  - `PerfEvent` host type (replaces m0 stub): `cycle: u64`,
    `name: &'static str` (interned, see notes), `kind: PerfEventKind`.
  - `EventsCollector`: writes `events.jsonl`, adds `clock_source:
    "emu_estimated"` to per-collector meta (already in `meta.json`
    top-level via m0; collector adds a `event_count`).
  - `ProfileSession::on_perf_event` extension: stamps cycle, runs
    gate state machine, fans out to collectors.
  - `HaltReason::ProfileStop` — new variant.
  - `StepResult::ProfileStop` — new variant.
  - `Riscv32Emulator::run_until_yield_or_stop(max_steps)` — new
    sibling method; returns `Result<FrameOutcome, EmulatorError>`.
- Mode system in `lp-cli/src/commands/profile/mode/`:
  - `ProfileMode` enum: `SteadyRender`, `Compile`, `Startup`, `All`.
    Parameter-less variants per roadmap.
  - `GateAction` enum: `Enable`, `Disable`, `NoChange`, `Stop`.
  - `Gate` trait: `on_event(&mut self, evt: &PerfEvent) ->
    GateAction`.
  - One file per mode's state machine. `pub const`s for tunables
    (warmup/capture frame counts, etc.).
- CLI surface expansion (additive, with one removal):
  - `--collect` accepts `events` (still accepts `alloc`); default
    flips from `alloc` to `events`.
  - `--mode {steady-render|compile|startup|all}` (default
    `steady-render`).
  - `--max-cycles N` safety cap (default 200_000_000).
  - `--frames` removed (mode handles termination).
- CLI internals:
  - Split `handler.rs` into `handler.rs` (orchestrator),
    `workload.rs` (frame loop + max-cycles + stop), `output.rs`
    (meta.json + report.txt), and `mode/` (mode state machines).
  - Frame loop *actually drives the emulator* per frame (fixes the
    m0 vestigial loop that only bumped the clock).
- Profile dir name extended: `profiles/<ts>--<workload>--<mode>
  [--<note>]/`. Mode segment always present; default `steady-render`.
- Metadata changes (per-collector blocks unchanged):
  - `frames_requested` removed; `mode: String` added.
  - `max_cycles: u64`, `cycles_used: u64`, `terminated_by: String`
    added (`terminated_by ∈ {"profile_stop", "max_cycles", "error"}`).
  - `schema_version` stays `1` — nothing real consumes meta.json yet.
- Tests:
  - Unit tests for each `Gate` impl over synthetic event streams.
  - Unit test for `EventsCollector` write path.
  - Unit test for `lp-perf` macros (cfg-gated noop and `log` paths;
    syscall path covered by the e2e test).
  - End-to-end: `lp-cli/tests/profile_events_steady_render_smoke.rs`
    runs `examples/basic` under `--collect events --mode
    steady-render` and asserts non-empty `events.jsonl` plus the
    expected `mode`, `terminated_by` in `meta.json`.
  - Update m0's `profile_alloc_emu` test to reflect the metadata
    field rename (`mode` instead of `frames_requested`).

Out of scope (deferred per roadmap):

- `CpuCollector`, `InstClass`, sample storage, hot-path attribution,
  speedscope export, `--cycle-model` flag — m2.
- Real Enable/Disable behavior on collectors (m1 lands the variants
  but only implements `Stop`; collectors record from t=0 to Stop) —
  m2.
- Functional `profile diff`, `--diff [PATH]` flag — m3.
- `HardwarePerfSink`, console parser — m4.
- `SYSCALL_JIT_MAP_LOAD` implementation, JIT symbolizer — m5.
- Per-event `arg: u32` payload (ABI room reserved as `a3`).
- `--raw-events` opt-in.
- `docs/design/native/fw-profile/` doc home — m6.
- `lp-core/* → lpc-*` rename and `lpfs` extraction (separate
  follow-ups; orthogonal to m1).

## File structure

```
lp-base/                                   # NEW DIRECTORY
├── README.md                              # NEW: convention doc
└── lp-perf/                               # NEW CRATE
    ├── Cargo.toml
    └── src/
        ├── lib.rs                         # macros, consts, kind enum, __emit
        └── sinks/
            ├── mod.rs
            ├── noop.rs
            ├── syscall.rs                 # cfg(feature = "syscall")
            └── log.rs                     # cfg(feature = "log")

lp-core/lp-engine/
├── Cargo.toml                             # UPDATE: + lp-perf dep
└── src/
    ├── runtime/
    │   ├── project.rs                     # UPDATE: emit EVENT_FRAME +
    │   │                                  #         EVENT_PROJECT_LOAD
    │   └── …
    └── nodes/shader/
        └── runtime.rs                     # UPDATE: emit
                                           #         EVENT_SHADER_COMPILE

lp-shader/lpvm-native/
├── Cargo.toml                             # UPDATE: + lp-perf dep
└── src/
    └── rt_jit/compiler.rs                 # UPDATE: emit EVENT_SHADER_LINK
                                           #         around link_jit call

lp-fw/fw-emu/
└── Cargo.toml                             # UPDATE: + lp-perf with
                                           #         features = ["syscall"]
lp-fw/fw-esp32/
└── Cargo.toml                             # UPDATE: + lp-perf default

lp-riscv/lp-riscv-emu-shared/
└── src/syscall.rs                         # UPDATE: + SYSCALL_PERF_EVENT,
                                           #         + reserved JIT consts

lp-riscv/lp-riscv-emu/
└── src/
    ├── profile/
    │   ├── mod.rs                         # UPDATE: HaltReason::ProfileStop;
    │   │                                  # PerfEvent fields (no longer stub);
    │   │                                  # ProfileSession::on_perf_event ext
    │   ├── perf_event.rs                  # NEW: PerfEvent type
    │   ├── events.rs                      # NEW: EventsCollector
    │   └── alloc.rs                       # unchanged
    └── emu/emulator/
        ├── types.rs                       # UPDATE: + StepResult::ProfileStop
        ├── run_loops.rs                   # UPDATE: SYSCALL_PERF_EVENT handler
        └── state.rs                       # UPDATE: + run_until_yield_or_stop

lp-cli/src/commands/profile/
├── args.rs                                # UPDATE: --mode, --max-cycles,
│                                          #         drop --frames
├── handler.rs                             # UPDATE: shrink to orchestrator
├── workload.rs                            # NEW: frame loop, stop, max-cycles
├── output.rs                              # NEW: meta.json + report.txt
├── mode/                                  # NEW
│   ├── mod.rs                             # ProfileMode, GateAction, Gate
│   ├── steady_render.rs
│   ├── compile.rs
│   ├── startup.rs
│   └── all.rs
├── diff_stub.rs                           # unchanged
└── mod.rs                                 # UPDATE: re-exports

lp-cli/tests/
└── profile_events_steady_render_smoke.rs  # NEW

lp-fw/fw-tests/tests/
└── profile_alloc_emu.rs                   # UPDATE: meta field assertions

Cargo.toml (root)                          # UPDATE: members += lp-base/lp-perf
```

## Conceptual architecture

### End-to-end flow

```
guest (RV32 firmware)                          host
─────────────────────                          ────
lp-engine / lpvm-native
   │
   ├─ emit_begin!(EVENT_FRAME)
   │      │
   │      ▼ (cfg-gated dispatch)
   │   lp-perf::__emit
   │      │
   │      ▼ (feature = "syscall")
   │   ECALL SYSCALL_PERF_EVENT(name_ptr, name_len, kind)
   │      │
   │      ▼
   │  emu run loop ─► handle_syscall ─► reads name from guest mem
   │                                  ─► stamps host cycle_count
   │                                  ─► PerfEvent { cycle, name, kind }
   │                                  ─► profile_session.on_perf_event(evt)
   │                                          │
   │           ┌──────────────────────────────┤
   │           │                              │
   │           ▼                              ▼
   │       gate.on_event(evt)             collectors.fan_out(evt)
   │           │                              │
   │           ▼                              ▼
   │      GateAction                      EventsCollector ─► events.jsonl
   │           │
   │           ▼
   │   if Stop: SyscallAction::Halt(HaltReason::ProfileStop)
   │            └── run loop returns StepResult::ProfileStop
   │                 └── run_until_yield_or_stop returns FrameOutcome::ProfileStopped
   │                      └── workload loop in lp-cli breaks
   │                           └── handler writes meta.json + report.txt + exits 0
```

### Component overview

```
lp-base/lp-perf                                                  lp-cli
───────────────                                                  ──────
src/lib.rs                                                       commands/profile/
   ├── pub enum PerfEventKind { Begin, End, Instant }                 args.rs ─────┐
   ├── pub const EVENT_FRAME / EVENT_SHADER_COMPILE / …               handler.rs   │
   ├── pub macro emit_begin!                                          workload.rs  │
   ├── pub macro emit_end!                                            output.rs    │
   ├── pub macro emit_instant!                                        mode/        │
   └── pub fn __emit(name, kind)                                          mod.rs   │
         (sinks selected at compile time)                                 steady_render.rs
                                                                          compile.rs
sinks/syscall.rs   (cfg feature = "syscall")                              startup.rs
   └── ECALL SYSCALL_PERF_EVENT, ptr, len, kind                            all.rs
sinks/log.rs       (cfg feature = "log")
   └── log::trace!("perf {} {:?}", name, kind)                       diff_stub.rs
sinks/noop.rs      (default)
   └── { /* compiles to nothing via inline */ }                                    │
                                                                                   │
lp-riscv-emu                                                                       │
────────────                                                                       │
profile/mod.rs                                                                     │
   ├── enum HaltReason { Oom, ProfileStop }                                        │
   ├── struct ProfileSession                                                       │
   │     ├── ::on_perf_event(EmuCtx, name_ptr, name_len, kind) → SyscallAction     │
   │     │     1. read name from guest memory; intern as &'static str              │
   │     │     2. stamp PerfEvent { cycle: ctx.cycle_count, name, kind }           │
   │     │     3. let action = self.gate.on_event(&evt)                            │
   │     │     4. for c in collectors: c.on_perf_event(&evt)                       │
   │     │     5. if action == Stop:                                               │
   │     │           SyscallAction::Halt(HaltReason::ProfileStop)                  │
   │     │        else: SyscallAction::Handled                                     │
   │     ├── ::set_gate(Box<dyn Gate>)         ◄────────────── lp-cli wires here ──┘
   │     └── ::cycles_used() → u64
   └── pub trait Gate: Send {
          fn on_event(&mut self, evt: &PerfEvent) -> GateAction;
       }

profile/perf_event.rs
   └── pub struct PerfEvent { cycle: u64, name: &'static str, kind: PerfEventKind }

profile/events.rs
   ├── struct EventsCollector { writer: BufWriter<File>, event_count: u64 }
   └── impl Collector → writes one JSON line per event:
         { "cycle": <u64>, "name": <str>, "kind": <"B"|"E"|"I"> }

emu/emulator/
   ├── types.rs:    pub enum StepResult { …, ProfileStop }
   ├── run_loops.rs: SYSCALL_PERF_EVENT handler
   │       ├── read args (name_ptr, name_len, kind)
   │       ├── build EmuCtx
   │       ├── session.on_perf_event(...) → SyscallAction
   │       ├── Pass / Handled / Halt(ProfileStop) → StepResult mapping
   └── state.rs:    fn run_until_yield_or_stop(max) → Result<FrameOutcome, …>
```

## The `lp-base/lp-perf` crate

### `Cargo.toml`

```toml
[package]
name = "lp-perf"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true
rust-version.workspace = true

[features]
default = []
syscall = ["dep:lp-riscv-emu-shared"]
log     = ["dep:log"]

[dependencies]
lp-riscv-emu-shared = { path = "../../lp-riscv/lp-riscv-emu-shared", optional = true }
log = { workspace = true, default-features = false, optional = true }

[lints]
workspace = true
```

`#![no_std]` throughout. No `alloc`. No transitive deps when consumed
without features.

### `src/lib.rs`

```rust
#![no_std]

mod sinks;

#[derive(Copy, Clone, Debug)]
#[repr(u32)]
pub enum PerfEventKind {
    Begin   = 0,
    End     = 1,
    Instant = 2,
}

// Canonical event-name constants. New names get added here, never
// inline in call sites.
pub const EVENT_FRAME:          &str = "frame";
pub const EVENT_SHADER_COMPILE: &str = "shader-compile";
pub const EVENT_SHADER_LINK:    &str = "shader-link";
pub const EVENT_PROJECT_LOAD:   &str = "project-load";

#[macro_export]
macro_rules! emit_begin {
    ($name:expr) => { $crate::__emit($name, $crate::PerfEventKind::Begin) };
}
#[macro_export]
macro_rules! emit_end {
    ($name:expr) => { $crate::__emit($name, $crate::PerfEventKind::End) };
}
#[macro_export]
macro_rules! emit_instant {
    ($name:expr) => { $crate::__emit($name, $crate::PerfEventKind::Instant) };
}

// Single dispatch point. Implementation is selected at compile time.
#[inline(always)]
pub fn __emit(name: &'static str, kind: PerfEventKind) {
    sinks::emit(name, kind);
}
```

### `src/sinks/mod.rs`

```rust
use crate::PerfEventKind;

#[cfg(all(feature = "syscall", not(feature = "log")))]
mod syscall;
#[cfg(all(feature = "syscall", not(feature = "log")))]
pub use syscall::emit;

#[cfg(all(feature = "log", not(feature = "syscall")))]
mod log_sink;
#[cfg(all(feature = "log", not(feature = "syscall")))]
pub use log_sink::emit;

#[cfg(not(any(feature = "syscall", feature = "log")))]
mod noop;
#[cfg(not(any(feature = "syscall", feature = "log")))]
pub use noop::emit;

#[cfg(all(feature = "syscall", feature = "log"))]
compile_error!("lp-perf: enable at most one of `syscall` or `log`");
```

(Mutually-exclusive features keep dispatch trivial. We can grow a
multi-sink mode later if a real need shows up; for m1, no caller wants
both.)

### `src/sinks/noop.rs`

```rust
use crate::PerfEventKind;

#[inline(always)]
pub fn emit(_name: &'static str, _kind: PerfEventKind) {}
```

### `src/sinks/syscall.rs`

```rust
use crate::PerfEventKind;
use lp_riscv_emu_shared::SYSCALL_PERF_EVENT;

#[inline(always)]
pub fn emit(name: &'static str, kind: PerfEventKind) {
    let ptr = name.as_ptr() as i32;
    let len = name.len() as i32;
    let kind_u = kind as i32;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("x17") SYSCALL_PERF_EVENT,
            in("x10") ptr,
            in("x11") len,
            in("x12") kind_u,
            // x13 reserved for future arg payload
            options(nostack, preserves_flags),
        );
    }
}
```

(Style matches the existing `lp-riscv-emu-guest::syscall::syscall`
fn — same register conventions, same calling style.)

### `src/sinks/log_sink.rs`

```rust
use crate::PerfEventKind;

#[inline(always)]
pub fn emit(name: &'static str, kind: PerfEventKind) {
    log::trace!("perf {} {:?}", name, kind);
}
```

### Usage examples

```rust
// In lp-engine/src/runtime/project.rs::tick
use lp_perf::{emit_begin, emit_end, EVENT_FRAME};

pub fn tick(&mut self, delta_ms: u32) -> Result<…, …> {
    emit_begin!(EVENT_FRAME);
    let result = self.tick_inner(delta_ms);
    emit_end!(EVENT_FRAME);
    result
}
```

```rust
// In lpvm-native/src/rt_jit/compiler.rs::compile
use lp_perf::{emit_begin, emit_end, EVENT_SHADER_LINK};

pub fn compile(...) -> ... {
    // ... codegen ...
    emit_begin!(EVENT_SHADER_LINK);
    let elf = link_jit(&object_bytes)?;
    emit_end!(EVENT_SHADER_LINK);
    // ...
}
```

In a non-profile build (no `syscall`/`log` feature on `lp-perf`),
both `emit_*!` calls inline to nothing and the resulting binary is
byte-identical to one with the calls removed.

## Engine emission points (m1 set)

| Event                  | Site                                           | Begin / End wrapper                          |
|------------------------|------------------------------------------------|----------------------------------------------|
| `EVENT_FRAME`          | `ProjectRuntime::tick(&mut self, delta_ms)`    | wraps the body                               |
| `EVENT_PROJECT_LOAD`   | `ProjectRuntime::load_from_filesystem(...)`    | wraps the body                               |
| `EVENT_SHADER_COMPILE` | `ShaderRuntime::compile_shader(...)`           | wraps the body (incl. `compile_shader` call) |
| `EVENT_SHADER_LINK`    | `lpvm_native::rt_jit::compiler::compile(...)`  | wraps the `link_jit(...)` call only          |

The `shader-link` event nests inside `shader-compile` because the
former is a sub-step of the latter — the timeline in `events.jsonl`
will show this nesting via cycle ranges.

## Syscall ABI

### `lp-riscv-emu-shared/src/syscall.rs` additions

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

### Host syscall handler (`run_loops.rs`)

```rust
SYSCALL_PERF_EVENT => {
    let session = match self.profile_session.as_mut() {
        Some(s) => s,
        None => {
            // No session installed (e.g. fw-emu built with feature
            // = ["syscall"] but lp-cli didn't install a session).
            // Treat as no-op + Continue.
            self.regs[Gpr::A0.num() as usize] = 0;
            return Ok(StepResult::Continue);
        }
    };

    let name_ptr = syscall_info.args[0] as u32;
    let name_len = syscall_info.args[1] as u32;
    let kind_raw = syscall_info.args[2] as u32;

    // Build EmuCtx + dispatch.
    let mut ctx = EmuCtx { /* same as Q7 sketch */ };
    let action = session.on_perf_event(&mut ctx, name_ptr, name_len, kind_raw);

    match action {
        SyscallAction::Pass | SyscallAction::Handled => {
            self.regs[Gpr::A0.num() as usize] = 0;
            Ok(StepResult::Continue)
        }
        SyscallAction::Halt(HaltReason::ProfileStop) => {
            Ok(StepResult::ProfileStop)
        }
        SyscallAction::Halt(HaltReason::Oom { size }) => {
            // Defensive: shouldn't happen from perf_event, but route
            // it correctly if it does.
            Ok(StepResult::Oom(OomInfo { size, pc: self.pc }))
        }
    }
}
```

`session.on_perf_event` reads the name string from guest memory
(bounded read; checks `name_len <= MAX_EVENT_NAME_LEN`) and either:

- Interns the slice into a `&'static str` via a session-owned
  `StringInterner` (small `HashMap<String, &'static str>` over a
  bumped string arena). Names are a small closed set in m1
  (`frame`, `shader-compile`, `shader-link`, `project-load`); the
  interner is mostly a defensive measure against a misbehaving
  guest sending arbitrary names.
- Or — simpler for m1 — match against a known-set of names and
  reject unknown ones via a warning. Pick the simpler approach for
  m1; switch to interner if/when the name set genuinely opens up.

Picked approach for m1: **known-set match**. The `lp-perf` crate's
event-name constants are mirrored on the host side in
`profile/perf_event.rs::KNOWN_EVENT_NAMES: &[&'static str]`. Unknown
names log a warning and are dropped. This keeps `PerfEvent.name:
&'static str` honest without an arena.

`MAX_EVENT_NAME_LEN: usize = 64` — generous; the longest we use is
`"shader-compile"` at 14.

## Host-side `PerfEvent` (`profile/perf_event.rs`)

```rust
use crate::profile::PerfEventKind;

pub const MAX_EVENT_NAME_LEN: usize = 64;

pub static KNOWN_EVENT_NAMES: &[&str] = &[
    "frame",
    "shader-compile",
    "shader-link",
    "project-load",
];

#[derive(Clone, Debug)]
pub struct PerfEvent {
    pub cycle: u64,
    pub name: &'static str,
    pub kind: PerfEventKind,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum PerfEventKind {
    Begin   = 0,
    End     = 1,
    Instant = 2,
}

impl PerfEventKind {
    pub fn from_u32(v: u32) -> Option<Self> {
        match v {
            0 => Some(Self::Begin),
            1 => Some(Self::End),
            2 => Some(Self::Instant),
            _ => None,
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Begin   => "B",
            Self::End     => "E",
            Self::Instant => "I",
        }
    }
}
```

(Note: `PerfEventKind` exists in *both* `lp-perf` and
`lp-riscv-emu/src/profile/`. The two enums are intentionally
separate — `lp-perf`'s is what the guest uses to encode the syscall
arg; the host re-derives via `from_u32`. Sharing one enum across
both crates would force `lp-riscv-emu` to depend on `lp-perf`, which
is the wrong direction. The duplication is two trivial three-variant
enums.)

## `EventsCollector` (`profile/events.rs`)

```rust
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use crate::profile::{
    Collector, EmuCtx, FinishCtx, PerfEvent, SyscallAction,
};

pub struct EventsCollector {
    writer: BufWriter<File>,
    event_count: u64,
}

impl EventsCollector {
    pub fn new(trace_dir: &Path) -> std::io::Result<Self> {
        let path = trace_dir.join("events.jsonl");
        let writer = BufWriter::new(File::create(&path)?);
        Ok(Self { writer, event_count: 0 })
    }
}

impl Collector for EventsCollector {
    fn name(&self) -> &'static str { "events" }
    fn report_title(&self) -> &'static str { "Perf Events" }

    fn meta_json(&self) -> serde_json::Value {
        serde_json::json!({
            "event_count": self.event_count,
        })
    }

    fn on_perf_event(&mut self, evt: &PerfEvent) {
        // One JSON line per event.
        let _ = writeln!(
            self.writer,
            r#"{{"cycle":{},"name":"{}","kind":"{}"}}"#,
            evt.cycle,
            evt.name,
            evt.kind.as_str(),
        );
        self.event_count += 1;
    }

    fn finish(&mut self, _ctx: &FinishCtx) -> std::io::Result<()> {
        self.writer.flush()
    }

    fn report_section(&self, w: &mut dyn std::fmt::Write) -> std::fmt::Result {
        writeln!(w, "events recorded: {}", self.event_count)
    }
}
```

`event_count` is what `ProfileSession::finish()` aggregates into the
"events" report banner.

## `ProfileSession` extensions

Added in `profile/mod.rs`:

```rust
pub trait Gate: Send {
    fn on_event(&mut self, evt: &PerfEvent) -> GateAction;
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum GateAction {
    Enable,
    Disable,
    NoChange,
    Stop,
}

impl ProfileSession {
    pub fn set_gate(&mut self, gate: Box<dyn Gate>) {
        self.gate = Some(gate);
    }

    pub fn on_perf_event(
        &mut self,
        ctx: &mut EmuCtx<'_>,
        name_ptr: u32,
        name_len: u32,
        kind_raw: u32,
    ) -> SyscallAction {
        // 1. Read name from guest memory (bounded by MAX_EVENT_NAME_LEN).
        let name = match resolve_known_name(ctx, name_ptr, name_len) {
            Some(n) => n,
            None => {
                log::warn!("perf event with unknown name (len={name_len})");
                return SyscallAction::Handled;
            }
        };

        // 2. Validate kind.
        let kind = match PerfEventKind::from_u32(kind_raw) {
            Some(k) => k,
            None => {
                log::warn!("perf event with invalid kind: {kind_raw}");
                return SyscallAction::Handled;
            }
        };

        // 3. Build event with host-side cycle stamp.
        let evt = PerfEvent { cycle: ctx.cycle_count, name, kind };

        // 4. Run gate.
        let action = match self.gate.as_mut() {
            Some(g) => g.on_event(&evt),
            None    => GateAction::NoChange,
        };

        // 5. Fan out to collectors.
        for c in &mut self.collectors {
            c.on_perf_event(&evt);
        }

        // 6. Translate gate result.
        match action {
            GateAction::Stop => SyscallAction::Halt(HaltReason::ProfileStop),
            // m1: Enable/Disable/NoChange all produce Handled.
            // m2 will wire Enable/Disable into a session-wide
            // `enabled: bool` that gates collectors.
            _ => SyscallAction::Handled,
        }
    }

    pub fn cycles_used(&self) -> u64 {
        self.cycles_used
    }
}
```

`ProfileSession` field additions:

```rust
pub struct ProfileSession {
    trace_dir: PathBuf,
    collectors: Vec<Box<dyn Collector>>,
    gate: Option<Box<dyn Gate>>,                    // NEW
    terminated_by: Option<TerminatedBy>,            // NEW
    // cycles_used is read from the emulator at finish time, not
    // stored here — source of truth is `Riscv32Emulator::cycle_count`.
}

#[derive(Copy, Clone, Debug)]
pub enum TerminatedBy {
    ProfileStop,
    MaxCycles,
    Error,
}
```

`HaltReason` extension (existing `Oom { size }` plus):

```rust
pub enum HaltReason {
    Oom { size: u32 },
    ProfileStop,                                    // NEW
}
```

## Run loop / step result

`emu/emulator/types.rs`:

```rust
pub enum StepResult {
    Continue,
    Syscall(SyscallInfo),
    Halted,
    Trap(u32),
    Panic,
    Oom(OomInfo),
    FuelExhausted,
    ProfileStop,                                    // NEW
}
```

`emu/emulator/state.rs`:

```rust
pub enum FrameOutcome {
    Yielded(SyscallInfo),
    ProfileStopped,
}

impl Riscv32Emulator {
    /// Like `run_until_yield`, but also returns cleanly on
    /// `StepResult::ProfileStop` (raised by SYSCALL_PERF_EVENT
    /// when the gate state machine signals Stop).
    pub fn run_until_yield_or_stop(
        &mut self,
        max_steps: u64,
    ) -> Result<FrameOutcome, EmulatorError> {
        loop {
            match self.run_fuel(max_steps)? {
                StepResult::Syscall(info) if info.number == SYSCALL_YIELD => {
                    return Ok(FrameOutcome::Yielded(info));
                }
                StepResult::Syscall(_) => continue,
                StepResult::ProfileStop => return Ok(FrameOutcome::ProfileStopped),
                StepResult::Halted => return Err(EmulatorError::InvalidInstruction { /* … */ }),
                StepResult::Trap(code) => return Err(EmulatorError::Trap { code, /* … */ }),
                StepResult::Oom(info) => return Err(EmulatorError::Oom(info)),
                StepResult::Panic => return Err(EmulatorError::Panic),
                StepResult::FuelExhausted => return Err(EmulatorError::FuelExhausted),
                StepResult::Continue => continue, // shouldn't reach
            }
        }
    }
}
```

The existing `run_until_yield` is **unchanged** — the `ProfileStop`
case maps to the existing "unexpected EBREAK" error path (it
shouldn't occur during transport-driven `project_load`, where this
function is called).

## Mode system

### `mode/mod.rs`

```rust
use clap::ValueEnum;
use lp_riscv_emu::profile::{Gate, GateAction, PerfEvent};

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum ProfileMode {
    SteadyRender,
    Compile,
    Startup,
    All,
}

impl ProfileMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SteadyRender => "steady-render",
            Self::Compile      => "compile",
            Self::Startup      => "startup",
            Self::All          => "all",
        }
    }

    pub fn build_gate(&self) -> Box<dyn Gate> {
        match self {
            Self::SteadyRender => Box::new(steady_render::SteadyRenderGate::new()),
            Self::Compile      => Box::new(compile::CompileGate::new()),
            Self::Startup      => Box::new(startup::StartupGate::new()),
            Self::All          => Box::new(all::AllGate::new()),
        }
    }
}

mod steady_render;
mod compile;
mod startup;
mod all;
```

### `mode/steady_render.rs`

```rust
use lp_perf::{EVENT_FRAME, EVENT_SHADER_COMPILE};
use lp_riscv_emu::profile::{Gate, GateAction, PerfEvent, PerfEventKind};

pub const STEADY_RENDER_WARMUP_FRAMES:  u32 = 2;
pub const STEADY_RENDER_CAPTURE_FRAMES: u32 = 4;

#[derive(Default)]
pub struct SteadyRenderGate {
    state: State,
    frames_after_warmup: u32,
}

#[derive(Default, Copy, Clone, Debug, PartialEq, Eq)]
enum State {
    #[default]
    WaitingForFirstCompile,
    Warmup { frames_seen: u32 },
    Capturing { frames_seen: u32 },
}

impl SteadyRenderGate {
    pub fn new() -> Self { Self::default() }
}

impl Gate for SteadyRenderGate {
    fn on_event(&mut self, evt: &PerfEvent) -> GateAction {
        match (&self.state, evt.name, evt.kind) {
            (State::WaitingForFirstCompile, EVENT_SHADER_COMPILE, PerfEventKind::End) => {
                self.state = State::Warmup { frames_seen: 0 };
                GateAction::NoChange
            }
            (State::Warmup { frames_seen }, EVENT_FRAME, PerfEventKind::End) => {
                let n = frames_seen + 1;
                if n >= STEADY_RENDER_WARMUP_FRAMES {
                    self.state = State::Capturing { frames_seen: 0 };
                    GateAction::Enable
                } else {
                    self.state = State::Warmup { frames_seen: n };
                    GateAction::NoChange
                }
            }
            (State::Capturing { frames_seen }, EVENT_FRAME, PerfEventKind::End) => {
                let n = frames_seen + 1;
                if n >= STEADY_RENDER_CAPTURE_FRAMES {
                    GateAction::Stop
                } else {
                    self.state = State::Capturing { frames_seen: n };
                    GateAction::NoChange
                }
            }
            _ => GateAction::NoChange,
        }
    }
}
```

### Other modes (sketch)

- `compile`: `Stop` after first `EVENT_SHADER_COMPILE` End. Useful
  for "what happens during shader compile".
- `startup`: `Stop` after first `EVENT_FRAME` End. Captures
  project-load + first-frame.
- `all`: never `Stop`. Runs until `--max-cycles`. Useful for
  manual exploration; relies on the safety cap.

Each mode file owns its own `pub const` tunables (or none, in `all`'s
case).

## CLI surface

### `args.rs` (final shape)

```rust
#[derive(clap::Args, Debug)]
pub struct ProfileArgs {
    #[arg(default_value = "examples/basic")]
    pub dir: PathBuf,

    /// Comma-separated list of collectors to enable.
    /// Default: events. Supported in m1: events, alloc.
    #[arg(long, value_delimiter = ',', default_value = "events")]
    pub collect: Vec<String>,

    /// Profiling mode. Drives termination policy.
    #[arg(long, value_enum, default_value_t = ProfileMode::SteadyRender)]
    pub mode: ProfileMode,

    /// Safety cap on total emulator cycles.
    #[arg(long, default_value_t = 200_000_000)]
    pub max_cycles: u64,

    /// Optional note appended to the profile dir name.
    #[arg(long)]
    pub note: Option<String>,
}

#[derive(clap::Subcommand, Debug)]
pub enum ProfileSubcommand {
    /// Diff two profile dirs (m3 — stub in m1).
    Diff(ProfileDiffArgs),
}
```

### `workload.rs` (per-frame loop)

```rust
const MAX_STEPS_PER_FRAME: u64 = 50_000_000;

pub enum WorkloadOutcome {
    ProfileStopped,
    MaxCyclesReached,
}

pub fn run_workload(
    emulator_arc: &Arc<Mutex<Riscv32Emulator>>,
    max_cycles: u64,
) -> Result<WorkloadOutcome, EmulatorError> {
    loop {
        let outcome = {
            let mut emu = emulator_arc.lock().unwrap();
            emu.advance_time(40);
            emu.run_until_yield_or_stop(MAX_STEPS_PER_FRAME)?
        };

        match outcome {
            FrameOutcome::ProfileStopped => return Ok(WorkloadOutcome::ProfileStopped),
            FrameOutcome::Yielded(_) => {
                let cycles = emulator_arc.lock().unwrap().cycle_count();
                if cycles >= max_cycles {
                    return Ok(WorkloadOutcome::MaxCyclesReached);
                }
            }
        }
    }
}
```

### `output.rs`

Owns:

- Building `meta.json` (top-level fields including new `mode`,
  `max_cycles`, `cycles_used`, `terminated_by`, plus per-collector
  blocks merged in via `Collector::meta_json`).
- Writing `report.txt` (per-collector banners — same shape as m0).
- Pretty-printing the CLI banner ("steady-render: skip 2, capture 4
  frames", paths, exit summary).

### `handler.rs` (slim orchestrator)

```rust
pub async fn run(args: ProfileArgs) -> Result<()> {
    validate_collectors(&args.collect)?;

    let trace_dir = build_trace_dir(&args)?;
    let load_info = load_elf_for(&args)?;
    let emulator = build_emulator(&load_info)?;

    let metadata = build_session_metadata(&args, &load_info);
    let collectors = build_collectors(&args, &trace_dir, &load_info)?;
    let gate = args.mode.build_gate();

    let emulator = emulator.with_profile_session(
        trace_dir.clone(), &metadata, collectors,
    )?;
    {
        let mut e = emulator.lock().unwrap();
        e.profile_session_mut().unwrap().set_gate(gate);
    }

    let emulator_arc = Arc::new(Mutex::new(emulator));
    bootstrap_project(&emulator_arc, &args).await?;          // project_load

    let outcome = workload::run_workload(&emulator_arc, args.max_cycles);

    let terminated_by = match &outcome {
        Ok(workload::WorkloadOutcome::ProfileStopped)    => TerminatedBy::ProfileStop,
        Ok(workload::WorkloadOutcome::MaxCyclesReached)  => TerminatedBy::MaxCycles,
        Err(_)                                           => TerminatedBy::Error,
    };

    output::write_outputs(&emulator_arc, &trace_dir, &args, terminated_by)?;

    if let Ok(workload::WorkloadOutcome::MaxCyclesReached) = outcome {
        eprintln!("warning: --max-cycles {} reached without ProfileMode terminating", args.max_cycles);
    }
    println!("{}", trace_dir.display());
    Ok(())
}
```

(Function names are sketches; final names settled during phase 1.)

## Metadata

Final top-level `meta.json` shape after m1:

```json
{
  "schema_version": 1,
  "timestamp": "2026-04-19T13:57:32Z",
  "project": "<project uid>",
  "workload": "examples/basic",
  "note": null,
  "mode": "steady-render",
  "max_cycles": 200000000,
  "cycles_used": 12345678,
  "terminated_by": "profile_stop",
  "clock_source": "emu_estimated",
  "symbols": [ … ],
  "collectors": {
    "events": { "event_count": 42 },
    "alloc":  { "heap_start": 2147483648, "heap_size": 65536 }
  }
}
```

Field changes vs m0 baseline (`schema_version` stays `1`):

- **removed** `frames_requested`
- **added** `mode`
- **added** `max_cycles`
- **added** `cycles_used`
- **added** `terminated_by`

## Tests

Per the scope list:

- `lp-cli/src/commands/profile/mode/steady_render.rs#tests` —
  drives synthetic event sequences through `SteadyRenderGate`,
  asserts the exact `(state, action)` transitions per the constants.
  Same shape for `compile`, `startup`, `all`.
- `lp-riscv-emu/src/profile/events.rs#tests` — `EventsCollector::
  on_perf_event` then read-back: writes 5 events of mixed kinds,
  flushes, parses the file as JSONL, asserts cycle/name/kind round-
  trip.
- `lp-base/lp-perf/tests/macros.rs` — compiles each macro under
  default features (asserts call resolves to noop, no panics) and
  under `feature = "log"` (uses a test logger to capture). The
  `feature = "syscall"` path is exercised by the e2e test below.
- `lp-cli/tests/profile_events_steady_render_smoke.rs` — runs
  `examples/basic` under `--collect events --mode steady-render`,
  asserts: trace dir created with `--steady-render` segment, non-
  empty `events.jsonl` containing at least one `frame` Begin/End
  pair, `meta.json` has `mode: "steady-render"` and `terminated_by:
  "profile_stop"`. (Slow test; gated behind `[[test]] required-
  features` if needed for CI tier separation.)
- `lp-fw/fw-tests/tests/profile_alloc_emu.rs` — update the m0
  metadata assertion to expect `mode: "steady-render"` (default) and
  the new fields. Existing alloc-trace assertions unchanged.

## Risks and mitigations

- **`lpvm-native` adds an `lp-perf` dep edge.** New direction:
  `lp-shader → lp-base`. Mitigated by the new `lp-base/` convention
  (cross-cutting, depended-on by anyone) being explicitly documented.
- **Mutually-exclusive feature flags on `lp-perf`** (`syscall` vs
  `log`) — if both ever get set in a real build, the `compile_error!`
  catches it. fw-emu only ever enables `syscall`; fw-esp32 only ever
  `log` (or default).
- **Frame-driving fix is a behavior change in m0's published CLI.**
  m0's `--frames N` flag silently accepted any value and did almost
  nothing; m1 removes the flag entirely. Any external script using
  `--frames` breaks loudly (`unknown argument`) rather than silently
  changing behavior — this is the intended failure mode.
- **Name interning vs known-set match.** Picked known-set match for
  m1 simplicity. If we ever want guest-defined event names, switch
  to the interner described above.
- **Mode state machines are easy to get subtly wrong.** Mitigated by
  unit tests asserting exact transitions per the constants. When we
  tune the constants, the tests update with them (because they
  reference the constants, not literals).

## Deferred follow-ups noted during design

- `lp-core/* → lpc-*` rename (separate workspace-wide refactor).
- `lpfs` extraction into `lp-base/lpfs/` (separate refactor; second
  inhabitant of `lp-base/`).
- Real Enable/Disable behavior on collectors (m2, when `CpuCollector`
  needs sample-storage gating).
- Shared `PerfEventKind` between `lp-perf` and `lp-riscv-emu` (left
  as duplicate enums for now; merging requires resolving the dep
  direction).
