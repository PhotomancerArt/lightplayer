//! When a filesystem may spend a tick on its own cleanup (`LpFs::idle_tick`;
//! the tree store's idle steps): the radio frame-rate budget's ceilings
//! (`docs/adr/2026-10-06-radio-frame-rate-budget.md`) applied to cleanup,
//! from the server's injected tick times alone (no clock is read here).
//!
//! - **Quiet first.** No piece until no request has changed a file (and no
//!   batch has been open) for [`IdlePacer::QUIET_MS`]; a request starts the
//!   quiet over, and the delta after it (its own handling time) is not
//!   quiet time. Reads do not end the quiet: Studio reads a board all the
//!   time it is open.
//! - **Nothing playing: every quiet tick.** No frame is waiting; a piece
//!   delays a read by its own few tens of ms.
//! - **A show playing: in its spare time.** A piece runs after a frame only
//!   when the frame it lengthens stays under the ceiling
//!   ([`IdlePacer::FRAME_CEILING_MS`], less a margin for the frames' own
//!   jitter) — by what the pacer has learned of the show's frame and of a
//!   piece — and only while cleanup has spent no more than
//!   [`IdlePacer::DUTY_PERCENT`] of the show's time (the median-fps
//!   ceiling: ≤ 10 %). A show too slow for any piece to fit still gets one
//!   every [`IdlePacer::OVER_EVERY_FRAMES`] frames: the frames it makes slow
//!   stay a small part of the 1 % the p99 bound allows.
//!
//! What it learns, from the deltas alone: a **frame** is the delta after a
//! tick that ran no piece (and handled no file-changing request), averaged;
//! a **piece** costs the delta after its tick less that frame, held as a
//! slowly decaying maximum (a piece is a mark, an erase, a forget or a few
//! copies: the slowest kind sets it).

use crate::IdleTick;

/// The idle work's pace (one per filesystem).
#[derive(Clone, Debug)]
pub struct IdlePacer {
    quiet_ms: u32,
    /// The tick that is ending handled a file-changing request: the next
    /// delta is that request's time.
    last_busy: bool,
    /// The tick that is ending ran a piece: the next delta holds its cost.
    ran: bool,
    /// The show's frame (ms × 8, averaged over eight).
    frame_x8: u32,
    /// A piece's cost (ms): the slowest lately.
    piece_ms: u32,
    /// What cleanup may still spend (ms × 100): earned at the duty while a
    /// show plays, spent by each piece's cost.
    credit: i32,
    /// Frames since a piece ran with no room under the ceiling.
    since_over: u32,
    /// What the pacer did lately, logged once a window while it works.
    report: IdleReport,
}

/// What the pacer did over a window of a playing show (one `[fs idle]`
/// log line, only for a window in which a piece ran).
#[derive(Clone, Copy, Debug)]
struct IdleReport {
    show_ms: u32,
    pieces: u32,
    piece_ms: u32,
    worst_ms: u32,
    /// Ticks a piece waited: no room under the ceiling, or no duty left.
    held_room: u32,
    held_duty: u32,
}

impl IdleReport {
    const NONE: Self = Self {
        show_ms: 0,
        pieces: 0,
        piece_ms: 0,
        worst_ms: 0,
        held_room: 0,
        held_duty: 0,
    };
}

impl Default for IdlePacer {
    fn default() -> Self {
        Self::new()
    }
}

impl IdlePacer {
    /// No file-changing request for this long (handling time excluded)
    /// makes a tick quiet.
    pub const QUIET_MS: u32 = 2_000;
    /// The share of a playing show's time cleanup may take.
    pub const DUTY_PERCENT: u32 = 8;
    /// The frame a piece must not push a show past (the p99 bound).
    pub const FRAME_CEILING_MS: u32 = 100;
    /// Room kept under the ceiling for the frames' own jitter.
    pub const SLACK_MARGIN_MS: u32 = 15;
    /// A show with no room under the ceiling gets a piece at most once in
    /// this many frames (0.5 %).
    pub const OVER_EVERY_FRAMES: u32 = 200;
    /// The most credit a long quiet saves up (ms): no burst of pieces.
    pub const CREDIT_CAP_MS: i32 = 50;
    /// A piece's cost before one has been seen (ms).
    pub const PIECE_GUESS_MS: u32 = 30;
    /// A report line covers this much of a playing show (ms).
    pub const REPORT_EVERY_MS: u32 = 5_000;

