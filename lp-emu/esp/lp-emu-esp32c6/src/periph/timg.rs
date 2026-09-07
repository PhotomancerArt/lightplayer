//! `TIMG0` / `TIMG1` at `0x6000_8000` / `0x6000_9000` — timer T0 modelled,
//! the RTC calibration modelled, the MWDT accepted.
//!
//! **This is the tick.** `esp_rtos::start` is handed `timg0.timer0`
//! (`fw-esp32c6/src/board/esp32c6/init.rs:76`), so esp-rtos's one-shot
//! `arm_next_wakeup` and its `timer_tick_handler` run against T0 here, not
//! against a SYSTIMER comparator. The C6 TIMG has one timer
//! (`timg0.rs:5`, `t: [T; 1]`).
//!
//! # T0 (`esp-hal-1.1.1/src/timer/timg.rs`, cited per line)
//!
//! - Source clock: **XTAL, 40 MHz** — `TimerGroup::new` configures
//!   `TimgFunctionClockConfig::default()` (`timg.rs:200`), which the clock
//!   tree initialises as `XtalClk` (`clock/mod.rs:152`). The prescaler is
//!   `config.divider` (bits 13:28) with esp-hal's reading of the TRM:
//!   0 → 65536, 1 or 2 → 2, else n (`timg.rs:493-506`). `t0config` resets to
//!   `0x6000_2000`: increase, auto-reload, divider 1 → **20 MHz ticks**.
//!   *Modeled*: the block cannot see PCR's `timer_clk_sel`; the XTAL choice
//!   is esp-hal 1.1.1's and is stated here rather than read.
//! - `now()`: write `update` (bit 31), poll it until 0, read `lo` then `hi`
//!   (`timg.rs:474-486`). `update` is a pulse; the value is latched.
//! - `schedule(timeout)`: `stop` (`en = 0`), `clear_interrupt`, `reset`
//!   (`loadlo/hi = 0`, `load = 1`), `autoreload = 0`, `alarmlo/hi = ticks`,
//!   `start` (`en = 0`, `alarm_en = 0`, reset, `increase = 1`, `en = 1`,
//!   `alarm_en = 1`) — `timer/mod.rs:274-286`, `timg.rs:225-234`.
//! - The alarm fires when the count reaches `alarm`; `alarm_en` clears
//!   itself ("automatically cleared once an alarm occurs", the PAC's field
//!   doc) and `int_raw.t0` sets. The source `TG0_T0_LEVEL` (51; `TG1`: 54)
//!   is the level `int_raw & int_ena`. `clear_interrupt` writes `int_clr.t0`
//!   and re-enables the alarm only when auto-reload is on (`timg.rs:467-473`).
//! - Decrementing mode (`increase = 0`) is not modelled: esp-hal sets
//!   `increase = 1` in `start`. A write that clears it is logged once.
//!
//! # RTC calibration (`clock/mod.rs:276-473`, discovery §2)
//!
//! `rtccalicfg.rtc_cali_start` (bit 31) starts a measurement of
//! `rtc_cali_max` (bits 16:30) slow-clock cycles; `rtc_cali_rdy` (bit 15)
//! reads 1 when it is done and `rtccalicfg1.rtc_cali_value` (bits 7:31)
//! holds the XTAL cycles counted. The value is **modeled** as
//! `XTAL_HZ * max / RC_SLOW_HZ` (`40_000_000 * 1024 / 136_000 ≈ 301_176`),
//! delivered after `max / RC_SLOW_HZ` of emulated time. `rtccalicfg` resets
//! to `0x0001_1000` (`start_cycling` set, `max = 1`), and a cycling
//! calibration is modelled as already complete, so the first poll
//! (`clock/mod.rs:298-313`, entered only when `start_cycling` reads set)
//! exits at once. `rtccalicfg2.rtc_cali_timeout` reads 0.
//!
//! # MWDT
//!
//! `wdtconfig0..5` and `wdtfeed` take writes only while `wdtwprotect` holds
//! `0x50D8_3AA1`; `esp_hal::init` unlocks, clears `wdt_en`, locks
//! (`lib.rs:751-761`). Expiry is **not** modelled: a write that sets
//! `wdt_en` puts a `WDT ARMED` note in the trace so an image that does arm
//! it is visible.

