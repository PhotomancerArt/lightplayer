//! `SYSTIMER` at `0x6000_A000` — the 52-bit system timer, modelled.
//!
//! Register facts from `m3/discovery-esp-hal-timers-uart-usb-pac.md` §1
//! (offsets in `regs::SYSTIMER`):
//!
//! - Two counter units at **XTAL / 2.5 = 16 MHz** (`systimer.rs:186-208`,
//!   `ticks_per_second`). At 160 MHz that is one tick per **10 cycles**;
//!   under `t1` cycles are instructions, which is still deterministic.
//!   `Instant::now` uses **Unit0** (`time.rs:769-782`).
//! - `unit_op.update` (bit 30) latches the count; `value_valid` (bit 29)
//!   reads 1 afterwards; `unit_value.{hi,lo}` return the latched value. The
//!   read is `lo, hi, lo` until the two `lo`s agree — trivially true here.
//! - Three comparators. `trgt.{hi,lo}` and `target_conf.period` are
//!   **staged** and committed by `comp_load` (bit 0); `target_conf.
//!   {period_mode, timer_unit_sel}` take effect on write (esp-hal's
//!   `load_value` writes the period, commits, then flips `period_mode`
//!   off and on — `systimer.rs:657-694`). `conf.targetN_work_en` arms the
//!   comparator; an armed one fires at the cycle where its unit reaches
//!   the committed target, and in period mode re-arms at `+period`.
//! - On fire: `int_raw.targetN`; the source `SYSTIMER_TARGET0+N`
//!   (57..=59) is the level `int_raw & int_ena`. `int_clr` is
//!   write-1-to-clear, `int_st = int_raw & int_ena`.
//! - `conf` resets to `0x4600_0000`: Unit0 running, Unit1 stopped
//!   (`systimer/conf.rs`, `RESET_VALUE`).
//!
//! **A target at or below the current count fires immediately** (modeled;
//! esp-hal always sets `now + ticks`, so the case is never reached by the
//! HAL, and "never fires, wraps in nine years" would be the worse
//! surprise).
//!
//! The shipped firmware's tick is **not** here — `esp_rtos::start` is
//! handed `timg0.timer0` (`board/esp32c6/init.rs:76`), so the tick is
//! [`super::timg`]. This block serves `Instant::now`, `Delay`, and
//! `esp_rtos`'s `now()`.

use lp_emu_core::sched::EventId;
use lp_emu_esp_common::regfile::{lane_of, merge_lane};
use lp_emu_esp_common::{BusCx, Peripheral, Width, event_id};

use crate::memmap;
use crate::regs::{self, source};

/// XTAL / 2.5.
pub const TICKS_PER_SECOND: u64 = 16_000_000;
/// One tick per ten CPU cycles at 160 MHz.
pub const CYCLES_PER_TICK: u64 = memmap::CPU_HZ / TICKS_PER_SECOND;

const MASK52: u64 = (1 << 52) - 1;
const MASK20: u32 = (1 << 20) - 1;
const PERIOD_MASK: u32 = 0x3FF_FFFF;

const CONF_RESET: u32 = 0x4600_0000;
const UNIT_WORK_EN: [u32; 2] = [1 << 30, 1 << 29];
const TARGET_WORK_EN: [u32; 3] = [1 << 24, 1 << 23, 1 << 22];
const OP_VALUE_VALID: u32 = 1 << 29;
const OP_UPDATE: u32 = 1 << 30;
const TC_PERIOD_MODE: u32 = 1 << 30;
const TC_UNIT_SEL: u32 = 1 << 31;

// Offsets (`regs::SYSTIMER`).
const CONF: u32 = 0x00;
const UNIT0_OP: u32 = 0x04;
const UNIT1_OP: u32 = 0x08;
const UNIT0_LOAD_HI: u32 = 0x0c;
const UNIT1_LOAD_LO: u32 = 0x18;
const TRGT0_HI: u32 = 0x1c;
const TRGT2_LO: u32 = 0x30;
const TARGET0_CONF: u32 = 0x34;
const TARGET2_CONF: u32 = 0x3c;
const UNIT0_VALUE_HI: u32 = 0x40;
const UNIT1_VALUE_LO: u32 = 0x4c;
const COMP0_LOAD: u32 = 0x50;
const COMP2_LOAD: u32 = 0x58;
const UNIT0_LOAD: u32 = 0x5c;
const UNIT1_LOAD: u32 = 0x60;
const INT_ENA: u32 = 0x64;
const INT_RAW: u32 = 0x68;
const INT_CLR: u32 = 0x6c;
const INT_ST: u32 = 0x70;
const REAL_TARGET0_LO: u32 = 0x74;
const REAL_TARGET2_HI: u32 = 0x88;
const DATE: u32 = 0xfc;