    pub const fn new() -> Self {
        Self {
            quiet_ms: 0,
            last_busy: false,
            ran: false,
            frame_x8: 0,
            piece_ms: Self::PIECE_GUESS_MS,
            credit: 0,
            since_over: 0,
            report: IdleReport::NONE,
        }
    }

    /// One tick, after its frame: `true` when the filesystem should do one
    /// piece of cleanup now (then report it with [`Self::ran`]).
    pub fn tick(&mut self, t: IdleTick) -> bool {
        let after_busy = core::mem::replace(&mut self.last_busy, t.busy);
        let ran = core::mem::take(&mut self.ran);
        let delta = t.delta_ms.min(60_000);
        if !after_busy {
            let frame = self.frame_ms();
            if ran {
                let cost = delta.saturating_sub(frame);
                self.piece_ms = cost.max(self.piece_ms - self.piece_ms / 8);
                self.credit = self.credit.saturating_sub(cost as i32 * 100);
                let r = &mut self.report;
                r.pieces += 1;
                r.piece_ms = r.piece_ms.saturating_add(cost);
                r.worst_ms = r.worst_ms.max(cost);
            } else if self.frame_x8 == 0 {
                self.frame_x8 = delta * 8;
            } else {
                self.frame_x8 = self.frame_x8 - self.frame_x8 / 8 + delta;
            }
            self.quiet_ms = self.quiet_ms.saturating_add(delta);
        }
        if t.busy {
            self.quiet_ms = 0;
            self.credit = 0;
            return false;
        }
        if self.quiet_ms < Self::QUIET_MS {
            return false;
        }
        if !t.playing {
            return true;
        }
        self.report_window(delta);
        self.credit =
            (self.credit + (delta * Self::DUTY_PERCENT) as i32).min(Self::CREDIT_CAP_MS * 100);
        self.since_over = self.since_over.saturating_add(1);
        if self.credit < 0 {
            self.report.held_duty += 1;
            return false;
        }
        if self.frame_ms() + self.piece_ms > Self::FRAME_CEILING_MS - Self::SLACK_MARGIN_MS {
            if self.since_over < Self::OVER_EVERY_FRAMES {
                self.report.held_room += 1;
                return false;
            }
            self.since_over = 0;
        }
        true
    }

    /// The piece [`Self::tick`] asked for: `did` = it did work (the next
    /// tick's delta holds its cost). `false` (nothing to clean) costs
    /// nothing.
    pub fn ran(&mut self, did: bool) {
        self.ran = did;
    }

    /// Count `delta` of a playing show; every [`Self::REPORT_EVERY_MS`],
    /// one log line if a piece ran in the window.
    fn report_window(&mut self, delta: u32) {
        let r = &mut self.report;
        r.show_ms = r.show_ms.saturating_add(delta);
        if r.show_ms < Self::REPORT_EVERY_MS {
            return;
        }
        if r.pieces > 0 {
            log::info!(
                "[fs idle] {} ms: {} pieces, {} ms, worst {} ms; frame {} ms, piece {} ms; \
                 held: room {}, duty {}",
                r.show_ms,
                r.pieces,
                r.piece_ms,
                r.worst_ms,
                self.frame_x8 / 8,
                self.piece_ms,
                r.held_room,
                r.held_duty
            );
        }
        self.report = IdleReport::NONE;
    }

    /// The show's frame as learned (ms).
    pub fn frame_ms(&self) -> u32 {
        self.frame_x8 / 8
    }

