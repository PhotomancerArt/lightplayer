use lp_emu_core::profile::{Gate, GateAction, PerfEvent, PerfEventKind};
use lp_perf::{EVENT_FRAME, EVENT_SHADER_COMPILE};

/// Capture project-load through the first *compiled* frame.
///
/// Shader compiles are deferred one frame for the memory-pressure compile
/// window (ADR 2026-08-03-memory-pressure-at-compile-safe-points): frame 1
/// requests the window and renders fallback, frame 2 compiles and renders
/// for real. Startup cost therefore spans load + both frames, so this gate
/// stops at the end of the frame that contained the first shader-compile —
/// or after two frames when the project compiles nothing.
#[derive(Default)]
pub struct StartupGate {
    frames_ended: u32,
    saw_compile_end: bool,
}

impl StartupGate {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Gate for StartupGate {
    fn on_event(&mut self, evt: &PerfEvent) -> GateAction {
        if evt.name == lp_emu_core::profile::perf_event::EVENT_PROFILE_START {
            return GateAction::Enable;
        }
        if evt.name == EVENT_SHADER_COMPILE && evt.kind == PerfEventKind::End {
            self.saw_compile_end = true;
            return GateAction::NoChange;
        }
        if evt.name == EVENT_FRAME && evt.kind == PerfEventKind::End {
            self.frames_ended += 1;
            if self.saw_compile_end || self.frames_ended >= 2 {
                return GateAction::Stop;
            }
        }
        GateAction::NoChange
    }

    fn report_section(&self, w: &mut dyn std::fmt::Write) -> std::fmt::Result {
        writeln!(w, "mode: startup")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_emu_core::profile::{PerfEvent, PerfEventKind};

    fn frame_end() -> PerfEvent {
        PerfEvent {
            cycle: 0,
            name: EVENT_FRAME,
            kind: PerfEventKind::End,
        }
    }

    #[test]
    fn stops_at_the_frame_containing_the_first_compile() {
        let mut g = StartupGate::new();
        // Frame 1: the deferral frame — no compile yet, keep going.
        assert_eq!(g.on_event(&frame_end()), GateAction::NoChange);
        // Frame 2: the compile window.
        assert_eq!(
            g.on_event(&PerfEvent {
                cycle: 0,
                name: EVENT_SHADER_COMPILE,
                kind: PerfEventKind::End,
            }),
            GateAction::NoChange
        );
        assert_eq!(g.on_event(&frame_end()), GateAction::Stop);
    }

    #[test]
    fn stops_after_two_frames_when_nothing_compiles() {
        let mut g = StartupGate::new();
        assert_eq!(g.on_event(&frame_end()), GateAction::NoChange);
        assert_eq!(g.on_event(&frame_end()), GateAction::Stop);
    }
}
