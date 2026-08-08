use lp_emu_core::profile::{Gate, GateAction, PerfEvent, PerfEventKind};
use lp_perf::{EVENT_FRAME, EVENT_SHADER_COMPILE};

/// Safety net: stop after this many `frame` Begins if no shader compile
/// ever fires (e.g. a project with no shader nodes). The compile-window
/// deferral runs the compile during frame 2, so this is generous.
pub const COMPILE_MAX_FRAMES: u32 = 8;

/// Stops at the End of the frame containing the shader compile.
///
/// The compile-window deferral (see
/// docs/adr/2026-08-03-memory-pressure-at-compile-safe-points.md) means the
/// first render only requests a compile window; the compile itself runs in a
/// later frame. So the gate cannot stop after a fixed frame count — it waits
/// for a `shader-compile` End bracket and stops at that frame's End, which
/// also captures the `shader-link` brackets nested inside the compile.
#[derive(Default)]
pub struct CompileGate {
    frame_begins: u32,
    saw_compile_end: bool,
}

impl CompileGate {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Gate for CompileGate {
    fn on_event(&mut self, evt: &PerfEvent) -> GateAction {
        if evt.name == lp_emu_core::profile::perf_event::EVENT_PROFILE_START {
            return GateAction::Enable;
        }
        match (evt.name, evt.kind) {
            (EVENT_SHADER_COMPILE, PerfEventKind::End) => {
                self.saw_compile_end = true;
                GateAction::NoChange
            }
            (EVENT_FRAME, PerfEventKind::End) if self.saw_compile_end => GateAction::Stop,
            (EVENT_FRAME, PerfEventKind::Begin) => {
                self.frame_begins += 1;
                if self.frame_begins > COMPILE_MAX_FRAMES {
                    GateAction::Stop
                } else {
                    GateAction::NoChange
                }
            }
            _ => GateAction::NoChange,
        }
    }

    fn report_section(&self, w: &mut dyn std::fmt::Write) -> std::fmt::Result {
        writeln!(w, "mode: compile")?;
        writeln!(w, "saw_shader_compile_end: {}", self.saw_compile_end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_emu_core::profile::PerfEvent;
    use lp_perf::{EVENT_PROJECT_LOAD, EVENT_SHADER_LINK};

    fn evt(name: &'static str, kind: PerfEventKind) -> PerfEvent {
        PerfEvent {
            cycle: 0,
            name,
            kind,
        }
    }

    fn frame_begin() -> PerfEvent {
        evt(EVENT_FRAME, PerfEventKind::Begin)
    }

    fn frame_end() -> PerfEvent {
        evt(EVENT_FRAME, PerfEventKind::End)
    }

    /// The deferred-compile shape: frame 1 only requests the window, the
    /// compile runs during frame 2. The gate must not stop until the End
    /// of the frame containing the compile.
    #[test]
    fn stops_on_frame_end_containing_compile_end() {
        let mut g = CompileGate::new();
        // Frame 1: compile deferred, nothing but the frame bracket.
        assert_eq!(g.on_event(&frame_begin()), GateAction::NoChange);
        assert_eq!(g.on_event(&frame_end()), GateAction::NoChange);
        // Frame 2: the compile (with nested link) runs.
        assert_eq!(g.on_event(&frame_begin()), GateAction::NoChange);
        assert_eq!(
            g.on_event(&evt(EVENT_SHADER_COMPILE, PerfEventKind::Begin)),
            GateAction::NoChange
        );
        assert_eq!(
            g.on_event(&evt(EVENT_SHADER_LINK, PerfEventKind::Begin)),
            GateAction::NoChange
        );
        assert_eq!(
            g.on_event(&evt(EVENT_SHADER_LINK, PerfEventKind::End)),
            GateAction::NoChange
        );
        assert_eq!(
            g.on_event(&evt(EVENT_SHADER_COMPILE, PerfEventKind::End)),
            GateAction::NoChange
        );
        assert_eq!(g.on_event(&frame_end()), GateAction::Stop);
    }

    #[test]
    fn compile_in_first_frame_still_stops_at_its_end() {
        let mut g = CompileGate::new();
        assert_eq!(g.on_event(&frame_begin()), GateAction::NoChange);
        assert_eq!(
            g.on_event(&evt(EVENT_SHADER_COMPILE, PerfEventKind::Begin)),
            GateAction::NoChange
        );
        assert_eq!(
            g.on_event(&evt(EVENT_SHADER_COMPILE, PerfEventKind::End)),
            GateAction::NoChange
        );
        assert_eq!(g.on_event(&frame_end()), GateAction::Stop);
    }

    #[test]
    fn no_compile_stops_at_max_frames() {
        let mut g = CompileGate::new();
        for _ in 0..COMPILE_MAX_FRAMES {
            assert_eq!(g.on_event(&frame_begin()), GateAction::NoChange);
            assert_eq!(g.on_event(&frame_end()), GateAction::NoChange);
        }
        assert_eq!(g.on_event(&frame_begin()), GateAction::Stop);
    }

    #[test]
    fn project_load_events_do_not_stop() {
        let mut g = CompileGate::new();
        assert_eq!(
            g.on_event(&evt(EVENT_PROJECT_LOAD, PerfEventKind::Begin)),
            GateAction::NoChange
        );
        assert_eq!(
            g.on_event(&evt(EVENT_PROJECT_LOAD, PerfEventKind::End)),
            GateAction::NoChange
        );
        assert_eq!(g.on_event(&frame_begin()), GateAction::NoChange);
    }

    #[test]
    fn compile_begin_alone_does_not_stop() {
        let mut g = CompileGate::new();
        assert_eq!(g.on_event(&frame_begin()), GateAction::NoChange);
        for _ in 0..10 {
            assert_eq!(
                g.on_event(&evt(EVENT_SHADER_COMPILE, PerfEventKind::Begin)),
                GateAction::NoChange
            );
        }
        // No compile End yet: the frame End must not stop the trace.
        assert_eq!(g.on_event(&frame_end()), GateAction::NoChange);
    }

    #[test]
    fn enables_on_profile_start() {
        let mut g = CompileGate::new();
        let start = evt(
            lp_emu_core::profile::perf_event::EVENT_PROFILE_START,
            PerfEventKind::Instant,
        );
        assert_eq!(g.on_event(&start), GateAction::Enable);
    }
}
