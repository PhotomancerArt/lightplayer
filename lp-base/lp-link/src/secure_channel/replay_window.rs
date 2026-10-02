//! Which sealed-frame counters a receiver still accepts.
//!
//! - **Window** (ARQ links): RFC 6479's sliding bitmap, 64 frames wide. A
//!   counter above the highest seen moves the window; one inside it is taken
//!   once; one below it is too old. A link has at most `tx_window` (≤ 16)
//!   frames in flight, so 64 holds every honest reorder, and a frame that falls
//!   out anyway is dropped and resent by ARQ.
//! - **Strict** (no-ARQ links, a WebSocket or the relay): the next counter
//!   only. Nothing resends on such a link, so a gap is lost data (or a relay
//!   dropping frames), and the link resets rather than deliver around it.
//!
//! [`check`](ReplayWindow::check) only looks; [`mark`](ReplayWindow::mark)
//! records, and is called only after the frame's tag verified, so a forged
//! counter can never move the window.

/// Bits in the window.
pub const WINDOW: u32 = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplayVerdict {
    /// Never seen: open it.
    Fresh,
    /// Seen already (inside the window, or below `next` in strict mode).
    Replay,
    /// Below the window: cannot be told from a replay.
    TooOld,
    /// Strict mode: ahead of the next counter (frames went missing).
    Gap,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReplayWindow {
    Window {
        /// The highest counter marked, once one has been.
        highest: Option<u32>,
        /// Bit `i`: `highest - i` was marked.
        bits: u64,
    },
    Strict {
        next: u32,
    },
}

impl ReplayWindow {
    pub fn window() -> Self {
        ReplayWindow::Window {
            highest: None,
            bits: 0,
        }
    }

    pub fn strict() -> Self {
        ReplayWindow::Strict { next: 0 }
    }

    pub fn check(&self, ctr: u32) -> ReplayVerdict {
        match *self {
            ReplayWindow::Window { highest, bits } => {
                let Some(top) = highest else {
                    return ReplayVerdict::Fresh;
                };
                if ctr > top {
                    return ReplayVerdict::Fresh;
                }
                let back = top - ctr;
                if back >= WINDOW {
                    ReplayVerdict::TooOld
                } else if bits & (1u64 << back) != 0 {
                    ReplayVerdict::Replay
                } else {
                    ReplayVerdict::Fresh
                }
            }
            ReplayWindow::Strict { next } => match ctr.cmp(&next) {
                core::cmp::Ordering::Equal => ReplayVerdict::Fresh,
                core::cmp::Ordering::Less => ReplayVerdict::Replay,
                core::cmp::Ordering::Greater => ReplayVerdict::Gap,
            },
        }
    }

    /// Record `ctr` (after its frame opened). Only call it for a `Fresh` one.
    pub fn mark(&mut self, ctr: u32) {
        match self {
            ReplayWindow::Window { highest, bits } => match *highest {
                Some(top) if ctr <= top => {
                    let back = top - ctr;
                    if back < WINDOW {
                        *bits |= 1u64 << back;
                    }
                }
                Some(top) => {
                    let shift = ctr - top;
                    *bits = if shift >= WINDOW { 0 } else { *bits << shift };
                    *bits |= 1;
                    *highest = Some(ctr);
                }
                None => {
                    *bits = 1;
                    *highest = Some(ctr);
                }
            },
            ReplayWindow::Strict { next } => *next = ctr.wrapping_add(1),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ReplayVerdict::*;

    fn take(w: &mut ReplayWindow, ctr: u32) -> ReplayVerdict {
        let v = w.check(ctr);
        if v == Fresh {
            w.mark(ctr);
        }
        v
    }

    #[test]
    fn in_order_is_fresh_once() {
        let mut w = ReplayWindow::window();
        for c in 0..200 {
            assert_eq!(take(&mut w, c), Fresh);
            assert_eq!(take(&mut w, c), Replay);
        }
    }

    #[test]
    fn reorder_inside_the_window_is_taken() {
        let mut w = ReplayWindow::window();
        assert_eq!(take(&mut w, 10), Fresh);
        assert_eq!(take(&mut w, 70), Fresh);
        // 70 - 63 = 7 is the oldest still inside.
        assert_eq!(take(&mut w, 7), Fresh);
        assert_eq!(take(&mut w, 6), TooOld);
        assert_eq!(take(&mut w, 10), Replay);
        assert_eq!(take(&mut w, 69), Fresh);
        assert_eq!(take(&mut w, 69), Replay);
        // A jump past the whole window forgets it.
        assert_eq!(take(&mut w, 1_000), Fresh);
        assert_eq!(take(&mut w, 999), Fresh);
        assert_eq!(take(&mut w, 70), TooOld);
    }

    #[test]
    fn a_check_without_a_mark_moves_nothing() {
        let mut w = ReplayWindow::window();
        take(&mut w, 5);
        assert_eq!(w.check(500), Fresh);
        assert_eq!(w.check(4), Fresh);
        assert_eq!(take(&mut w, 4), Fresh);
    }

    #[test]
    fn strict_takes_only_the_next() {
        let mut w = ReplayWindow::strict();
        assert_eq!(take(&mut w, 1), Gap);
        assert_eq!(take(&mut w, 0), Fresh);
        assert_eq!(take(&mut w, 0), Replay);
        assert_eq!(take(&mut w, 1), Fresh);
        assert_eq!(take(&mut w, 3), Gap);
        assert_eq!(take(&mut w, 2), Fresh);
    }

    #[test]
    fn the_top_of_the_counter_space() {
        let mut w = ReplayWindow::window();
        assert_eq!(take(&mut w, u32::MAX - 1), Fresh);
        assert_eq!(take(&mut w, u32::MAX), Fresh);
        assert_eq!(take(&mut w, u32::MAX), Replay);
        assert_eq!(take(&mut w, 0), TooOld);
    }
}