/// The system timer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Systimer {
    index: usize,
    conf: u32,
    /// `count = (cycles / 10 + offset) & MASK52` while the unit runs.
    offset: [u64; 2],
    /// The count a stopped unit holds.
    frozen: [Option<u64>; 2],
    latched: [u64; 2],
    /// `unitNload.{hi,lo}`, staged until `unit_load`.
    load_staged: [u64; 2],
    /// `trgtN.{hi,lo}`, staged until `compN_load`.
    target_staged: [u64; 3],
    target_conf: [u32; 3],
    /// Committed by `comp_load`: the value the comparator is armed against.
    real_target: [u64; 3],
    period: [u32; 3],
    int_ena: u32,
    int_raw: u32,
    date: u32,
}

impl Default for Systimer {
    fn default() -> Self {
        Self::new()
    }
}

impl Systimer {
    pub fn new() -> Self {
        Self {
            index: 0,
            conf: CONF_RESET,
            offset: [0; 2],
            // Unit1's `work_en` is clear at reset: it holds 0 until started.
            frozen: [
                (CONF_RESET & UNIT_WORK_EN[0] == 0).then_some(0),
                (CONF_RESET & UNIT_WORK_EN[1] == 0).then_some(0),
            ],
            latched: [0; 2],
            load_staged: [0; 2],
            target_staged: [0; 3],
            target_conf: [0; 3],
            real_target: [0; 3],
            period: [0; 3],
            int_ena: 0,
            int_raw: 0,
            date: 0,
        }
    }

    /// Unit `u`'s count at `now`.
    pub fn count(&self, u: usize, now: u64) -> u64 {
        match self.frozen[u] {
            Some(v) => v,
            None => (now / CYCLES_PER_TICK).wrapping_add(self.offset[u]) & MASK52,
        }
    }

    /// The committed target of comparator `n`.
    pub fn real_target(&self, n: usize) -> u64 {
        self.real_target[n]
    }

    fn unit_of(&self, n: usize) -> usize {
        usize::from(self.target_conf[n] & TC_UNIT_SEL != 0)
    }

    fn period_mode(&self, n: usize) -> bool {
        self.target_conf[n] & TC_PERIOD_MODE != 0
    }

    fn event(&self, n: usize) -> EventId {
        event_id(self.index, n as u16)
    }

    fn update_lines(&self, cx: &mut BusCx<'_>) {
        let st = self.int_raw & self.int_ena;
        for n in 0..3u16 {
            cx.irq
                .set_level(source::SYSTIMER_TARGET0 + n, st & (1 << n) != 0);
        }
    }

    /// Cancel comparator `n`'s pending fire and, if it is armed, schedule
    /// the cycle its unit reaches the committed target.
    fn rearm(&mut self, n: usize, cx: &mut BusCx<'_>) {
        cx.sched.cancel(self.event(n));
        if self.conf & TARGET_WORK_EN[n] == 0 {
            return;
        }
        let u = self.unit_of(n);
        if self.frozen[u].is_some() {
            // A stopped unit never reaches anything.
            return;
        }
        // First cycle `c` with `c / 10 + offset >= target`.
        let need = self.real_target[n].wrapping_sub(self.offset[u]) & MASK52;
        let at = need.saturating_mul(CYCLES_PER_TICK);
        cx.sched.schedule_at(at.max(cx.now), self.event(n));
    }

    fn write_conf(&mut self, value: u32, cx: &mut BusCx<'_>) {
        let old = self.conf;
        self.conf = value;
        for u in 0..2 {
            let was = old & UNIT_WORK_EN[u] != 0;
            let is = value & UNIT_WORK_EN[u] != 0;
            if was && !is {
                self.frozen[u] = Some(self.count(u, cx.now));
            } else if !was && is {
                let held = self.frozen[u].take().unwrap_or(0);
                self.offset[u] = held.wrapping_sub(cx.now / CYCLES_PER_TICK) & MASK52;
            }
        }
        for n in 0..3 {
            if (old ^ value) & TARGET_WORK_EN[n] != 0 {
                self.rearm(n, cx);
            }
        }
    }

