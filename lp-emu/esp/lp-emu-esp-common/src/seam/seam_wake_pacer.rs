//! When to raise the wake: the host-side pacing rules (G0 rule (b)).
//!
//! The wake (`lp_seam::wake`) is one pending word and one line. The host sets
//! bits, then raises; the guest's handler clears the line, then swaps the
//! word to zero, and its consumer drains every endpoint it was woken for
//! until a take returns nothing. The M0 spike's flood — inject after every
//! take — starved the render loop, so the emulator **paces** what it
//! produces, never an unbounded producer:
//!
//! - **never two raises outstanding**: no raise until the guest has read the
//!   last one (the word reads zero);
//! - **a minimum spacing** between raises, in guest cycles;
//! - **a bounded queue** per endpoint, which refuses and counts rather than
//!   grow ([`super::SeamEndpoint`]);
//! - **a cap on what one take returns**.
//!
//! Pure: this decides *when*; the chip machine does the raising (the word
//! write and the `FROM_CPU_INTR3` raise) and reports what the word reads.

use lp_emu_core::sched::Cycles;

/// The pacing knobs. Guest cycles, so a chip converts from its clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PacerConfig {
    /// The least time between two raises.
    pub min_spacing: Cycles,
    /// Events an endpoint holds before it refuses.
    pub queue_bound: usize,
    /// The most bytes one take returns — and so the largest event an
    /// endpoint accepts. The default (512) is **below a full Ethernet frame
    /// (1514 B)**: a seam that carries frames sets its own config rather than
    /// relying on the default.
    pub take_cap: usize,
}

impl Default for PacerConfig {
    /// 16 000 cycles between raises (100 µs at the C6's 160 MHz), 64 events
    /// per endpoint, 512 bytes per take.
    fn default() -> Self {
        Self {
            min_spacing: 16_000,
            queue_bound: 64,
            take_cap: 512,
        }
    }
}

/// What [`WakePacer::tick`] decided.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tick {
    /// Do nothing this boundary.
    Wait,
    /// Set the pending bits and raise the line now.
    Raise,
}

/// The pacing state of one machine's wake line.
#[derive(Clone, Debug, Default)]
pub struct WakePacer {
    config: PacerConfig,
    /// The cycle of the raise the guest has not yet consumed.
    outstanding_since: Option<Cycles>,
    last_raise: Option<Cycles>,
    raised: u64,
    consumed: u64,
    /// Boundaries at which work was waiting but a rule held the raise back.
    held: u64,
    /// The longest raise → consume seen, in guest cycles (emulated: never a
    /// silicon figure).
    worst_latency: Cycles,
}

impl WakePacer {
    pub fn new(config: PacerConfig) -> Self {
        Self {
            config,
            ..Self::default()
        }
    }

    pub fn config(&self) -> PacerConfig {
        self.config
    }

    /// One slice boundary at guest cycle `now`. `word_is_zero`: the pending
    /// word reads zero (the guest swapped it). `work`: some endpoint holds
    /// inbound events.
    pub fn tick(&mut self, now: Cycles, word_is_zero: bool, work: bool) -> Tick {
        if let Some(since) = self.outstanding_since
            && word_is_zero
        {
            self.consumed += 1;
            self.worst_latency = self.worst_latency.max(now.saturating_sub(since));
            self.outstanding_since = None;
        }
        if !work {
            return Tick::Wait;
        }
        let spaced = self
            .last_raise
            .is_none_or(|at| now.saturating_sub(at) >= self.config.min_spacing);
        if self.outstanding_since.is_some() || !spaced {
            self.held += 1;
            return Tick::Wait;
        }
        self.outstanding_since = Some(now);
        self.last_raise = Some(now);
        self.raised += 1;
        Tick::Raise
    }

    /// The earliest cycle a raise may go, or `None` while one is waiting for
    /// the guest (only the guest's swap can free the line). A run loop bounds
    /// an idle skip by it, so a held raise is not held past its spacing.
    pub fn earliest_raise(&self) -> Option<Cycles> {
        if self.outstanding_since.is_some() {
            return None;
        }
        Some(
            self.last_raise
                .map_or(0, |at| at.saturating_add(self.config.min_spacing)),
        )
    }

    /// A raise is waiting for the guest.
    pub fn outstanding(&self) -> bool {
        self.outstanding_since.is_some()
    }

    pub fn raised(&self) -> u64 {
        self.raised
    }

    pub fn consumed(&self) -> u64 {
        self.consumed
    }

    pub fn held(&self) -> u64 {
        self.held
    }

    pub fn worst_latency(&self) -> Cycles {
        self.worst_latency
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pacer(spacing: Cycles) -> WakePacer {
        WakePacer::new(PacerConfig {
            min_spacing: spacing,
            ..PacerConfig::default()
        })
    }

    #[test]
    fn never_two_raises_outstanding() {
        let mut p = pacer(0);
        assert_eq!(p.tick(10, true, true), Tick::Raise);
        // The guest has not swapped the word: nothing more, however much work.
        for now in 11..100 {
            assert_eq!(p.tick(now, false, true), Tick::Wait);
        }
        assert_eq!(p.raised(), 1);
        // Swapped: consumed, and the next raise may go at once.
        assert_eq!(p.tick(100, true, true), Tick::Raise);
        assert_eq!(p.consumed(), 1);
        assert_eq!(p.worst_latency(), 90);
    }

    #[test]
    fn the_spacing_is_respected() {
        let mut p = pacer(1_000);
        assert_eq!(p.tick(0, true, true), Tick::Raise);
        assert_eq!(p.tick(5, true, true), Tick::Wait, "consumed, but too soon");
        assert_eq!(p.consumed(), 1);
        assert_eq!(p.tick(999, true, true), Tick::Wait);
        assert_eq!(p.tick(1_000, true, true), Tick::Raise);
        assert!(p.held() >= 2);
    }

    #[test]
    fn the_earliest_raise_is_the_spacing_after_the_last_and_none_while_outstanding() {
        let mut p = pacer(1_000);
        assert_eq!(p.earliest_raise(), Some(0));
        p.tick(10, true, true);
        assert_eq!(p.earliest_raise(), None, "outstanding");
        p.tick(20, true, false);
        assert_eq!(p.earliest_raise(), Some(1_010));
    }

    #[test]
    fn no_work_no_raise() {
        let mut p = pacer(0);
        assert_eq!(p.tick(0, true, false), Tick::Wait);
        assert_eq!(p.raised(), 0);
        assert!(!p.outstanding());
    }

    #[test]
    fn an_every_boundary_producer_is_held_to_one_raise_per_spacing() {
        // The spike's flood: work at every boundary, the guest consuming at
        // once. Over 10 000 boundaries, the raises stay at most one a spacing.
        let mut p = pacer(100);
        for now in 0..10_000u64 {
            p.tick(now, true, true);
        }
        assert_eq!(p.raised(), 100);
    }
}
