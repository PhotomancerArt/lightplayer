use lp_perf::{EVENT_FRAME, EVENT_PROJECT_LOAD, EVENT_SHADER_COMPILE};
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
    pub fn new() -> Self {
        Self::default()
    }
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
            "warmup: {STEADY_RENDER_WARMUP_FRAMES} frames; capture: {STEADY_RENDER_CAPTURE_FRAMES} frames"
        )?;
        writeln!(w, "frame_begins observed: {}", self.frame_begins)?;
        writeln!(
            w,
            "saw_compile_after_start: {}",
            self.saw_compile_after_start
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_perf::EVENT_SHADER_LINK;
    use lp_riscv_emu::profile::PerfEvent;

    fn frame_begin() -> PerfEvent {
        PerfEvent {
            cycle: 0,
            name: EVENT_FRAME,
            kind: PerfEventKind::Begin,
        }
    }

    #[test]
    fn enable_after_warmup_plus_one_frame_begins() {
        let mut g = SteadyRenderGate::new();
        assert_eq!(g.on_event(&frame_begin()), GateAction::NoChange);
        assert_eq!(g.on_event(&frame_begin()), GateAction::NoChange);
        assert_eq!(g.on_event(&frame_begin()), GateAction::Enable);
    }

    #[test]
    fn stop_after_warmup_plus_capture_plus_one_frame_begins() {
        let mut g = SteadyRenderGate::new();
        let total = STEADY_RENDER_WARMUP_FRAMES + STEADY_RENDER_CAPTURE_FRAMES;
        for _ in 0..total {
            assert_ne!(g.on_event(&frame_begin()), GateAction::Stop);
        }
        assert_eq!(g.on_event(&frame_begin()), GateAction::Stop);
    }

    #[test]
    fn shader_compile_begin_resets_warmup() {
        let mut g = SteadyRenderGate::new();
        assert_eq!(g.on_event(&frame_begin()), GateAction::NoChange);
        assert_eq!(g.on_event(&frame_begin()), GateAction::NoChange);
        assert_eq!(
            g.on_event(&PerfEvent {
                cycle: 0,
                name: EVENT_SHADER_COMPILE,
                kind: PerfEventKind::Begin,
            }),
            GateAction::NoChange
        );
        assert_eq!(g.on_event(&frame_begin()), GateAction::NoChange);
        assert_eq!(g.on_event(&frame_begin()), GateAction::NoChange);
        assert_eq!(g.on_event(&frame_begin()), GateAction::Enable);
    }

    #[test]
    fn non_frame_events_return_no_change() {
        let mut g = SteadyRenderGate::new();
        assert_eq!(
            g.on_event(&PerfEvent {
                cycle: 0,
                name: EVENT_SHADER_LINK,
                kind: PerfEventKind::Instant,
            }),
            GateAction::NoChange
        );
        assert_eq!(
            g.on_event(&PerfEvent {
                cycle: 0,
                name: EVENT_PROJECT_LOAD,
                kind: PerfEventKind::End,
            }),
            GateAction::NoChange
        );
    }

    #[test]
    fn does_not_enable_on_profile_start() {
        let mut g = SteadyRenderGate::new();
        let evt = PerfEvent {
            cycle: 0,
            name: lp_riscv_emu::profile::perf_event::EVENT_PROFILE_START,
            kind: PerfEventKind::Instant,
        };
        assert_eq!(g.on_event(&evt), GateAction::NoChange);
    }
}