    fn read_word(&self, off: u32, now: u64) -> u32 {
        match off {
            CONF => self.conf,
            UNIT0_OP | UNIT1_OP => OP_VALUE_VALID,
            UNIT0_LOAD_HI..=UNIT1_LOAD_LO => {
                let i = (off - UNIT0_LOAD_HI) / 4;
                let u = (i / 2) as usize;
                if i % 2 == 0 {
                    (self.load_staged[u] >> 32) as u32 & MASK20
                } else {
                    self.load_staged[u] as u32
                }
            }
            TRGT0_HI..=TRGT2_LO => {
                let i = (off - TRGT0_HI) / 4;
                let n = (i / 2) as usize;
                if i % 2 == 0 {
                    (self.target_staged[n] >> 32) as u32 & MASK20
                } else {
                    self.target_staged[n] as u32
                }
            }
            TARGET0_CONF..=TARGET2_CONF => self.target_conf[((off - TARGET0_CONF) / 4) as usize],
            UNIT0_VALUE_HI..=UNIT1_VALUE_LO => {
                let i = (off - UNIT0_VALUE_HI) / 4;
                let u = (i / 2) as usize;
                if i % 2 == 0 {
                    (self.latched[u] >> 32) as u32 & MASK20
                } else {
                    self.latched[u] as u32
                }
            }
            COMP0_LOAD..=UNIT1_LOAD => 0,
            INT_ENA => self.int_ena,
            INT_RAW => self.int_raw,
            INT_CLR => 0,
            INT_ST => self.int_raw & self.int_ena,
            REAL_TARGET0_LO..=REAL_TARGET2_HI => {
                let i = (off - REAL_TARGET0_LO) / 4;
                let n = (i / 2) as usize;
                // `real_target` clusters are `lo` then `hi` — the opposite of
                // `trgt` (discovery §1, the ordering note).
                if i % 2 == 0 {
                    self.real_target[n] as u32
                } else {
                    (self.real_target[n] >> 32) as u32 & MASK20
                }
            }
            DATE => self.date,
            _ => {
                let _ = now;
                0
            }
        }
    }

    fn write_word(&mut self, off: u32, value: u32, cx: &mut BusCx<'_>) {
        match off {
            CONF => self.write_conf(value, cx),
            UNIT0_OP | UNIT1_OP => {
                if value & OP_UPDATE != 0 {
                    let u = usize::from(off == UNIT1_OP);
                    self.latched[u] = self.count(u, cx.now);
                }
            }
            UNIT0_LOAD_HI..=UNIT1_LOAD_LO => {
                let i = (off - UNIT0_LOAD_HI) / 4;
                let u = (i / 2) as usize;
                self.load_staged[u] = if i % 2 == 0 {
                    (self.load_staged[u] & 0xffff_ffff) | (u64::from(value & MASK20) << 32)
                } else {
                    (self.load_staged[u] & !0xffff_ffff) | u64::from(value)
                };
            }
            TRGT0_HI..=TRGT2_LO => {
                let i = (off - TRGT0_HI) / 4;
                let n = (i / 2) as usize;
                self.target_staged[n] = if i % 2 == 0 {
                    (self.target_staged[n] & 0xffff_ffff) | (u64::from(value & MASK20) << 32)
                } else {
                    (self.target_staged[n] & !0xffff_ffff) | u64::from(value)
                };
            }
            TARGET0_CONF..=TARGET2_CONF => {
                let n = ((off - TARGET0_CONF) / 4) as usize;
                let old = self.target_conf[n];
                self.target_conf[n] = value;
                if (old ^ value) & (TC_PERIOD_MODE | TC_UNIT_SEL) != 0 {
                    self.rearm(n, cx);
                }
            }
            COMP0_LOAD..=COMP2_LOAD => {
                if value & 1 != 0 {
                    let n = ((off - COMP0_LOAD) / 4) as usize;
                    self.real_target[n] = self.target_staged[n] & MASK52;
                    self.period[n] = self.target_conf[n] & PERIOD_MASK;
                    self.rearm(n, cx);
                }
            }
            UNIT0_LOAD | UNIT1_LOAD => {
                if value & 1 != 0 {
                    let u = usize::from(off == UNIT1_LOAD);
                    let load = self.load_staged[u] & MASK52;
                    if self.frozen[u].is_some() {
                        self.frozen[u] = Some(load);
                    } else {
                        self.offset[u] = load.wrapping_sub(cx.now / CYCLES_PER_TICK) & MASK52;
                    }
                    for n in 0..3 {
                        if self.unit_of(n) == u {
                            self.rearm(n, cx);
                        }
                    }
                }
            }
            INT_ENA => {
                self.int_ena = value & 0b111;
                self.update_lines(cx);
            }
            INT_CLR => {
                self.int_raw &= !(value & 0b111);
                self.update_lines(cx);
            }
            DATE => self.date = value,
            // `unit_value`, `int_raw`, `int_st`, `real_target`: read-only.
            _ => {}
        }
    }
}

