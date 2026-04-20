use lp_perf::EVENT_FRAME;
use lp_riscv_emu::profile::{Gate, GateAction, PerfEvent, PerfEventKind};

#[derive(Default)]
pub struct CompileGate {
    saw_first_frame: bool,
}

impl CompileGate {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Gate for CompileGate {
    fn on_event(&mut self, evt: &PerfEvent) -> GateAction {
        if evt.name == lp_riscv_emu::profile::perf_event::EVENT_PROFILE_START {
            return GateAction::Enable;
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use lp_perf::{EVENT_PROJECT_LOAD, EVENT_SHADER_COMPILE};
    use lp_riscv_emu::profile::PerfEvent;

    fn frame_begin() -> PerfEvent {
        PerfEvent {
            cycle: 0,
            name: EVENT_FRAME,
            kind: PerfEventKind::Begin,
        }
    }

    #[test]
    fn stops_on_second_frame_begin() {
        let mut g = CompileGate::new();
        assert_eq!(g.on_event(&frame_begin()), GateAction::NoChange);
        assert_eq!(g.on_event(&frame_begin()), GateAction::Stop);
    }

    #[test]
    fn project_load_events_do_not_stop() {
        let mut g = CompileGate::new();
        let load_begin = PerfEvent {
            cycle: 0,
            name: EVENT_PROJECT_LOAD,
            kind: PerfEventKind::Begin,
        };
        let load_end = PerfEvent {
            cycle: 0,
            name: EVENT_PROJECT_LOAD,
            kind: PerfEventKind::End,
        };
        assert_eq!(g.on_event(&load_begin), GateAction::NoChange);
        assert_eq!(g.on_event(&load_end), GateAction::NoChange);
        assert_eq!(g.on_event(&frame_begin()), GateAction::NoChange);
        assert_eq!(g.on_event(&frame_begin()), GateAction::Stop);
    }

    #[test]
    fn shader_compile_alone_does_not_stop() {
        let mut g = CompileGate::new();
        let compile_begin = PerfEvent {
            cycle: 0,
            name: EVENT_SHADER_COMPILE,
            kind: PerfEventKind::Begin,
        };
        for _ in 0..10 {
            assert_eq!(g.on_event(&compile_begin), GateAction::NoChange);
        }
        assert_eq!(g.on_event(&frame_begin()), GateAction::NoChange);
        assert_eq!(g.on_event(&frame_begin()), GateAction::Stop);
    }

    #[test]
    fn enables_on_profile_start() {
        let mut g = CompileGate::new();
        let evt = PerfEvent {
            cycle: 0,
            name: lp_riscv_emu::profile::perf_event::EVENT_PROFILE_START,
            kind: PerfEventKind::Instant,
        };
        assert_eq!(g.on_event(&evt), GateAction::Enable);
    }
}
