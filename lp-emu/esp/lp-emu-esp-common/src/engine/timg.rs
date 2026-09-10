//! A timer group's counters: an up-count at a configured rate, an alarm that
//! is a scheduler event, auto-reload, the load/latch pair — and the MWDT's
//! write-protect gate.
//!
//! **Behaviour only.** Not one register offset, not one bit position, not one
//! reset value, not one clock rate and not one counter width: the chip's view
//! reads its own registers, computes the numbers in [`CounterConfig`], and
//! hands them in. What comes back out is a verdict the view maps onto
//! whatever bits its own PAC declares.
//!
//! # Why this is an engine
//!
//! On the classic ESP32 there is no SYSTIMER, so TIMG0 is the clock three
//! ways at once: `timer0` is the esp-rtos tick, `timer1` is the 1 ms io pacer
//! and LACT is `Instant::now()`. All three are counters at a configured rate
//! whose alarms are scheduler events raising levels into `IrqLines` — the
//! same scheduled-behaviour-feeding-the-fabric shape the C6 already has, and
//! exactly what a second chip's view would otherwise re-derive and drift on.
//!
//! # N counters, and no more than that
//!
//! The engine holds a `Vec` of counters, each with its own [`TickRate`]. The
//! C6's view builds one, the S3's will build two, and the classic's will
//! build two plus a third whose rate is RTC-derived. Everything that makes
//! the classic's LACT *the LACT* — its own register block, its own divider
//! field, its RTC calibration tie-in — is view-side. There is no `is_lact`
//! flag here and there should never be one.
//!
//! # The rule that keeps two runs identical
//!
//! The rate is an exact rational in guest cycles, because an integer "ticks
//! per cycle" is wrong for every rate anyone actually uses. The count is
//! `elapsed × numer / denom` in a `u128` intermediate and the cycle count for
//! a number of ticks is `ticks × denom` rounded **up** over `numer`, also in
//! `u128`. Both are the chip view's own arithmetic with `divider × cpu_hz`
//! folded into `denom` and the source clock into `numer`: same expressions,
//! same intermediate types, same rounding. A rounding change here is not a
//! wrong byte, it is a different cycle count on every image.

use alloc::vec;
use alloc::vec::Vec;

use lp_emu_core::sched::{Cycles, EventId};

use crate::periph::BusCx;

/// How fast a counter counts, as an exact rational in guest cycles.
///
/// `numer` ticks per `denom` guest cycles. The view computes it: on the C6
/// that is the XTAL rate over `divider × cpu_hz`; on the classic it is the
/// APB rate over the same; on the classic's LACT it is the RTC rate over its
/// own divider. Not one of those numbers belongs in this crate.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct TickRate {
    /// Ticks per `denom` guest cycles.
    pub numer: u64,
    /// Guest cycles per `numer` ticks. A zero here would divide by zero, so
    /// a view that can compute one must not; the C6's divider reading is
    /// never zero (`0` means 65536) and its CPU rate is a constant.
    pub denom: u64,
}

/// What the view tells the engine about one counter before it acts, all of
/// it read out of the chip's own registers.
///
/// Cheap enough to build unconditionally at each call site — which is what
/// the view should do, because every one of these fields can have changed
/// since the last access.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct CounterConfig {
    /// The counter is running. A stopped counter holds its count.
    pub enabled: bool,
    /// The alarm compare is armed. The part clears this itself when the
    /// alarm fires; the **view** owns that bit, so it does the clearing.
    pub alarm_enabled: bool,
    /// On the alarm, reload from `load_value` instead of counting on.
    pub auto_reload: bool,
    pub rate: TickRate,
    /// The counter's width mask.
    pub mask: u64,
    /// The alarm compare, already masked by the view.
    pub alarm: u64,
    /// What a load writes into the count, already masked by the view.
    pub load_value: u64,
}

/// The scheduler ids the view has assigned to one counter's events.
///
/// The engine never packs one itself: it does not know its peripheral index,
/// and a renumbering would change *when* events fire.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct TimgEventIds {
    /// This counter's alarm compare was reached.
    pub alarm: EventId,
}

/// What a write to the watchdog's configuration should do.
///
/// See [`wdt_write`].
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum WdtWrite {
    /// The block is write-protected: drop the write.
    Locked,
    /// Take the write.
    Accepted,
    /// Take the write, and say out loud that the watchdog is now armed —
    /// because **its expiry is not modelled**, so an image that arms one and
    /// then relies on it would otherwise run on in silence.
    ArmedNow,
}