use lp_emu_core::sched::EventId;
use lp_emu_esp_common::regfile::{lane_of, merge_lane};
use lp_emu_esp_common::{BusCx, Peripheral, RegFile, Width, event_id, event_local};

use super::systimer::Reader;
use super::{RC_SLOW_HZ, WDT_WKEY, XTAL_HZ};
use crate::memmap;
use crate::regs::{self, source};

// T0 (`regs::TIMG0`, cluster `t0`).
const T0_CONFIG: u32 = 0x00;
const T0_LO: u32 = 0x04;
const T0_HI: u32 = 0x08;
const T0_UPDATE: u32 = 0x0c;
const T0_ALARMLO: u32 = 0x10;
const T0_ALARMHI: u32 = 0x14;
const T0_LOADLO: u32 = 0x18;
const T0_LOADHI: u32 = 0x1c;
const T0_LOAD: u32 = 0x20;
// WDT.
const WDTCONFIG0: u32 = 0x48;
const WDTFEED: u32 = 0x60;
const WDTWPROTECT: u32 = 0x64;
// Calibration + interrupts.
const RTCCALICFG: u32 = 0x68;
const RTCCALICFG1: u32 = 0x6c;
const INT_ENA: u32 = 0x70;
const INT_RAW: u32 = 0x74;
const INT_ST: u32 = 0x78;
const INT_CLR: u32 = 0x7c;
const RTCCALICFG2: u32 = 0x80;

const CONFIG_RESET: u32 = 0x6000_2000;
const CFG_ALARM_EN: u32 = 1 << 10;
const CFG_DIVCNT_RST: u32 = 1 << 12;
const CFG_DIVIDER_SHIFT: u32 = 13;
const CFG_DIVIDER_MASK: u32 = 0xffff;
const CFG_AUTORELOAD: u32 = 1 << 29;
const CFG_INCREASE: u32 = 1 << 30;
const CFG_EN: u32 = 1 << 31;
const UPDATE_BIT: u32 = 1 << 31;
const COUNTER_MASK: u64 = (1 << 54) - 1;

const WDTCONFIG0_RESET: u32 = 0x0004_c000;
const WDT_EN: u32 = 1 << 31;

const CALI_RESET: u32 = 0x0001_1000;
const CALI_RDY: u32 = 1 << 15;
const CALI_START_CYCLING: u32 = 1 << 12;
const CALI_MAX_SHIFT: u32 = 16;
const CALI_MAX_MASK: u32 = 0x7fff;
const CALI_START: u32 = 1 << 31;
const CALI2_RESET: u32 = 0xffff_ff98;
const CALI2_TIMEOUT: u32 = 1;

const EV_ALARM: u16 = 0;
const EV_CALI: u16 = 1;

/// One timer group.
#[derive(Debug)]
pub struct Timg {
    name: &'static str,
    index: usize,
    /// Every register's stored value, and the block's names.
    regs: RegFile,
    /// The count at `base_cycle`.
    base_ticks: u64,
    base_cycle: u64,
    latched: u64,
    cali_rdy: bool,
    cali_value: u32,
    t0_source: u16,
    wdt_source: u16,
    warned_decrement: bool,
}

impl Timg {
    fn new(name: &'static str, t0_source: u16, wdt_source: u16) -> Self {
        Self {
            name,
            index: 0,
            regs: RegFile::new(name, 0x100)
                .with_names(regs::TIMG0)
                .with_reset(T0_CONFIG, CONFIG_RESET)
                .with_reset(WDTCONFIG0, WDTCONFIG0_RESET)
                .with_reset(RTCCALICFG, CALI_RESET)
                .with_reset(RTCCALICFG2, CALI2_RESET),
            base_ticks: 0,
            base_cycle: 0,
            latched: 0,
            // Reset: `start_cycling` set → a cycling calibration that has
            // already completed once (modeled).
            cali_rdy: true,
            cali_value: cali_value_for(1),
            t0_source,
            wdt_source,
            warned_decrement: false,
        }
    }

