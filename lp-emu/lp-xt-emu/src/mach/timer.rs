//! `CCOUNT` and the three `CCOMPARE` timers (ISA RM §4.4.6, Tables 5-175 and
//! 5-176).
//!
//! `CCOUNT` is the hart's cycle counter viewed through a writable offset:
//! `CCOUNT = cycle_count + offset`, so a `wsr.ccount` moves the view and
//! never the counter (the counter is monotonic; `advance_to_cycle` depends on
//! it). Timer `i` requests its interrupt when `CCOUNT` **reaches**
//! `CCOMPARE[i]` — the match is one cycle wide and the request is
//! remembered (§4.4.6.2) — and the request is cleared by writing
//! `CCOMPARE[i]`.
//!
//! Both are undefined at reset (§4.4.6.2, and `xtensa-lx-rt-0.22.0/src/
//! lib.rs:165-167` says so in as many words, which is why the runtime writes
//! `CCOMPARE0..2 = 0` itself at boot). They reset to 0 here: a choice the
//! architecture permits, not a measured value.

use super::trap::NUM_TIMERS;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Timers {
    pub ccompare: [u32; NUM_TIMERS],
    /// `CCOUNT - cycle_count`, as a wrapping 32-bit difference.
    offset: u32,
    /// The absolute cycle at which each timer next matches, or `u64::MAX`
    /// once it has fired and until its `CCOMPARE` is rewritten.
    match_at: [u64; NUM_TIMERS],
    /// `min(match_at)`, so the slice loop pays one compare per instruction.
    next: u64,
}

impl Timers {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            ccompare: [0; NUM_TIMERS],
            offset: 0,
            match_at: [u64::MAX; NUM_TIMERS],
            next: u64::MAX,
        }
    }

    /// `CCOUNT` at cycle `now`.
    #[inline]
    #[must_use]
    pub const fn ccount(&self, now: u64) -> u32 {
        (now as u32).wrapping_add(self.offset)
    }

    /// `wsr.ccount`: move the view so `CCOUNT == value` at cycle `now`, and
    /// re-derive every match.
    pub fn write_ccount(&mut self, now: u64, value: u32) {
        self.offset = value.wrapping_sub(now as u32);
        for i in 0..NUM_TIMERS {
            self.rearm(i, now);
        }
    }

    /// `wsr.ccompare[i]`: set the compare value; the timer's request is
    /// cleared by the caller (it lives in the interrupt unit).
    pub fn write_ccompare(&mut self, now: u64, i: usize, value: u32) {
        if let Some(c) = self.ccompare.get_mut(i) {
            *c = value;
            self.rearm(i, now);
        }
    }

    /// The next cycle at which `CCOUNT == CCOMPARE[i]`, strictly after
    /// `now`. A compare equal to the current count matches a full wrap
    /// later: the match is on the count *reaching* the value, and the count
    /// at `now` has already been reached.
    fn rearm(&mut self, i: usize, now: u64) {
        let dist = self.ccompare[i].wrapping_sub(self.ccount(now));
        let dist = if dist == 0 {
            1u64 << 32
        } else {
            u64::from(dist)
        };
        self.match_at[i] = now.saturating_add(dist);
        self.next = self.match_at.iter().copied().min().unwrap_or(u64::MAX);
    }

    /// The earliest cycle at which any timer matches, for a machine that
    /// wants to end a slice or an idle skip there. `None` when no timer is
    /// armed.
    #[inline]
    #[must_use]
    pub fn next_match(&self) -> Option<u64> {
        (self.next != u64::MAX).then_some(self.next)
    }

    /// The cycle counter moved to `now`: return the timers whose match cycle
    /// was reached, as a bitmask over `0..NUM_TIMERS`, and re-arm each for
    /// its next wrap.
    #[inline]
    pub fn advance(&mut self, now: u64) -> u8 {
        if now < self.next {
            return 0;
        }
        let mut fired = 0u8;
        for i in 0..NUM_TIMERS {
            if self.match_at[i] <= now {
                fired |= 1 << i;
                // A timer matches once per wrap of CCOUNT.
                self.match_at[i] = self.match_at[i].saturating_add(1u64 << 32);
            }
        }
        self.next = self.match_at.iter().copied().min().unwrap_or(u64::MAX);
        fired
    }
}

impl Default for Timers {
    fn default() -> Self {
        Self::new()
    }
}
