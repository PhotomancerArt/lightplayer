use lp_perf::EVENT_FRAME;
use lp_riscv_emu::profile::{Gate, GateAction, PerfEvent, PerfEventKind};

#[derive(Default)]
pub struct StartupGate {
    first_frame_ended: bool,
}

impl StartupGate {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Gate for StartupGate {
    fn on_event(&mut self, evt: &PerfEvent) -> GateAction {
        if evt.name == lp_riscv_emu::profile::perf_event::EVENT_PROFILE_START {
            return GateAction::Enable;
        }
        if evt.name == EVENT_FRAME && evt.kind == PerfEventKind::End {
            if self.first_frame_ended {
                return GateAction::NoChange;
            }
            self.first_frame_ended = true;
            return GateAction::Stop;
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
    use lp_riscv_emu::profile::{PerfEvent, PerfEventKind};

    #[test]
    fn stops_on_first_frame_end() {
        let mut g = StartupGate::new();
        let frame_end = PerfEvent {
            cycle: 0,
            name: EVENT_FRAME,
            kind: PerfEventKind::End,
        };
        assert_eq!(g.on_event(&frame_end), GateAction::Stop);
    }

    #[test]
    fn enables_on_profile_start() {
        let mut g = StartupGate::new();
        let evt = PerfEvent {
            cycle: 0,
            name: lp_riscv_emu::profile::perf_event::EVENT_PROFILE_START,
            kind: PerfEventKind::Instant,
        };
        assert_eq!(g.on_event(&evt), GateAction::Enable);
    }
}