    pub fn timg0() -> Self {
        Self::new("TIMG0", source::TG0_T0_LEVEL, source::TG0_WDT_LEVEL)
    }

    pub fn timg1() -> Self {
        Self::new("TIMG1", source::TG1_T0_LEVEL, source::TG1_WDT_LEVEL)
    }

    fn config(&self) -> u32 {
        self.regs.stored(T0_CONFIG)
    }

    /// esp-hal's reading of the prescaler (`timg.rs:493-506`).
    fn divider(&self) -> u64 {
        match (self.config() >> CFG_DIVIDER_SHIFT) & CFG_DIVIDER_MASK {
            0 => 65536,
            1 | 2 => 2,
            n => u64::from(n),
        }
    }

    fn enabled(&self) -> bool {
        self.config() & CFG_EN != 0
    }

    /// The count at `now`.
    pub fn count(&self, now: u64) -> u64 {
        if !self.enabled() {
            return self.base_ticks;
        }
        let delta = u128::from(now.saturating_sub(self.base_cycle));
        let ticks = delta * u128::from(XTAL_HZ) / (u128::from(self.divider()) * u128::from(memmap::CPU_HZ));
        (self.base_ticks + ticks as u64) & COUNTER_MASK
    }

    fn alarm(&self) -> u64 {
        ((u64::from(self.regs.stored(T0_ALARMHI)) << 32) | u64::from(self.regs.stored(T0_ALARMLO)))
            & COUNTER_MASK
    }

    fn load_value(&self) -> u64 {
        ((u64::from(self.regs.stored(T0_LOADHI)) << 32) | u64::from(self.regs.stored(T0_LOADLO)))
            & COUNTER_MASK
    }

    fn update_lines(&self, cx: &mut BusCx<'_>) {
        let st = self.regs.stored(INT_RAW) & self.regs.stored(INT_ENA);
        cx.irq.set_level(self.t0_source, st & 1 != 0);
        cx.irq.set_level(self.wdt_source, st & 2 != 0);
    }

    fn rearm(&mut self, cx: &mut BusCx<'_>) {
        let ev = event_id(self.index, EV_ALARM);
        cx.sched.cancel(ev);
        let cfg = self.config();
        if cfg & CFG_EN == 0 || cfg & CFG_ALARM_EN == 0 {
            return;
        }
        let alarm = self.alarm();
        let now = self.count(cx.now);
        if alarm <= now {
            cx.sched.schedule_at(cx.now, ev);
            return;
        }
        // Cycles for `alarm - base_ticks` ticks, rounded up.
        let ticks = u128::from(alarm - self.base_ticks);
        let num = ticks * u128::from(self.divider()) * u128::from(memmap::CPU_HZ);
        let den = u128::from(XTAL_HZ);
        let cycles = num.div_ceil(den) as u64;
        cx.sched.schedule_at(self.base_cycle.saturating_add(cycles), ev);
    }

    fn write_config(&mut self, value: u32, cx: &mut BusCx<'_>) {
        let old = self.config();
        let mut new = value;
        if new & CFG_INCREASE == 0 && !self.warned_decrement {
            self.warned_decrement = true;
            log::warn!("{}: t0config.increase cleared; decrementing mode is not modelled", self.name);
        }
        if (old ^ new) & CFG_EN != 0 {
            if new & CFG_EN == 0 {
                // Freeze at the current count.
                self.base_ticks = self.count(cx.now);
            } else {
                self.base_cycle = cx.now;
            }
        }
        // The divider-counter reset is a pulse.
        new &= !CFG_DIVCNT_RST;
        self.regs.poke(T0_CONFIG, new);
        self.rearm(cx);
    }

    fn wdt_unlocked(&self) -> bool {
        self.regs.stored(WDTWPROTECT) == WDT_WKEY
    }

    fn read_word(&self, off: u32) -> u32 {
        match off {
            T0_LO => self.latched as u32,
            T0_HI => (self.latched >> 32) as u32,
            T0_UPDATE | T0_LOAD | WDTFEED | INT_CLR => 0,
            RTCCALICFG => {
                let v = self.regs.stored(RTCCALICFG) & !CALI_RDY;
                if self.cali_rdy { v | CALI_RDY } else { v }
            }
            RTCCALICFG1 => self.cali_value << 7,
            RTCCALICFG2 => self.regs.stored(RTCCALICFG2) & !CALI2_TIMEOUT,
            INT_ST => self.regs.stored(INT_RAW) & self.regs.stored(INT_ENA),
            other => self.regs.stored(other),
        }
    }

