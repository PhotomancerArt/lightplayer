//! The wake's emulator half: set bits in the pending word, then raise
//! `FROM_CPU_INTR3` the way another CPU would (`lp_seam::wake`).
//!
//! At the top of each slice, while an engaged capability seam has an endpoint
//! and the live table names a pending word (an idle skip never sleeps past
//! the moment the pacer would next allow a raise):
//!
//! 1. read the word; zero means the guest's handler swapped it, so the last
//!    raise is **consumed**;
//! 2. ask the pacer (`lp_emu_esp_common::seam::WakePacer`, G0 rule (b)):
//!    never two raises outstanding, a minimum spacing, and only when an
//!    endpoint has work: an inbound event, or on the network endpoint a
//!    station event (a join's outcome, a scan's end) waiting for
//!    `net_event_take`;
//! 3. on a raise, OR every waiting endpoint's bit into the word — a
//!    read-modify-write between two guest instructions, so atomic: the guest
//!    is not running — then write `INTPRI.cpu_intr_from_cpu_3 = 1`, which is
//!    the level of the wake source. The guest's handler clears it.
//!
//! **G0 rule (a)** — whatever a wake wakes runs on the firmware's IO thread,
//! never the main or render executor — is a *firmware* rule and nothing here
//! can enforce it. The Bluetooth seam's plan builds the firmware API that
//! does; the ADR (`docs/adr/2026-10-05-emulator-seams.md`, §7) records both
//! rules.
//!
//! A lost or doubled wake that this protocol cannot explain is the roadmap's
//! **R-WAKE**: stop, and take it back to a decision.

use lp_emu_core::sched::Cycles;
use lp_emu_esp_common::seam::Tick;

use crate::machine::Esp32C6Machine;
use crate::memmap;

/// `INTPRI.cpu_intr_from_cpu_n` is at `+0x90 + 4n`; its bit 0 is the level of
/// source `FROM_CPU_INTR0 + n`.
const CPU_INTR_FROM_CPU0: u32 = 0x90;

impl Esp32C6Machine {
    /// Whether this chip start has a wake to drive at all. False on every
    /// seam-off run and on every image without a pending word.
    pub(crate) fn seam_wake_armed(&self) -> bool {
        self.seams.pending != 0 && !self.seams.endpoints.is_empty()
    }

    /// When the run loop must next look at the seams: the pacer's earliest
    /// raise while an endpoint has work and no raise is waiting, so an idle
    /// skip never sleeps past a raise the spacing allows; and, on a LAN this
    /// machine drives itself, the LAN's next due cycle, so a guest asleep in
    /// `wfi` never sleeps through a join landing or a frame arriving
    /// ([`super::net_seam`]). `None` on a seam-off run.
    pub(crate) fn seam_wake_deadline(&self) -> Option<Cycles> {
        let wake = if self.seam_wake_armed() && self.seam_work_bits() != 0 {
            self.seams.pacer.earliest_raise()
        } else {
            None
        };
        [wake, self.net_deadline()].into_iter().flatten().min()
    }

    /// The pending bits of every endpoint with work for the guest: inbound
    /// events, or — on the network endpoint — a station event waiting.
    fn seam_work_bits(&self) -> u32 {
        self.seams
            .endpoints
            .iter()
            .enumerate()
            .filter(|(i, e)| e.has_inbound() || self.net_has_event(*i))
            .fold(0, |bits, (_, e)| bits | e.bit)
    }

    /// One look at the wake, at the top of a slice. `true` when it raised the
    /// line (the caller resamples the matrix). See [the module docs](self).
    pub(crate) fn seam_wake_tick(&mut self, now: Cycles) -> bool {
        let addr = self.seams.pending;
        let word = self.peek_word(addr).unwrap_or(0);
        let word_is_zero = word == 0;
        if word_is_zero {
            for s in &mut self.seams.wake_stats {
                s.consume(now);
            }
        }
        let waiting = self.seam_work_bits();
        if self.seams.pacer.tick(now, word_is_zero, waiting != 0) != Tick::Raise {
            return false;
        }
        for (e, s) in self.seams.endpoints.iter().zip(&mut self.seams.wake_stats) {
            if waiting & e.bit != 0 {
                s.raise(now);
            }
        }
        self.poke_word(addr, word | waiting);
        let line = u32::from(lp_seam::wake::WAKE_FROM_CPU_INTR);
        self.poke_word(memmap::periph::INTPRI + CPU_INTR_FROM_CPU0 + 4 * line, 1);
        true
    }
}