/// The MWDT's write-protect gate, as a pure function of what the view read
/// out of its own registers.
///
/// `unlocked` is the view's comparison of its own write-protect register
/// against its own key (the key is a chip number and stays there, even
/// though all three generations happen to use the same one). `was_enabled`
/// and `now_enabled` are the enable bit before and after this write.
///
/// **Expiry is not modelled anywhere in this engine**, deliberately: the C6
/// view does not model it either, and a watchdog that started firing would
/// be a behaviour change wearing a refactor's clothes. The note on the
/// arming edge is the whole of the model.
pub fn wdt_write(unlocked: bool, was_enabled: bool, now_enabled: bool) -> WdtWrite {
    if !unlocked {
        return WdtWrite::Locked;
    }
    if now_enabled && !was_enabled {
        WdtWrite::ArmedNow
    } else {
        WdtWrite::Accepted
    }
}

/// One counter: an up-count at a configured rate, with an alarm, an
/// auto-reload and a load/latch pair.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
struct Counter {
    /// The count at `base_cycle`.
    base_ticks: u64,
    base_cycle: Cycles,
    /// What a read of the low/high pair returns: the value the last latch
    /// froze, not the live count.
    latched: u64,
}

/// A timer group's counters and its watchdog gate.
#[derive(Clone, Debug)]
pub struct TimgEngine {
    counters: Vec<Counter>,
}

impl TimgEngine {
    /// A group with `counters` counters, all stopped at zero.
    pub fn new(counters: usize) -> Self {
        Self {
            counters: vec![Counter::default(); counters],
        }
    }

    /// How many counters this group has.
    pub fn len(&self) -> usize {
        self.counters.len()
    }

    /// Never, in practice: a group with no counters is not a timer group.
    /// Here because clippy asks for it beside `len`.
    pub fn is_empty(&self) -> bool {
        self.counters.is_empty()
    }

    /// The count at `now`.
    ///
    /// A stopped counter holds the count it was stopped at. A running one is
    /// `base_ticks` plus the rate applied to the cycles elapsed since
    /// `base_cycle`, in a `u128` intermediate, masked to the counter's width.
    pub fn count(&self, i: usize, cfg: &CounterConfig, now: Cycles) -> u64 {
        let c = &self.counters[i];
        if !cfg.enabled {
            return c.base_ticks;
        }
        let delta = u128::from(now.saturating_sub(c.base_cycle));
        let ticks = delta * u128::from(cfg.rate.numer) / u128::from(cfg.rate.denom);
        (c.base_ticks + ticks as u64) & cfg.mask
    }

    /// Freeze what a read of the count returns. The part latches on a pulse
    /// write and the guest then reads the two halves; without the latch a
    /// 64-bit read of a running counter could tear.
    pub fn latch(&mut self, i: usize, cfg: &CounterConfig, now: Cycles) {
        self.counters[i].latched = self.count(i, cfg, now);
    }

    /// The value the last [`latch`](Self::latch) froze.
    pub fn latched(&self, i: usize) -> u64 {
        self.counters[i].latched
    }

    /// Load the count from the configured load value and restart the base
    /// cycle. The view re-arms afterwards.
    pub fn load(&mut self, i: usize, cfg: &CounterConfig, now: Cycles) {
        let c = &mut self.counters[i];
        c.base_ticks = cfg.load_value;
        c.base_cycle = now;
    }

    /// The enable edge, both ways.
    ///
    /// `before` is the configuration **as it stood before the write** — the
    /// old rate included, because freezing means evaluating the count the
    /// old configuration was producing. `enabled` is what the write is
    /// setting it to. No edge, nothing happens.
    pub fn set_enabled(&mut self, i: usize, before: &CounterConfig, enabled: bool, now: Cycles) {
        if before.enabled == enabled {
            return;
        }
        if enabled {
            self.counters[i].base_cycle = now;
        } else {
            // Freeze at the count the old configuration was producing.
            self.counters[i].base_ticks = self.count(i, before, now);
        }
    }