    fn write_word(&mut self, off: u32, value: u32, cx: &mut BusCx<'_>) {
        match off {
            T0_CONFIG => self.write_config(value, cx),
            T0_UPDATE => {
                if value & UPDATE_BIT != 0 {
                    self.latched = self.count(cx.now);
                }
            }
            T0_ALARMLO | T0_ALARMHI => {
                self.regs.poke(off, value);
                self.rearm(cx);
            }
            T0_LOADLO | T0_LOADHI => self.regs.poke(off, value),
            T0_LOAD => {
                if value & 1 != 0 {
                    self.base_ticks = self.load_value();
                    self.base_cycle = cx.now;
                    self.rearm(cx);
                }
            }
            WDTCONFIG0..=WDTFEED => {
                if !self.wdt_unlocked() {
                    log::debug!("{}: write to +{off:#05x} dropped, WDT locked", self.name);
                    return;
                }
                if off == WDTFEED {
                    return;
                }
                let old = self.regs.stored(off);
                self.regs.poke(off, value);
                if off == WDTCONFIG0 && value & WDT_EN != 0 && old & WDT_EN == 0 {
                    let line = format!(
                        "cyc={} pc=0x{:08x} {} WDT ARMED (MWDT expiry is not modelled)",
                        cx.now, cx.pc, self.name
                    );
                    cx.trace.note(&line);
                    log::warn!("{}: MWDT armed; its expiry is not modelled", self.name);
                }
            }
            RTCCALICFG => {
                let old = self.regs.stored(RTCCALICFG);
                self.regs.poke(RTCCALICFG, value & !CALI_RDY);
                let ev = event_id(self.index, EV_CALI);
                let running = CALI_START | CALI_START_CYCLING;
                if value & CALI_START != 0 && old & CALI_START == 0 {
                    // A one-shot measurement starts: not ready until it ends.
                    self.cali_rdy = false;
                    cx.sched.cancel(ev);
                    let max = u64::from(((value >> CALI_MAX_SHIFT) & CALI_MAX_MASK).max(1));
                    let cycles = max * memmap::CPU_HZ / RC_SLOW_HZ;
                    cx.sched.schedule_in(cx.now, cycles, ev);
                } else if value & running == 0 && old & running != 0 {
                    // Neither one-shot nor cycling any more: nothing to be
                    // ready about. (Clearing `start` while `start_cycling`
                    // stays set leaves the cycling result readable.)
                    self.cali_rdy = false;
                    cx.sched.cancel(ev);
                }
            }
            RTCCALICFG1 => {}
            INT_ENA => {
                self.regs.poke(INT_ENA, value & 0b11);
                self.update_lines(cx);
            }
            INT_RAW | INT_ST | T0_LO | T0_HI => {}
            INT_CLR => {
                let raw = self.regs.stored(INT_RAW) & !(value & 0b11);
                self.regs.poke(INT_RAW, raw);
                self.update_lines(cx);
            }
            other => self.regs.poke(other, value),
        }
    }
}

/// The calibration result for `max` slow-clock cycles, modeled.
pub fn cali_value_for(max: u32) -> u32 {
    (XTAL_HZ * u64::from(max) / RC_SLOW_HZ) as u32
}

