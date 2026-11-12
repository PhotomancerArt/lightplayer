# Phase 6 — Mode system

Build the `ProfileMode` enum and the per-mode `Gate` state machines
in `lp-cli/src/commands/profile/mode/`. Each gate is a small finite
state machine over the perf-event stream, returning `GateAction`.

This phase can run in parallel with phases 3 and 5.

## Subagent assignment

`generalPurpose` subagent. New module with one file per mode + a
mod-level dispatcher. Per-mode logic is pinned in the design doc;
unit tests are mechanical.

## Files to create

```
lp-cli/src/commands/profile/
└── mode/
    ├── mod.rs                # NEW: enum + dispatcher + Gate trait re-export
    ├── steady_render.rs      # NEW
    ├── compile.rs            # NEW
    ├── startup.rs            # NEW
    └── all.rs                # NEW
```

Also touch:

```
lp-cli/src/commands/profile/mod.rs       # UPDATE: + pub mod mode;
```

## Contents

### `mode/mod.rs`

```rust
//! Profile mode state machines.
//!
//! Each mode is a `Gate` (defined in `lp-riscv-emu::profile`) that
//! observes the perf-event stream and decides when to start/stop
//! collection. m1 honors only `Stop`; `Enable`/`Disable` semantics
//! arrive in m2.

use clap::ValueEnum;
use lp_riscv_emu::profile::{Gate, GateAction, PerfEvent};

pub mod all;
pub mod compile;
pub mod startup;
pub mod steady_render;

/// Selectable profile modes for `lp-cli profile --mode <…>`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
#[clap(rename_all = "kebab-case")]
pub enum ProfileMode {
    /// Capture N stable frames after warmup. Default.
    SteadyRender,
    /// Capture shader compile/link work, stop after the run completes.
    Compile,
    /// Capture project-load + first frame.
    Startup,
    /// Capture everything; stop only on guest halt or --max-cycles.
    All,
}

impl ProfileMode {
    pub fn build_gate(self) -> Box<dyn Gate> {
        match self {
            ProfileMode::SteadyRender => Box::new(steady_render::SteadyRenderGate::new()),
            ProfileMode::Compile      => Box::new(compile::CompileGate::new()),
            ProfileMode::Startup      => Box::new(startup::StartupGate::new()),
            ProfileMode::All          => Box::new(all::AllGate::new()),
        }
    }

    pub fn slug(self) -> &'static str {
        match self {
            ProfileMode::SteadyRender => "steady-render",
            ProfileMode::Compile      => "compile",
            ProfileMode::Startup      => "startup",
            ProfileMode::All          => "all",
        }
    }
}
```

### `mode/steady_render.rs`

Per design doc, "`SteadyRenderGate`" section:

```rust
use lp_perf::{EVENT_FRAME, EVENT_SHADER_COMPILE, EVENT_PROJECT_LOAD};
use lp_riscv_emu::profile::{Gate, GateAction, PerfEvent, PerfEventKind};

/// Number of `frame` Begin events to skip before starting capture.
pub const STEADY_RENDER_WARMUP_FRAMES: u32 = 2;

/// Number of `frame` Begin events to capture after warmup.
pub const STEADY_RENDER_CAPTURE_FRAMES: u32 = 4;

#[derive(Default)]
pub struct SteadyRenderGate {
    frame_begins: u32,
    /// Set true once the first non-startup frame has been observed.
    /// Resets warmup counter if a shader-compile fires (re-warmup).
    saw_compile_after_start: bool,
}

impl SteadyRenderGate {
    pub fn new() -> Self { Self::default() }
}

impl Gate for SteadyRenderGate {
    fn on_event(&mut self, evt: &PerfEvent) -> GateAction {
        match (evt.name, evt.kind) {
            (EVENT_PROJECT_LOAD, _) => GateAction::NoChange,

            // A shader-compile mid-run means the system is unstable;
            // restart warmup. (m1 logs only — see GateAction::Disable
            // semantics deferred to m2.)
            (EVENT_SHADER_COMPILE, PerfEventKind::Begin) => {
                self.frame_begins = 0;
                self.saw_compile_after_start = true;
                GateAction::NoChange
            }

            (EVENT_FRAME, PerfEventKind::Begin) => {
                self.frame_begins += 1;
                let total = STEADY_RENDER_WARMUP_FRAMES + STEADY_RENDER_CAPTURE_FRAMES;
                if self.frame_begins == STEADY_RENDER_WARMUP_FRAMES + 1 {
                    // First captured frame.
                    GateAction::Enable
                } else if self.frame_begins > total {
                    GateAction::Stop
                } else {
                    GateAction::NoChange
                }
            }

            _ => GateAction::NoChange,
        }
    }

    fn report_section(&self, w: &mut dyn std::fmt::Write) -> std::fmt::Result {
        writeln!(w, "mode: steady-render")?;
        writeln!(
            w,
            "warmup: {} frames; capture: {} frames",
            STEADY_RENDER_WARMUP_FRAMES, STEADY_RENDER_CAPTURE_FRAMES
        )?;
        writeln!(w, "frame_begins observed: {}", self.frame_begins)
    }
}
```

