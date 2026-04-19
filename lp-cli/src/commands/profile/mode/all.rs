use lp_riscv_emu::profile::{Gate, GateAction, PerfEvent};

#[derive(Default)]
pub struct AllGate;

impl AllGate {
    pub fn new() -> Self {
        Self
    }
}

impl Gate for AllGate {
    fn on_event(&mut self, _evt: &PerfEvent) -> GateAction {
        GateAction::NoChange
    }

    fn report_section(&self, w: &mut dyn std::fmt::Write) -> std::fmt::Result {
        writeln!(w, "mode: all")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_perf::{EVENT_FRAME, EVENT_SHADER_COMPILE, EVENT_SHADER_LINK};
    use lp_riscv_emu::profile::{PerfEvent, PerfEventKind};

    #[test]
    fn no_change_for_various_events() {
        let mut g = AllGate::new();
        let events = [
            PerfEvent {
                cycle: 0,
                name: EVENT_FRAME,
                kind: PerfEventKind::Begin,
            },
            PerfEvent {
                cycle: 0,
                name: EVENT_FRAME,
                kind: PerfEventKind::End,
            },
            PerfEvent {
                cycle: 0,
                name: EVENT_SHADER_COMPILE,
                kind: PerfEventKind::Begin,
            },
            PerfEvent {
                cycle: 0,
                name: EVENT_SHADER_LINK,
                kind: PerfEventKind::Instant,
            },
        ];
        for evt in &events {
            assert_eq!(g.on_event(evt), GateAction::NoChange);
        }
    }
}