impl Peripheral for Timg {
    fn name(&self) -> &'static str {
        self.name
    }

    fn attached(&mut self, index: usize) {
        self.index = index;
    }

    fn read(&mut self, off: u32, width: Width, _cx: &mut BusCx<'_>) -> u32 {
        lane_of(self.read_word(off & !3), off, width)
    }

    fn write(&mut self, off: u32, width: Width, value: u32, cx: &mut BusCx<'_>) {
        let word = off & !3;
        let merged = merge_lane(self.read_word(word), off, width, value);
        self.write_word(word, merged, cx);
    }

    fn on_event(&mut self, id: EventId, cx: &mut BusCx<'_>) {
        match event_local(id) {
            EV_ALARM => {
                let cfg = self.config();
                if cfg & CFG_EN == 0 || cfg & CFG_ALARM_EN == 0 {
                    return;
                }
                if self.count(cx.now) < self.alarm() {
                    // Re-armed for later since this was scheduled.
                    return;
                }
                self.regs.poke(T0_CONFIG, cfg & !CFG_ALARM_EN);
                if cfg & CFG_AUTORELOAD != 0 {
                    self.base_ticks = self.load_value();
                    self.base_cycle = cx.now;
                }
                self.regs.poke(INT_RAW, self.regs.stored(INT_RAW) | 1);
                self.update_lines(cx);
            }
            EV_CALI => {
                let max = (self.regs.stored(RTCCALICFG) >> CALI_MAX_SHIFT) & CALI_MAX_MASK;
                self.cali_value = cali_value_for(max.max(1));
                self.cali_rdy = true;
            }
            _ => {}
        }
    }

    fn reg_name(&self, off: u32) -> Option<&'static str> {
        regs::TIMG0.name(off)
    }

    fn save_state(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(0x100 + 48);
        out.extend_from_slice(&(self.index as u64).to_le_bytes());
        out.extend_from_slice(&self.base_ticks.to_le_bytes());
        out.extend_from_slice(&self.base_cycle.to_le_bytes());
        out.extend_from_slice(&self.latched.to_le_bytes());
        out.extend_from_slice(&u32::from(self.cali_rdy).to_le_bytes());
        out.extend_from_slice(&self.cali_value.to_le_bytes());
        out.extend_from_slice(&u32::from(self.warned_decrement).to_le_bytes());
        out.extend_from_slice(&self.regs.save_state());
        out
    }

    fn load_state(&mut self, bytes: &[u8]) {
        let mut r = Reader(bytes);
        let (Some(index), Some(base_ticks), Some(base_cycle), Some(latched)) =
            (r.u64(), r.u64(), r.u64(), r.u64())
        else {
            log::warn!("{}: load_state blob too short, ignored", self.name);
            return;
        };
        self.index = index as usize;
        self.base_ticks = base_ticks;
        self.base_cycle = base_cycle;
        self.latched = latched;
        self.cali_rdy = r.u32().unwrap_or(1) != 0;
        self.cali_value = r.u32().unwrap_or(0);
        self.warned_decrement = r.u32().unwrap_or(0) != 0;
        self.regs.load_state(r.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_emu_esp_common::Sandbox;

    /// esp-hal's `now()` (`timg.rs:474-486`).
    fn now(sb: &mut Sandbox, t: &mut Timg) -> u64 {
        sb.write(t, T0_UPDATE, UPDATE_BIT);
        assert_eq!(sb.read(t, T0_UPDATE) & UPDATE_BIT, 0, "update is a pulse");
        let lo = sb.read(t, T0_LO);
        let hi = sb.read(t, T0_HI);
        (u64::from(hi) << 32) | u64::from(lo)
    }

    /// esp-hal's `OneShotTimer::schedule` on a TIMG timer, register for
    /// register (`timer/mod.rs:274-286` + `timg.rs:225-234`).
    fn schedule(sb: &mut Sandbox, t: &mut Timg, ticks: u64) {
        let cfg = sb.read(t, T0_CONFIG);
        sb.write(t, T0_CONFIG, cfg & !CFG_EN); // stop
        sb.write(t, INT_CLR, 1); // clear_interrupt
        let cfg = sb.read(t, T0_CONFIG);
        sb.write(t, T0_CONFIG, cfg & !CFG_ALARM_EN); // set_alarm_active(autoreload=false)
        sb.write(t, T0_LOADLO, 0); // reset
        sb.write(t, T0_LOADHI, 0);
        sb.write(t, T0_LOAD, 1);
        let cfg = sb.read(t, T0_CONFIG);
        sb.write(t, T0_CONFIG, cfg & !CFG_AUTORELOAD); // enable_auto_reload(false)
        sb.write(t, T0_ALARMLO, ticks as u32); // load_value
        sb.write(t, T0_ALARMHI, (ticks >> 32) as u32);
        // start
        let cfg = sb.read(t, T0_CONFIG);
        sb.write(t, T0_CONFIG, cfg & !CFG_EN);
        let cfg = sb.read(t, T0_CONFIG);
        sb.write(t, T0_CONFIG, cfg & !CFG_ALARM_EN);
        sb.write(t, T0_LOADLO, 0);
        sb.write(t, T0_LOADHI, 0);
        sb.write(t, T0_LOAD, 1);
        let cfg = sb.read(t, T0_CONFIG);
        sb.write(t, T0_CONFIG, cfg | CFG_INCREASE);
        let cfg = sb.read(t, T0_CONFIG);
        sb.write(t, T0_CONFIG, cfg | CFG_EN);
        let cfg = sb.read(t, T0_CONFIG);
        sb.write(t, T0_CONFIG, cfg | CFG_ALARM_EN);
    }

    #[test]
    fn t0_ticks_at_twenty_megahertz_with_the_reset_prescaler() {
        let mut sb = Sandbox::new();
        let mut t = Timg::timg0();
        assert_eq!(sb.read(&mut t, T0_CONFIG), CONFIG_RESET);
        // Disabled at reset: holds 0.
        sb.now = 1_600_000;
        assert_eq!(now(&mut sb, &mut t), 0);
        sb.write(&mut t, T0_CONFIG, CONFIG_RESET | CFG_EN);
        sb.now = 3_200_000; // +10 ms = 200_000 ticks at 20 MHz
        assert_eq!(now(&mut sb, &mut t), 200_000);
        // Stop freezes; restart continues.
        sb.write(&mut t, T0_CONFIG, CONFIG_RESET);
        sb.now = 4_000_000;
        assert_eq!(now(&mut sb, &mut t), 200_000);
        sb.write(&mut t, T0_CONFIG, CONFIG_RESET | CFG_EN);
        sb.now = 4_000_008;
        assert_eq!(now(&mut sb, &mut t), 200_001);
    }

    #[test]
    fn the_esp_rtos_tick_fires_ten_ms_after_schedule_and_clears_alarm_en() {
        let mut sb = Sandbox::new();
        let mut t = Timg::timg0();
        t.attached(4);
        sb.write(&mut t, INT_ENA, 1);
        sb.now = 1_000_000;
        schedule(&mut sb, &mut t, 200_000); // 10 ms at 20 MHz
        assert_eq!(sb.sched.next_deadline(), Some(1_000_000 + 1_600_000));
        sb.run_to(&mut t, 2_599_999);
        assert!(!sb.irq.level(source::TG0_T0_LEVEL));
        sb.run_to(&mut t, 2_600_000);
        assert!(sb.irq.level(source::TG0_T0_LEVEL));
        assert_eq!(sb.read(&mut t, INT_RAW), 1);
        assert_eq!(sb.read(&mut t, INT_ST), 1);
        assert_eq!(
            sb.read(&mut t, T0_CONFIG) & CFG_ALARM_EN,
            0,
            "alarm_en clears itself"
        );
        // timer_tick_handler: clear_interrupt, then a fresh schedule.
        sb.write(&mut t, INT_CLR, 1);
        assert!(!sb.irq.level(source::TG0_T0_LEVEL));
        assert_eq!(sb.read(&mut t, INT_CLR), 0);
        schedule(&mut sb, &mut t, 20_000); // 1 ms
        assert_eq!(sb.sched.next_deadline(), Some(2_600_000 + 160_000));
        assert_eq!(sb.sched.live(), 1, "the re-arm replaced, it did not add");
    }

    #[test]
    fn an_alarm_already_passed_is_due_now_and_a_disabled_timer_never_fires() {
        let mut sb = Sandbox::new();
        let mut t = Timg::timg0();
        sb.now = 5_000;
        sb.write(&mut t, T0_CONFIG, CONFIG_RESET | CFG_EN);
        sb.now = 5_000 + 8 * 100;
        sb.write(&mut t, T0_ALARMLO, 50);
        let cfg = sb.read(&mut t, T0_CONFIG);
        sb.write(&mut t, T0_CONFIG, cfg | CFG_ALARM_EN);
        assert_eq!(sb.sched.next_deadline(), Some(5_800));
        sb.write(&mut t, T0_CONFIG, CONFIG_RESET); // en = 0
        assert_eq!(sb.sched.next_deadline(), None);
    }

    #[test]
    fn the_mwdt_is_write_protected_and_arming_it_leaves_a_note() {
        let buf = lp_emu_esp_common::trace::SharedBuffer::new();
        let mut sb = Sandbox::new();
        sb.trace = lp_emu_esp_common::Trace::to_sink(Box::new(buf.clone()));
        let mut t = Timg::timg0();
        assert_eq!(sb.read(&mut t, WDTCONFIG0), WDTCONFIG0_RESET);
        sb.write(&mut t, WDTCONFIG0, 0);
        assert_eq!(sb.read(&mut t, WDTCONFIG0), WDTCONFIG0_RESET, "locked");
        // esp_hal::init's disable: unlock, clear wdt_en, lock.
        sb.write(&mut t, WDTWPROTECT, WDT_WKEY);
        sb.write(&mut t, WDTCONFIG0, 0);
        sb.write(&mut t, WDTWPROTECT, 0);
        assert_eq!(sb.read(&mut t, WDTCONFIG0), 0);
        assert!(buf.lines().is_empty());
        sb.write(&mut t, WDTWPROTECT, WDT_WKEY);
        sb.write(&mut t, WDTCONFIG0, WDT_EN);
        sb.write(&mut t, WDTFEED, 1);
        assert_eq!(sb.read(&mut t, WDTFEED), 0);
        assert_eq!(buf.lines().len(), 1);
        assert!(buf.lines()[0].contains("TIMG0 WDT ARMED"));
    }

    #[test]
    fn the_rtc_calibration_answers_the_two_esp_hal_polls() {
        let mut sb = Sandbox::new();
        let mut t = Timg::timg0();
        t.attached(2);
        // Poll loop 1: entered because start_cycling reads set; exits on rdy.
        let cfg = sb.read(&mut t, RTCCALICFG);
        assert!(cfg & CALI_START_CYCLING != 0);
        assert!(cfg & CALI_RDY != 0, "a cycling calibration has completed");
        // measure_rtc_clock: `rtc_cali_start.clear_bit()` (a modify, so
        // `start_cycling` stays), then max = 1024, cycling off, start on.
        sb.write(&mut t, RTCCALICFG2, 0);
        sb.write(&mut t, RTCCALICFG, cfg & !CALI_START);
        assert!(
            sb.read(&mut t, RTCCALICFG) & CALI_RDY != 0,
            "still cycling: the cycling result stays readable"
        );
        sb.now = 100;
        sb.write(&mut t, RTCCALICFG, (1024 << CALI_MAX_SHIFT) | CALI_START);
        assert_eq!(sb.read(&mut t, RTCCALICFG) & CALI_RDY, 0);
        assert_eq!(sb.read(&mut t, RTCCALICFG2) & CALI2_TIMEOUT, 0);
        let expected_cycles = 1024 * memmap::CPU_HZ / RC_SLOW_HZ;
        assert_eq!(sb.sched.next_deadline(), Some(100 + expected_cycles));
        sb.run_to(&mut t, 100 + expected_cycles);
        assert!(sb.read(&mut t, RTCCALICFG) & CALI_RDY != 0);
        assert_eq!(sb.read(&mut t, RTCCALICFG1) >> 7, 301_176);
        assert_eq!(cali_value_for(1024), 301_176);
    }

    #[test]
    fn the_state_round_trips() {
        let mut sb = Sandbox::new();
        let mut t = Timg::timg0();
        t.attached(9);
        sb.now = 777;
        sb.write(&mut t, T0_CONFIG, CONFIG_RESET | CFG_EN);
        sb.now = 1_777;
        now(&mut sb, &mut t);
        let blob = t.save_state();
        let mut other = Timg::timg0();
        other.load_state(&blob);
        assert_eq!(other.count(sb.now), t.count(sb.now));
        assert_eq!(other.latched, t.latched);
        assert_eq!(other.index, 9);
        assert_eq!(other.regs.stored(T0_CONFIG), CONFIG_RESET | CFG_EN);
    }
}