    /// Cancel this counter's pending alarm and schedule the next one.
    ///
    /// A disabled counter, or one whose alarm compare is not armed, gets no
    /// event at all. An alarm already at or behind the count is due **now**,
    /// not in the past: the scheduler would refuse a deadline behind it, and
    /// a driver that writes a compare it has already passed still expects
    /// its interrupt.
    pub fn rearm(&mut self, i: usize, cfg: &CounterConfig, ids: TimgEventIds, cx: &mut BusCx<'_>) {
        cx.sched.cancel(ids.alarm);
        if !cfg.enabled || !cfg.alarm_enabled {
            return;
        }
        let now_ticks = self.count(i, cfg, cx.now);
        if cfg.alarm <= now_ticks {
            cx.sched.schedule_at(cx.now, ids.alarm);
            return;
        }
        let c = &self.counters[i];
        // Cycles for `alarm - base_ticks` ticks, rounded up.
        let ticks = u128::from(cfg.alarm - c.base_ticks);
        let num = ticks * u128::from(cfg.rate.denom);
        let den = u128::from(cfg.rate.numer);
        let cycles = num.div_ceil(den) as u64;
        cx.sched
            .schedule_at(c.base_cycle.saturating_add(cycles), ids.alarm);
    }

    /// The alarm event fired.
    ///
    /// Returns `true` when the alarm really is due — the view then clears its
    /// own alarm-enable bit, sets its own raw-interrupt bit and refreshes its
    /// levels. `false` when the compare moved out since this event was
    /// scheduled, which is the guard that keeps a stale event from firing a
    /// spurious interrupt.
    ///
    /// The auto-reload happens here, because the count is engine state. It is
    /// ordered before the view's register writes rather than between them,
    /// which is unobservable: nothing the view writes on this path is read
    /// back before it returns.
    pub fn on_alarm(&mut self, i: usize, cfg: &CounterConfig, now: Cycles) -> bool {
        if !cfg.enabled || !cfg.alarm_enabled {
            return false;
        }
        if self.count(i, cfg, now) < cfg.alarm {
            // Re-armed for later since this was scheduled.
            return false;
        }
        if cfg.auto_reload {
            let c = &mut self.counters[i];
            c.base_ticks = cfg.load_value;
            c.base_cycle = now;
        }
        true
    }

    /// The engine's serialized state: three words per counter, in order.
    ///
    /// The view calls this at exactly the point in its own stream where it
    /// used to write these fields itself, so the byte format of a chip's
    /// blob does not move when its block becomes a view.
    pub fn save(&self, out: &mut Vec<u8>) {
        for c in &self.counters {
            out.extend_from_slice(&c.base_ticks.to_le_bytes());
            out.extend_from_slice(&c.base_cycle.to_le_bytes());
            out.extend_from_slice(&c.latched.to_le_bytes());
        }
    }

    /// Restore from [`save`](Self::save), returning how many bytes were
    /// consumed. `None` — and **nothing applied** — when the blob is short,
    /// so that a truncated blob leaves the engine at its reset state rather
    /// than half-loaded.
    pub fn load_state(&mut self, bytes: &[u8]) -> Option<usize> {
        let mut r = Cursor(bytes);
        let mut parsed = Vec::with_capacity(self.counters.len());
        for _ in 0..self.counters.len() {
            parsed.push(Counter {
                base_ticks: r.u64()?,
                base_cycle: r.u64()?,
                latched: r.u64()?,
            });
        }
        let used = bytes.len() - r.0.len();
        self.counters = parsed;
        Some(used)
    }
}

struct Cursor<'a>(&'a [u8]);