    /// A piece's cost as learned (ms).
    pub fn piece_ms(&self) -> u32 {
        self.piece_ms
    }
}

#[cfg(test)]
mod tests {
    extern crate alloc;

    use alloc::vec::Vec;

    use super::*;

    fn quiet(delta_ms: u32, playing: bool) -> IdleTick {
        IdleTick {
            delta_ms,
            busy: false,
            playing,
        }
    }

    #[test]
    fn nothing_playing_runs_a_piece_every_quiet_tick_after_two_seconds() {
        let mut p = IdlePacer::new();
        let mut at = Vec::new();
        for t in 0..100u32 {
            if p.tick(quiet(25, false)) {
                p.ran(true);
                at.push((t + 1) * 25);
            }
        }
        assert_eq!(&at[..4], &[2000, 2025, 2050, 2075]);
    }

    #[test]
    fn a_request_starts_the_quiet_over_and_its_time_is_not_quiet() {
        let mut p = IdlePacer::new();
        for _ in 0..79 {
            assert!(!p.tick(quiet(25, false)));
        }
        assert!(!p.tick(IdleTick {
            delta_ms: 25,
            busy: true,
            playing: false
        }));
        // The delta after the busy tick is the request's own time.
        assert!(!p.tick(quiet(5_000, false)));
        let mut n = 0;
        while !p.tick(quiet(25, false)) {
            n += 1;
        }
        assert_eq!(n, 79);
    }

    /// A 44 ms show (CX1's c40) and 20 ms pieces: no frame a piece
    /// lengthens reaches the margin, and cleanup takes about the duty.
    #[test]
    fn a_show_with_room_gets_pieces_at_the_duty_and_under_the_ceiling() {
        let r = show(44, 20, 10_000);
        assert!(r.worst <= 64, "{r:?}");
        assert!(r.duty_percent() <= 8.5 && r.duty_percent() >= 6.0, "{r:?}");
    }

    /// A piece too slow for the show's room (60 ms beside 44): once learned,
    /// pieces run once every `OVER_EVERY_FRAMES` frames.
    #[test]
    fn pieces_that_do_not_fit_run_once_in_two_hundred_frames() {
        let r = show(44, 60, 20_000);
        assert!(r.over <= 20_000 / 200 + 2, "{r:?}");
        assert!(r.pieces >= 20_000 / 200 - 2, "{r:?}");
    }

    /// A show already near the ceiling (90 ms): at most one frame in 200
    /// pushed over it.
    #[test]
    fn a_slow_show_loses_at_most_half_a_percent_of_frames() {
        let r = show(90, 15, 20_000);
        assert!(
            (r.over as f64) <= 20_000.0 * 0.005 + 1.0,
            "{r:?}: {} over",
            r.over
        );
    }

    #[derive(Debug)]
    struct Show {
        pieces: u32,
        over: u32,
        worst: u32,
        piece_time: u64,
        time: u64,
    }

    impl Show {
        fn duty_percent(&self) -> f64 {
            self.piece_time as f64 * 100.0 / self.time as f64
        }
    }

    /// `frames` quiet frames of `frame_ms`, each followed by a piece of
    /// `piece_ms` when the pacer asks for one.
    fn show(frame_ms: u32, piece_ms: u32, frames: u32) -> Show {
        let mut p = IdlePacer::new();
        let mut r = Show {
            pieces: 0,
            over: 0,
            worst: 0,
            piece_time: 0,
            time: 0,
        };
        let mut delta = frame_ms;
        for _ in 0..frames {
            let run = p.tick(quiet(delta, true));
            delta = frame_ms;
            if run {
                p.ran(true);
                delta += piece_ms;
                r.pieces += 1;
                r.piece_time += u64::from(piece_ms);
            }
            r.time += u64::from(delta);
            r.worst = r.worst.max(delta);
            if delta > IdlePacer::FRAME_CEILING_MS {
                r.over += 1;
            }
        }
        r
    }
}