impl Peripheral for Systimer {
    fn name(&self) -> &'static str {
        "SYSTIMER"
    }

    fn attached(&mut self, index: usize) {
        self.index = index;
    }

    fn read(&mut self, off: u32, width: Width, cx: &mut BusCx<'_>) -> u32 {
        lane_of(self.read_word(off & !3, cx.now), off, width)
    }

    fn write(&mut self, off: u32, width: Width, value: u32, cx: &mut BusCx<'_>) {
        let word = off & !3;
        let merged = merge_lane(self.read_word(word, cx.now), off, width, value);
        self.write_word(word, merged, cx);
    }

    fn on_event(&mut self, id: EventId, cx: &mut BusCx<'_>) {
        let n = lp_emu_esp_common::event_local(id) as usize;
        if n >= 3 || self.conf & TARGET_WORK_EN[n] == 0 {
            return;
        }
        self.int_raw |= 1 << n;
        self.update_lines(cx);
        if self.period_mode(n) {
            if self.period[n] == 0 {
                log::warn!("SYSTIMER: comparator {n} in period mode with period 0; not re-armed");
                return;
            }
            self.real_target[n] = (self.real_target[n] + u64::from(self.period[n])) & MASK52;
            self.rearm(n, cx);
        }
    }

    fn reg_name(&self, off: u32) -> Option<&'static str> {
        regs::SYSTIMER.name(off)
    }

    fn save_state(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(160);
        out.extend_from_slice(&(self.index as u64).to_le_bytes());
        out.extend_from_slice(&self.conf.to_le_bytes());
        for u in 0..2 {
            out.extend_from_slice(&self.offset[u].to_le_bytes());
            out.extend_from_slice(&self.frozen[u].map_or(u64::MAX, |v| v).to_le_bytes());
            out.extend_from_slice(&self.latched[u].to_le_bytes());
            out.extend_from_slice(&self.load_staged[u].to_le_bytes());
        }
        for n in 0..3 {
            out.extend_from_slice(&self.target_staged[n].to_le_bytes());
            out.extend_from_slice(&self.target_conf[n].to_le_bytes());
            out.extend_from_slice(&self.real_target[n].to_le_bytes());
            out.extend_from_slice(&self.period[n].to_le_bytes());
        }
        out.extend_from_slice(&self.int_ena.to_le_bytes());
        out.extend_from_slice(&self.int_raw.to_le_bytes());
        out.extend_from_slice(&self.date.to_le_bytes());
        out
    }

    fn load_state(&mut self, bytes: &[u8]) {
        let mut r = Reader(bytes);
        let Some(index) = r.u64() else {
            log::warn!("SYSTIMER: load_state blob too short, ignored");
            return;
        };
        let mut s = Self::new();
        s.index = index as usize;
        s.conf = r.u32().unwrap_or(CONF_RESET);
        for u in 0..2 {
            s.offset[u] = r.u64().unwrap_or(0);
            s.frozen[u] = r.u64().filter(|v| *v != u64::MAX);
            s.latched[u] = r.u64().unwrap_or(0);
            s.load_staged[u] = r.u64().unwrap_or(0);
        }
        for n in 0..3 {
            s.target_staged[n] = r.u64().unwrap_or(0);
            s.target_conf[n] = r.u32().unwrap_or(0);
            s.real_target[n] = r.u64().unwrap_or(0);
            s.period[n] = r.u32().unwrap_or(0);
        }
        s.int_ena = r.u32().unwrap_or(0);
        s.int_raw = r.u32().unwrap_or(0);
        s.date = r.u32().unwrap_or(0);
        *self = s;
    }
}