impl Cursor<'_> {
    fn u64(&mut self) -> Option<u64> {
        let (head, rest) = self.0.split_first_chunk::<8>()?;
        self.0 = rest;
        Some(u64::from_le_bytes(*head))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::periph::Sandbox;

    /// A counter width no part on the roadmap uses, so a test can never
    /// pass by agreeing with some chip's real one.
    const MASK: u64 = (1 << 48) - 1;

    /// A rate no chip supplies, so a test can never pass by accident because
    /// it happened to agree with the C6's numbers.
    fn cfg(numer: u64, denom: u64) -> CounterConfig {
        CounterConfig {
            enabled: true,
            alarm_enabled: false,
            auto_reload: false,
            rate: TickRate { numer, denom },
            mask: MASK,
            alarm: 0,
            load_value: 0,
        }
    }

    fn ids(n: u32) -> TimgEventIds {
        TimgEventIds { alarm: EventId(n) }
    }

    #[test]
    fn the_count_is_the_rate_applied_to_elapsed_cycles() {
        let e = TimgEngine::new(1);
        let c = cfg(3, 7);
        // 3 ticks per 7 cycles, truncating, from a base of zero at cycle 0.
        assert_eq!(e.count(0, &c, 0), 0);
        assert_eq!(e.count(0, &c, 1), 0); // 3/7
        assert_eq!(e.count(0, &c, 2), 0); // 6/7
        assert_eq!(e.count(0, &c, 3), 1); // 9/7
        assert_eq!(e.count(0, &c, 7), 3);
        assert_eq!(e.count(0, &c, 70), 30);
        assert_eq!(e.count(0, &c, 1_000), 428); // 3000/7
        // Stopped: the count is held, whatever the cycle.
        let stopped = CounterConfig {
            enabled: false,
            ..c
        };
        assert_eq!(e.count(0, &stopped, 1_000), 0);
        // The width mask is the view's, and it is applied: a narrow counter
        // wraps rather than growing past its top.
        let narrow = CounterConfig { mask: 0xff, ..c };
        assert_eq!(e.count(0, &narrow, 700), 300 & 0xff);
    }

    #[test]
    fn disabling_freezes_and_enabling_restarts_the_base() {
        let mut e = TimgEngine::new(1);
        let running = cfg(3, 7);
        let stopped = CounterConfig {
            enabled: false,
            ..running
        };
        // Run to cycle 70: 30 ticks.
        assert_eq!(e.count(0, &running, 70), 30);
        // The write that clears enable is evaluated against the OLD config.
        e.set_enabled(0, &running, false, 70);
        assert_eq!(e.count(0, &stopped, 70), 30);
        assert_eq!(e.count(0, &stopped, 7_000), 30, "frozen, not counting");
        // Re-enable at 7_000: the base cycle restarts, the count resumes.
        e.set_enabled(0, &stopped, true, 7_000);
        assert_eq!(e.count(0, &running, 7_000), 30);
        assert_eq!(e.count(0, &running, 7_070), 60);
        // No edge, no effect.
        e.set_enabled(0, &running, true, 9_999);
        assert_eq!(e.count(0, &running, 7_070), 60);
    }

    #[test]
    fn the_latch_is_what_a_read_returns_not_the_live_count() {
        let mut e = TimgEngine::new(1);
        let c = cfg(3, 7);
        assert_eq!(e.latched(0), 0);
        e.latch(0, &c, 70);
        assert_eq!(e.latched(0), 30);
        assert_eq!(e.count(0, &c, 700), 300, "the count moved on");
        assert_eq!(e.latched(0), 30, "the latch did not");
        e.latch(0, &c, 700);
        assert_eq!(e.latched(0), 300);
    }

    #[test]
    fn an_alarm_already_past_is_scheduled_at_now() {
        let mut sb = Sandbox::new();
        let mut e = TimgEngine::new(1);
        let c = CounterConfig {
            alarm_enabled: true,
            alarm: 10,
            ..cfg(3, 7)
        };
        // At cycle 700 the count is 300, well past the compare of 10.
        sb.now = 700;
        e.rearm(0, &c, ids(1), &mut sb.cx());
        assert_eq!(
            sb.sched.next_deadline(),
            Some(700),
            "due now, not in the past"
        );
        // And a compare still ahead schedules at the cycle it is reached.
        let ahead = CounterConfig { alarm: 301, ..c };
        e.rearm(0, &ahead, ids(1), &mut sb.cx());
        // 301 ticks at 3/7 = 702.33 cycles, rounded up.
        assert_eq!(sb.sched.next_deadline(), Some(703));
        // Disarmed: no event at all.
        let disarmed = CounterConfig {
            alarm_enabled: false,
            ..ahead
        };
        e.rearm(0, &disarmed, ids(1), &mut sb.cx());
        assert_eq!(sb.sched.next_deadline(), None);
    }

    #[test]
    fn auto_reload_reloads_from_the_load_value() {
        let mut e = TimgEngine::new(1);
        let plain = CounterConfig {
            alarm_enabled: true,
            alarm: 30,
            load_value: 5,
            ..cfg(3, 7)
        };
        // Without auto-reload the counter keeps counting past the alarm.
        assert!(e.on_alarm(0, &plain, 70));
        assert_eq!(e.count(0, &plain, 70), 30);
        assert_eq!(e.count(0, &plain, 140), 60, "it counted on");

        let mut e = TimgEngine::new(1);
        let reload = CounterConfig {
            auto_reload: true,
            ..plain
        };
        assert!(e.on_alarm(0, &reload, 70));
        assert_eq!(e.count(0, &reload, 70), 5, "reloaded");
        assert_eq!(e.count(0, &reload, 140), 35, "and counting from there");
    }

    #[test]
    fn a_rearm_for_later_makes_the_pending_alarm_a_no_op() {
        let mut sb = Sandbox::new();
        let mut e = TimgEngine::new(1);
        let near = CounterConfig {
            alarm_enabled: true,
            alarm: 30,
            ..cfg(3, 7)
        };
        sb.now = 0;
        e.rearm(0, &near, ids(1), &mut sb.cx());
        assert_eq!(sb.sched.next_deadline(), Some(70));
        // The guest moves the compare out before the event is dispatched.
        let far = CounterConfig { alarm: 300, ..near };
        assert!(
            !e.on_alarm(0, &far, 70),
            "the stale event must not fire an interrupt"
        );
        // At the new compare it is due.
        assert!(e.on_alarm(0, &far, 700));
        // A disabled counter never fires either.
        let off = CounterConfig {
            enabled: false,
            ..far
        };
        assert!(!e.on_alarm(0, &off, 700));
    }

    #[test]
    fn two_counters_are_independent() {
        let mut sb = Sandbox::new();
        let mut e = TimgEngine::new(2);
        // The LACT-room test: two counters at unrelated rates, alarms
        // interleaved, neither disturbing the other.
        let fast = CounterConfig {
            alarm_enabled: true,
            alarm: 30,
            ..cfg(3, 7)
        };
        let slow = CounterConfig {
            alarm_enabled: true,
            alarm: 30,
            ..cfg(1, 11)
        };
        sb.now = 0;
        e.rearm(0, &fast, ids(1), &mut sb.cx());
        e.rearm(1, &slow, ids(2), &mut sb.cx());
        assert_eq!(sb.sched.next_deadline(), Some(70), "the fast one is first");

        // Counter 0's alarm is due at 70; counter 1's is not.
        assert!(e.on_alarm(0, &fast, 70));
        assert!(!e.on_alarm(1, &slow, 70));
        assert_eq!(e.count(0, &fast, 70), 30);
        assert_eq!(e.count(1, &slow, 70), 6);

        // Reloading and latching counter 0 leaves counter 1 alone.
        e.load(
            0,
            &CounterConfig {
                load_value: 0,
                ..fast
            },
            70,
        );
        e.latch(0, &fast, 70);
        assert_eq!(e.latched(0), 0);
        assert_eq!(e.latched(1), 0);
        assert_eq!(e.count(1, &slow, 330), 30, "counter 1 kept its own base");
        assert!(e.on_alarm(1, &slow, 330));
        assert_eq!(e.count(0, &fast, 330), 111, "counter 0 counts from 70");
    }

    #[test]
    fn the_watchdog_gate_drops_locked_writes_and_notes_the_arming_edge() {
        assert_eq!(wdt_write(false, false, true), WdtWrite::Locked);
        assert_eq!(wdt_write(true, false, true), WdtWrite::ArmedNow);
        assert_eq!(wdt_write(true, true, true), WdtWrite::Accepted);
        assert_eq!(wdt_write(true, true, false), WdtWrite::Accepted);
        assert_eq!(wdt_write(true, false, false), WdtWrite::Accepted);
    }

    #[test]
    fn the_engine_state_round_trips() {
        let mut e = TimgEngine::new(2);
        let a = cfg(3, 7);
        let b = cfg(1, 11);
        e.set_enabled(
            0,
            &CounterConfig {
                enabled: false,
                ..a
            },
            true,
            100,
        );
        e.latch(0, &a, 800);
        e.load(
            1,
            &CounterConfig {
                load_value: 12_345,
                ..b
            },
            500,
        );
        let mut blob = Vec::new();
        e.save(&mut blob);
        assert_eq!(blob.len(), 2 * 3 * 8, "three words per counter");

        let mut other = TimgEngine::new(2);
        assert_eq!(other.load_state(&blob), Some(blob.len()));
        assert_eq!(other.count(0, &a, 800), e.count(0, &a, 800));
        assert_eq!(other.latched(0), e.latched(0));
        assert_eq!(other.count(1, &b, 5_000), e.count(1, &b, 5_000));

        // A short blob applies nothing.
        let mut third = TimgEngine::new(2);
        assert_eq!(third.load_state(&blob[..blob.len() - 1]), None);
        let fresh = TimgEngine::new(2);
        assert_eq!(third.count(0, &a, 800), fresh.count(0, &a, 800));
        assert_eq!(third.latched(0), 0, "untouched, not half-loaded");
        // And a blob with more behind it says how much it took.
        let mut tail = blob.clone();
        tail.extend_from_slice(&[0xaa; 4]);
        let mut fourth = TimgEngine::new(2);
        assert_eq!(fourth.load_state(&tail), Some(blob.len()));
    }
}
