//! Profile mode state machines.
//!
//! Each mode is a `Gate` (defined in `lp-riscv-emu::profile`) that
//! observes the perf-event stream and decides when to start/stop
//! collection. m1 honors only `Stop`; `Enable`/`Disable` semantics
//! arrive in m2.

use clap::ValueEnum;

pub mod all;
pub mod compile;
pub mod startup;
pub mod steady_render;

pub use lp_riscv_emu::profile::Gate;

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
            ProfileMode::Compile => Box::new(compile::CompileGate::new()),
            ProfileMode::Startup => Box::new(startup::StartupGate::new()),
            ProfileMode::All => Box::new(all::AllGate::new()),
        }
    }

    pub fn slug(self) -> &'static str {
        match self {
            ProfileMode::SteadyRender => "steady-render",
            ProfileMode::Compile => "compile",
            ProfileMode::Startup => "startup",
            ProfileMode::All => "all",
        }
    }
}

#[cfg(test)]
mod profile_mode_smoke {
    use super::ProfileMode;
    use lp_perf::EVENT_FRAME;
    use lp_riscv_emu::profile::{PerfEvent, PerfEventKind};

    #[test]
    fn build_gate_slug_and_on_event_smoke() {
        for mode in [
            ProfileMode::SteadyRender,
            ProfileMode::Compile,
            ProfileMode::Startup,
            ProfileMode::All,
        ] {
            let _ = mode.slug();
            let mut gate = mode.build_gate();
            let _ = gate.on_event(&PerfEvent {
                cycle: 0,
                name: EVENT_FRAME,
                kind: PerfEventKind::Begin,
            });
        }
    }
}
