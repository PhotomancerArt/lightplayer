//! Per-endpoint wake counters.
//!
//! **Emulated figures, always labelled so**: guest cycles on this machine's
//! clock, never a silicon latency. A wake's raise → consume time here is the
//! emulator's scheduling of a model, and a desk sitting is what would measure
//! the real one.

use lp_emu_core::sched::Cycles;

/// One endpoint's wake history.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WakeStats {
    /// Raises whose pending bits included this endpoint's.
    pub raised: u64,
    /// Of those, the ones the guest consumed (its word read zero).
    pub consumed: u64,
    /// Bytes the guest took from this endpoint.
    pub bytes_taken: u64,
    /// The longest raise → consume, in **emulated** guest cycles.
    pub worst_latency: Cycles,
    /// The raise still waiting for the guest.
    pub(crate) outstanding_since: Option<Cycles>,
}

impl WakeStats {
    pub(crate) fn raise(&mut self, now: Cycles) {
        self.raised += 1;
        self.outstanding_since.get_or_insert(now);
    }

    pub(crate) fn consume(&mut self, now: Cycles) {
        if let Some(since) = self.outstanding_since.take() {
            self.consumed += 1;
            self.worst_latency = self.worst_latency.max(now.saturating_sub(since));
        }
    }

    /// One line for a report, emulated and labelled so.
    pub fn line(&self, endpoint: &str, refused: u64) -> String {
        format!(
            "seam wake {endpoint}: raised {}, consumed {}, refused {refused}, {} B taken, \
             worst raise->consume {} cycles (emulated)",
            self.raised, self.consumed, self.bytes_taken, self.worst_latency
        )
    }
}
