use lp_perf::EVENT_PROJECT_LOAD;
use lp_riscv_emu::profile::{Gate, GateAction, PerfEvent, PerfEventKind};

#[derive(Default)]
pub struct ProjectLoadGate {
    in_project_load: bool,
    completed: bool,
}

impl ProjectLoadGate {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Gate for ProjectLoadGate {
    fn on_event(&mut self, evt: &PerfEvent) -> GateAction {
        match (evt.name, evt.kind) {
            (EVENT_PROJECT_LOAD, PerfEventKind::Begin) if !self.completed => {
                self.in_project_load = true;
                GateAction::Enable
            }
            (EVENT_PROJECT_LOAD, PerfEventKind::End) if self.in_project_load => {
                self.in_project_load = false;
                self.completed = true;
                GateAction::Stop
            }
            _ => GateAction::NoChange,
        }
    }

    fn report_section(&self, w: &mut dyn std::fmt::Write) -> std::fmt::Result {
        writeln!(w, "mode: project-load")?;
        writeln!(w, "completed: {}", self.completed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_perf::EVENT_FRAME;
    use lp_riscv_emu::profile::{PerfEvent, PerfEventKind};

    fn event(name: &'static str, kind: PerfEventKind) -> PerfEvent {
        PerfEvent {
            cycle: 0,
            name,
            kind,
        }
    }

    #[test]
    fn enables_on_project_load_begin_and_stops_on_end() {
        let mut g = ProjectLoadGate::new();
        assert_eq!(
            g.on_event(&event(EVENT_PROJECT_LOAD, PerfEventKind::Begin)),
            GateAction::Enable
        );
        assert_eq!(
            g.on_event(&event(EVENT_PROJECT_LOAD, PerfEventKind::End)),
            GateAction::Stop
        );
    }

    #[test]
    fn ignores_frame_events() {
        let mut g = ProjectLoadGate::new();
        assert_eq!(
            g.on_event(&event(EVENT_FRAME, PerfEventKind::Begin)),
            GateAction::NoChange
        );
    }
}