/// A little-endian cursor for the state blobs.
pub(crate) struct Reader<'a>(pub &'a [u8]);

impl Reader<'_> {
    pub fn u32(&mut self) -> Option<u32> {
        let (head, rest) = self.0.split_first_chunk::<4>()?;
        self.0 = rest;
        Some(u32::from_le_bytes(*head))
    }

    pub fn u64(&mut self) -> Option<u64> {
        let (head, rest) = self.0.split_first_chunk::<8>()?;
        self.0 = rest;
        Some(u64::from_le_bytes(*head))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_emu_esp_common::Sandbox;

    /// esp-hal's `read_count` (`systimer.rs:364-388`).
    fn read_count(sb: &mut Sandbox, t: &mut Systimer, u: usize) -> u64 {
        let op = if u == 0 { UNIT0_OP } else { UNIT1_OP };
        sb.write(t, op, OP_UPDATE);
        assert!(sb.read(t, op) & OP_VALUE_VALID != 0, "value_valid after update");
        let base = UNIT0_VALUE_HI + 8 * u as u32;
        let lo = sb.read(t, base + 4);
        let hi = sb.read(t, base);
        let lo2 = sb.read(t, base + 4);
        assert_eq!(lo, lo2);
        (u64::from(hi) << 32) | u64::from(lo)
    }

    /// esp-hal's one-shot `schedule` on a systimer alarm (discovery §1).
    fn schedule(sb: &mut Sandbox, t: &mut Systimer, n: u32, ticks: u64) {
        let conf = sb.read(t, CONF);
        sb.write(t, CONF, conf & !TARGET_WORK_EN[n as usize]); // stop
        sb.write(t, INT_CLR, 1 << n);
        let now = read_count(sb, t, 0);
        let target = now + ticks;
        sb.write(t, TRGT0_HI + 8 * n, (target >> 32) as u32);
        sb.write(t, TRGT0_HI + 8 * n + 4, target as u32);
        sb.write(t, COMP0_LOAD + 4 * n, 1);
        let conf = sb.read(t, CONF);
        sb.write(t, CONF, conf | TARGET_WORK_EN[n as usize]); // start
    }

    #[test]
    fn unit0_counts_at_sixteen_megahertz_from_cycles_and_unit1_is_stopped_at_reset() {
        let mut sb = Sandbox::new();
        let mut t = Systimer::new();
        assert_eq!(sb.read(&mut t, CONF), CONF_RESET);
        sb.now = 1_600_000; // 10 ms
        assert_eq!(read_count(&mut sb, &mut t, 0), 160_000);
        sb.now = 1_600_010;
        assert_eq!(read_count(&mut sb, &mut t, 0), 160_001);
        // Unit1: work_en clear at reset, so it holds 0.
        assert_eq!(read_count(&mut sb, &mut t, 1), 0);
        // Start it: it counts from where it was, not from cycles/10.
        sb.write(&mut t, CONF, CONF_RESET | UNIT_WORK_EN[1]);
        sb.now = 1_600_110;
        assert_eq!(read_count(&mut sb, &mut t, 1), 10);
        assert_eq!(read_count(&mut sb, &mut t, 0), 160_011);
    }

    #[test]
    fn a_one_shot_alarm_fires_at_the_target_and_drives_the_source_while_enabled() {
        let mut sb = Sandbox::new();
        let mut t = Systimer::new();
        t.attached(7);
        sb.write(&mut t, INT_ENA, 1);
        sb.now = 1000;
        schedule(&mut sb, &mut t, 0, 100); // fires at count 200 = cycle 2000
        assert_eq!(sb.sched.next_deadline(), Some(2000));
        assert_eq!(sb.read(&mut t, REAL_TARGET0_LO), 200);
        sb.run_to(&mut t, 1999);
        assert!(!sb.irq.level(source::SYSTIMER_TARGET0));
        sb.run_to(&mut t, 2000);
        assert!(sb.irq.level(source::SYSTIMER_TARGET0));
        assert_eq!(sb.read(&mut t, INT_RAW), 1);
        assert_eq!(sb.read(&mut t, INT_ST), 1);
        // One-shot: nothing else pending.
        assert_eq!(sb.sched.next_deadline(), None);
        // The handler clears it.
        sb.write(&mut t, INT_CLR, 1);
        assert!(!sb.irq.level(source::SYSTIMER_TARGET0));
        assert_eq!(sb.read(&mut t, INT_RAW), 0);
        assert_eq!(sb.read(&mut t, INT_CLR), 0, "int_clr reads 0");
    }

    #[test]
    fn int_ena_gates_the_level_not_the_raw_bit() {
        let mut sb = Sandbox::new();
        let mut t = Systimer::new();
        sb.now = 1000;
        schedule(&mut sb, &mut t, 1, 10);
        sb.run_to(&mut t, 1100);
        assert_eq!(sb.read(&mut t, INT_RAW), 2);
        assert!(!sb.irq.level(source::SYSTIMER_TARGET1), "not enabled");
        sb.write(&mut t, INT_ENA, 2);
        assert!(sb.irq.level(source::SYSTIMER_TARGET1));
        sb.write(&mut t, INT_ENA, 0);
        assert!(!sb.irq.level(source::SYSTIMER_TARGET1));
    }

    #[test]
    fn comp_load_cancels_and_re_arms_and_a_past_target_fires_at_once() {
        let mut sb = Sandbox::new();
        let mut t = Systimer::new();
        sb.now = 1000;
        schedule(&mut sb, &mut t, 0, 1000);
        assert_eq!(sb.sched.next_deadline(), Some(11_000));
        // Re-arm sooner: the old deadline is gone.
        sb.now = 2000;
        schedule(&mut sb, &mut t, 0, 10);
        assert_eq!(sb.sched.next_deadline(), Some(2100));
        assert_eq!(sb.sched.live(), 1);
        // A target already passed: due now, not in nine years.
        sb.now = 5000;
        sb.write(&mut t, TRGT0_HI, 0);
        sb.write(&mut t, TRGT0_HI + 4, 1);
        sb.write(&mut t, COMP0_LOAD, 1);
        assert_eq!(sb.sched.next_deadline(), Some(5000));
        // Disarming cancels.
        let conf = sb.read(&mut t, CONF);
        sb.write(&mut t, CONF, conf & !TARGET_WORK_EN[0]);
        assert_eq!(sb.sched.next_deadline(), None);
    }

    #[test]
    fn period_mode_re_arms_itself_at_plus_period() {
        let mut sb = Sandbox::new();
        let mut t = Systimer::new();
        sb.write(&mut t, INT_ENA, 4);
        // esp-hal's period-mode load_value: period, comp_load, mode off/on.
        sb.write(&mut t, TARGET2_CONF, 50);
        sb.write(&mut t, COMP2_LOAD, 1);
        sb.write(&mut t, TARGET2_CONF, 50);
        sb.write(&mut t, TARGET2_CONF, 50 | TC_PERIOD_MODE);
        let conf = sb.read(&mut t, CONF);
        sb.write(&mut t, CONF, conf | TARGET_WORK_EN[2]);
        // Target 0 is in the past: fires now, then every 50 ticks.
        assert_eq!(sb.sched.next_deadline(), Some(0));
        sb.run_to(&mut t, 0);
        assert_eq!(sb.read(&mut t, REAL_TARGET2_HI - 4), 50);
        assert_eq!(sb.sched.next_deadline(), Some(500));
        sb.write(&mut t, INT_CLR, 4);
        sb.run_to(&mut t, 500);
        assert!(sb.irq.level(source::SYSTIMER_TARGET2));
        assert_eq!(sb.sched.next_deadline(), Some(1000));
    }

    #[test]
    fn unit_load_moves_the_count_and_the_state_round_trips() {
        let mut sb = Sandbox::new();
        let mut t = Systimer::new();
        t.attached(3);
        sb.now = 10_000;
        sb.write(&mut t, UNIT0_LOAD_HI, 0x1);
        sb.write(&mut t, UNIT0_LOAD_HI + 4, 0x2000);
        sb.write(&mut t, UNIT0_LOAD, 1);
        assert_eq!(read_count(&mut sb, &mut t, 0), 0x1_0000_2000);
        sb.now = 10_100;
        assert_eq!(read_count(&mut sb, &mut t, 0), 0x1_0000_200a);

        let blob = t.save_state();
        let mut other = Systimer::new();
        other.load_state(&blob);
        assert_eq!(other, t);
    }
}