Unit tests (in `#[cfg(test)] mod tests` at the bottom):
- After `WARMUP+1` frame Begins, returns `Enable`.
- After `WARMUP+CAPTURE+1` frame Begins, returns `Stop`.
- A `shader-compile` Begin in the middle resets the counter.
- Non-frame events return `NoChange`.

### `mode/compile.rs`

Per design doc. Stops when:
- The first `EVENT_FRAME` Begin arrives (we've passed the load /
  initial compile phase), OR
- The guest halts naturally.

```rust
use lp_perf::{EVENT_FRAME, EVENT_PROJECT_LOAD};
use lp_riscv_emu::profile::{Gate, GateAction, PerfEvent, PerfEventKind};

#[derive(Default)]
pub struct CompileGate { saw_first_frame: bool }

impl CompileGate { pub fn new() -> Self { Self::default() } }

impl Gate for CompileGate {
    fn on_event(&mut self, evt: &PerfEvent) -> GateAction {
        if evt.name == EVENT_FRAME && evt.kind == PerfEventKind::Begin {
            if self.saw_first_frame {
                return GateAction::Stop;
            }
            self.saw_first_frame = true;
        }
        GateAction::NoChange
    }
    fn report_section(&self, w: &mut dyn std::fmt::Write) -> std::fmt::Result {
        writeln!(w, "mode: compile")
    }
}
```

(Stops on the *second* frame begin so the first frame's compile work
is fully captured.)

Unit tests: stops on second frame begin; captures project-load
events; doesn't stop on shader-compile events alone.

### `mode/startup.rs`

Stops after `EVENT_FRAME::End` for the first frame:

```rust
use lp_perf::EVENT_FRAME;
use lp_riscv_emu::profile::{Gate, GateAction, PerfEvent, PerfEventKind};

#[derive(Default)]
pub struct StartupGate { first_frame_ended: bool }

impl StartupGate { pub fn new() -> Self { Self::default() } }

impl Gate for StartupGate {
    fn on_event(&mut self, evt: &PerfEvent) -> GateAction {
        if evt.name == EVENT_FRAME && evt.kind == PerfEventKind::End {
            if self.first_frame_ended { return GateAction::NoChange; }
            self.first_frame_ended = true;
            return GateAction::Stop;
        }
        GateAction::NoChange
    }
    fn report_section(&self, w: &mut dyn std::fmt::Write) -> std::fmt::Result {
        writeln!(w, "mode: startup")
    }
}
```

Unit test: stops on first FRAME::End.

### `mode/all.rs`

Never stops on its own. Lets `--max-cycles` or natural halt end the
run.

```rust
use lp_riscv_emu::profile::{Gate, GateAction, PerfEvent};

#[derive(Default)]
pub struct AllGate;

impl AllGate { pub fn new() -> Self { Self } }

impl Gate for AllGate {
    fn on_event(&mut self, _evt: &PerfEvent) -> GateAction {
        GateAction::NoChange
    }
    fn report_section(&self, w: &mut dyn std::fmt::Write) -> std::fmt::Result {
        writeln!(w, "mode: all")
    }
}
```

Unit test: returns `NoChange` for arbitrary events.

### `lp-cli/src/commands/profile/mod.rs`

Add `pub mod mode;` next to the existing module declarations
(`pub mod args; pub mod handler; ...`).

## Validation

```bash
cargo check -p lp-cli
cargo build -p lp-cli
cargo test  -p lp-cli mode::
```

All four mode unit-test modules should pass. No e2e tests in this
phase — the gate is exercised end-to-end in phase 8.

## Out of scope for this phase

- Wiring `ProfileMode` into the CLI handler (phase 7).
- `--mode` CLI argument parsing (phase 7).
- Real `Enable`/`Disable` semantics (deferred to m2 per design doc).
