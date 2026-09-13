//! `TIMG0` / `TIMG1` at `0x6001_F000` / `0x6002_0000` — **two** counters
//! per group modelled, the RTC calibration modelled, the MWDT accepted.
//!
//! **The C6's view, parameterised by the counter count** (`m6/notes.md`
//! §3.0 row 8): the timer cluster's internals are identical on all three
//! chips (stride `0x24`: `config +0x00`, `lo +0x04`, `hi +0x08`, `update
//! +0x0c`, `alarmlo +0x10`, `alarmhi +0x14`, `loadlo +0x18`, `loadhi
//! +0x1c`, `load +0x20`); the S3 has **two** where the C6 has one; and
//! everything from `+0x48` to `+0xfc` — `wdtconfig0..5`, `wdtfeed`,
//! `wdtwprotect`, `rtccalicfg{,1,2}`, `int_*`, `regclk` — is byte-identical
//! S3 ≡ C6 (`regs::TIMG0`, generated from the S3's PAC).
//!
//! ⚠️ **Not the classic's view, and there is no LACT.** The classic's LACT
//! block sits at `+0x70..+0x94` and pushes its `int_ena` to `+0x98`; on this
//! chip `+0x70` *is* `int_ena`, and `grep -rn lact` over the S3's PAC
//! returns nothing. `tests/clock.rs` asserts the absence rather than
//! assuming it. The S3's `Instant::now()` is [`super::systimer`].
//!
//! **This is the tick.** `esp_rtos::start` is handed `timg0`
//! (`fw-esp32s3/src/board/esp32s3/init.rs:69`), so esp-rtos's one-shot
//! `arm_next_wakeup` and its `timer_tick_handler` run against T0 here.
//!
//! # The counters (`esp-hal/src/timer/timg.rs`, cited per line)
//!
//! - Source clock: **XTAL or APB, per the group's `use_xtal` bit**
//!   (`config` bit 9 — the PAC's field doc: "1: Use XTAL_CLK as the source
//!   clock of timer group. 0: Use APB_CLK"). esp-hal's
//!   `TimerGroup::new` configures `TimgFunctionClockConfig::default()`
//!   (`timg.rs:200`), which the S3's generated clock tree defaults to
//!   `XtalClk` (`_generated_esp32s3.rs:1308-1313`), and the S3 clock tree
//!   writes that choice into **both** counters' `use_xtal`
//!   (`soc/esp32s3/clocks.rs:838-849`). The PAC reset `0x6000_2000` has the
//!   bit **clear**, so a group counts APB until the driver configures it —
//!   which is why the rate is read out of the register rather than assumed.
//!   The prescaler is `divider` (bits 13:28) with esp-hal's reading of the
//!   TRM: 0 → 65536, 1 or 2 → 2, else n (`timg.rs:493-506`). Reset:
//!   increase, auto-reload, divider 1 → APB/2 = **40 MHz** until
//!   configured, XTAL/2 = **20 MHz** after.
//! - `now()`: write `update` (bit 31), poll it until 0, read `lo` then `hi`
//!   (`timg.rs:474-486`). `update` is a pulse; the value is latched.
//! - `schedule(timeout)`: `stop`, `clear_interrupt`, `reset` (`loadlo/hi =
//!   0`, `load = 1`), `autoreload = 0`, `alarmlo/hi = ticks`, `start`.
//! - The alarm fires when the count reaches `alarm`; `alarm_en` clears
//!   itself ("automatically cleared once an alarm occurs", the PAC's field
//!   doc) and `int_raw.tN` sets. The source `TG0_T0_LEVEL` (50) /
//!   `TG0_T1_LEVEL` (51) / `TG0_WDT_LEVEL` (52), and `TG1`'s 53/54/55, is
//!   the level `int_raw & int_ena` — `int_ena` gates it on this chip as on
//!   the C6 (`timg.rs:520-536`: only the esp32/S2 arm uses `level_int_en`).
//! - Decrementing mode (`increase = 0`) is not modelled: esp-hal sets
//!   `increase = 1` in `start`. A write that clears it is logged once.
//!
//! # RTC calibration (`clock/mod.rs`, `measure_rtc_clock`)
//!
//! `rtccalicfg.rtc_cali_start` (bit 31) starts a measurement of
//! `rtc_cali_max` (bits 16:30) slow-clock cycles; `rtc_cali_rdy` (bit 15)
//! reads 1 when it is done and `rtccalicfg1.rtc_cali_value` (bits 7:31)
//! holds the XTAL cycles counted. **On this chip the boot runs it**:
//! `Clocks::init` → `calibrate_rtc_slow_clock` (`clock/mod.rs:481-535`,
//! under `soc_has_clock_node_timg_calibration_clock`, which the S3's tree
//! has) measures 1024 cycles of `RC_SLOW` and stores the period in
//! `RTC_CNTL.store1` — the number the RWDT's `set_timeout` divides by. The
//! value is **modeled** as `XTAL_HZ * max / RC_SLOW_HZ` (`40_000_000 * 1024 /
//! 136_000 ≈ 301_176`), delivered after `max / RC_SLOW_HZ` of emulated time;
//! `rtccalicfg` resets to `0x0001_3000` (`start_cycling` set, `clk_sel =
//! 1`, `max = 1`) and a cycling calibration is modelled as already complete.
//! `rtccalicfg2.rtc_cali_timeout` reads 0.
//!
//! # MWDT
//!
//! `wdtconfig0..5` and `wdtfeed` take writes only while `wdtwprotect` holds
//! [`super::WDT_WKEY`]; `esp_hal::init` unlocks, clears `wdt_en`, locks, for
//! both groups (`lib.rs:757-761`). Expiry is **not** modelled: a write that
//! sets `wdt_en` puts a `WDT ARMED` note in the trace so an image that does
//! arm it is visible. **`wdtwprotect` resets to the write key** (PAC), so
//! the MWDT is *unlocked* out of reset — the part's surprise, the C6's and
//! the classic's too.

use lp_emu_core::sched::EventId;
use lp_emu_esp_common::engine::timg::{
    CounterConfig, TickRate, TimgEngine, TimgEventIds, WdtWrite, wdt_write,
};
use lp_emu_esp_common::regfile::{lane_of, merge_lane};
use lp_emu_esp_common::{BusCx, Peripheral, RegFile, Width, event_id, event_local};

use super::systimer::Reader;
use super::{APB_HZ, RC_SLOW_HZ, WDT_WKEY, XTAL_HZ};
use crate::memmap;
use crate::regs::{self, source};

/// The block's aperture: the generated table runs to `+0xfc` (`regclk`).
pub const TIMG_LEN: u32 = 0x100;

/// The S3's timer groups have **two** counters (`regs::TIMG0`: `t0.*` at
/// `+0x00..+0x24`, `t1.*` at `+0x24..+0x48`). The C6 has one, the classic
/// two plus LACT.
pub const COUNTERS: usize = 2;
/// One counter's register cluster, `0x24` bytes.
const T_STRIDE: u32 = 0x24;
// Inside a cluster.
const T_CONFIG: u32 = 0x00;
const T_LO: u32 = 0x04;
const T_HI: u32 = 0x08;
const T_UPDATE: u32 = 0x0c;
const T_ALARMLO: u32 = 0x10;
const T_ALARMHI: u32 = 0x14;
const T_LOADLO: u32 = 0x18;
const T_LOADHI: u32 = 0x1c;
const T_LOAD: u32 = 0x20;
/// One past the last counter's cluster.
const T_END: u32 = T_STRIDE * COUNTERS as u32;
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

/// `config.use_xtal` — "1: Use XTAL_CLK as the source clock of timer
/// group. 0: Use APB_CLK" (`esp32s3-0.35.2/src/timg0/t/config.rs`).
const CFG_USE_XTAL: u32 = 1 << 9;
const CFG_ALARM_EN: u32 = 1 << 10;
const CFG_DIVCNT_RST: u32 = 1 << 12;
const CFG_DIVIDER_SHIFT: u32 = 13;
const CFG_DIVIDER_MASK: u32 = 0xffff;
const CFG_AUTORELOAD: u32 = 1 << 29;
const CFG_INCREASE: u32 = 1 << 30;
const CFG_EN: u32 = 1 << 31;
const UPDATE_BIT: u32 = 1 << 31;
const COUNTER_MASK: u64 = (1 << 54) - 1;

const WDT_EN: u32 = 1 << 31;

const CALI_RDY: u32 = 1 << 15;
const CALI_START_CYCLING: u32 = 1 << 12;
const CALI_MAX_SHIFT: u32 = 16;
const CALI_MAX_MASK: u32 = 0x7fff;
const CALI_START: u32 = 1 << 31;
const CALI2_TIMEOUT: u32 = 1;

/// Event ids: counter `i`'s alarm is local id `i`; the calibration is one
/// past the last counter.
const EV_CALI: u16 = COUNTERS as u16;

/// One timer group: **a view over
/// [`TimgEngine`](lp_emu_esp_common::engine::timg::TimgEngine)**.
///
/// The counters, their alarms and the watchdog gate are behaviour and live
/// in the engine. What lives here is everything that would be wrong on
/// another part: the offsets, the bit positions, the `RegFile` and its PAC
/// reset values, the XTAL/APB and CPU rates, the 54-bit counter width, the
/// interrupt source numbers, the `EventId` packing — and the RTC
/// calibration, which is generic in shape but two chip clock rates in
/// content.
#[derive(Debug)]
pub struct Timg {
    name: &'static str,
    index: usize,
    /// Every register's stored value, and the block's names.
    regs: RegFile,
    /// The counters and their alarms.
    engine: TimgEngine,
    cali_rdy: bool,
    cali_value: u32,
    /// `TGn_T0_LEVEL`, `TGn_T1_LEVEL`, `TGn_WDT_LEVEL`.
    sources: [u16; 3],
    warned_decrement: bool,
}

impl Timg {
    fn new(name: &'static str, sources: [u16; 3]) -> Self {
        Self {
            name,
            index: 0,
            regs: RegFile::new(name, TIMG_LEN).with_names(regs::TIMG0),
            engine: TimgEngine::new(COUNTERS),
            // Reset: `start_cycling` set → a cycling calibration that has
            // already completed once (modeled).
            cali_rdy: true,
            cali_value: cali_value_for(1),
            sources,
            warned_decrement: false,
        }
    }

    pub fn timg0() -> Self {
        Self::new(
            "TIMG0",
            [
                source::TG0_T0_LEVEL,
                source::TG0_T1_LEVEL,
                source::TG0_WDT_LEVEL,
            ],
        )
    }

    pub fn timg1() -> Self {
        Self::new(
            "TIMG1",
            [
                source::TG1_T0_LEVEL,
                source::TG1_T1_LEVEL,
                source::TG1_WDT_LEVEL,
            ],
        )
    }

    /// Which counter an offset inside the clusters belongs to, and the
    /// register inside that cluster.
    fn cluster(off: u32) -> Option<(usize, u32)> {
        (off < T_END).then(|| ((off / T_STRIDE) as usize, off % T_STRIDE))
    }

    fn t(i: usize, reg: u32) -> u32 {
        T_STRIDE * i as u32 + reg
    }

    fn config(&self, i: usize) -> u32 {
        self.regs.stored(Self::t(i, T_CONFIG))
    }

    /// esp-hal's reading of the prescaler (`timg.rs:493-506`).
    fn divider(&self, i: usize) -> u64 {
        match (self.config(i) >> CFG_DIVIDER_SHIFT) & CFG_DIVIDER_MASK {
            0 => 65536,
            1 | 2 => 2,
            n => u64::from(n),
        }
    }

    /// The counter's source clock, read out of its own `use_xtal` bit.
    fn source_hz(&self, i: usize) -> u64 {
        if self.config(i) & CFG_USE_XTAL != 0 {
            XTAL_HZ
        } else {
            APB_HZ
        }
    }

    /// The scheduler ids this block has assigned to counter `i`'s events.
    /// The engine never packs one: it does not know the peripheral index.
    fn ids(&self, i: usize) -> TimgEventIds {
        TimgEventIds {
            alarm: event_id(self.index, i as u16),
        }
    }

    /// Everything the engine needs about counter `i`, read out of this
    /// chip's own registers. Cheap enough to build unconditionally.
    ///
    /// **The rate.** The counter counts source-clock ticks at `divider`
    /// against a CPU at `memmap::CPU_HZ`, so `numer / denom` is
    /// `source_hz / (divider × CPU_HZ)`; `divider × CPU_HZ` is at most
    /// `65536 × 240_000_000 ≈ 2^43.8`, so folding it into a `u64` `denom`
    /// cannot overflow.
    fn counter_config(&self, i: usize) -> CounterConfig {
        let cfg = self.config(i);
        CounterConfig {
            enabled: cfg & CFG_EN != 0,
            alarm_enabled: cfg & CFG_ALARM_EN != 0,
            auto_reload: cfg & CFG_AUTORELOAD != 0,
            rate: TickRate {
                numer: self.source_hz(i),
                denom: self.divider(i) * memmap::CPU_HZ,
            },
            mask: COUNTER_MASK,
            alarm: self.alarm(i),
            load_value: self.load_value(i),
        }
    }

    /// Counter `i`'s count at `now`.
    pub fn count(&self, i: usize, now: u64) -> u64 {
        self.engine.count(i, &self.counter_config(i), now)
    }

    /// What a read of `tN.lo`/`tN.hi` returns: the value the last `update`
    /// pulse latched.
    pub fn latched(&self, i: usize) -> u64 {
        self.engine.latched(i)
    }

    fn alarm(&self, i: usize) -> u64 {
        ((u64::from(self.regs.stored(Self::t(i, T_ALARMHI))) << 32)
            | u64::from(self.regs.stored(Self::t(i, T_ALARMLO))))
            & COUNTER_MASK
    }

    fn load_value(&self, i: usize) -> u64 {
        ((u64::from(self.regs.stored(Self::t(i, T_LOADHI))) << 32)
            | u64::from(self.regs.stored(Self::t(i, T_LOADLO))))
            & COUNTER_MASK
    }

    fn update_lines(&self, cx: &mut BusCx<'_>) {
        let st = self.regs.stored(INT_RAW) & self.regs.stored(INT_ENA);
        for (bit, src) in self.sources.iter().enumerate() {
            cx.irq.set_level(*src, st & (1 << bit) != 0);
        }
    }

    fn rearm(&mut self, i: usize, cx: &mut BusCx<'_>) {
        let cfg = self.counter_config(i);
        let ids = self.ids(i);
        self.engine.rearm(i, &cfg, ids, cx);
    }

    fn write_config(&mut self, i: usize, value: u32, cx: &mut BusCx<'_>) {
        let old = self.config(i);
        let mut new = value;
        if new & CFG_INCREASE == 0 && !self.warned_decrement {
            self.warned_decrement = true;
            log::warn!(
                "{}: t{i}.config.increase cleared; decrementing mode is not modelled",
                self.name
            );
        }
        const RATE_BITS: u32 = CFG_USE_XTAL | (CFG_DIVIDER_MASK << CFG_DIVIDER_SHIFT);
        let before = self.counter_config(i);
        if (old ^ new) & CFG_EN != 0 {
            // The engine freezes against the configuration as it stood
            // *before* this write — its divider and clock included — because
            // that is the rate the count it is freezing was produced at.
            self.engine
                .set_enabled(i, &before, new & CFG_EN != 0, cx.now);
        } else if before.enabled && (old ^ new) & RATE_BITS != 0 {
            // A rate change on a running counter: the count so far was
            // produced at the old rate, so freeze it there and restart the
            // base at `now`, rather than re-pricing the whole elapsed span
            // at the new one. esp-hal only rewrites `use_xtal` on a stopped
            // counter, so the boot never takes this arm; it is here so the
            // model is right rather than merely sufficient.
            self.engine.set_enabled(i, &before, false, cx.now);
            let mut frozen = before;
            frozen.enabled = false;
            self.engine.set_enabled(i, &frozen, true, cx.now);
        }
        // The divider-counter reset is a pulse.
        new &= !CFG_DIVCNT_RST;
        self.regs.poke(Self::t(i, T_CONFIG), new);
        self.rearm(i, cx);
    }

    fn wdt_unlocked(&self) -> bool {
        self.regs.stored(WDTWPROTECT) == WDT_WKEY
    }

    fn read_word(&self, off: u32) -> u32 {
        if let Some((i, reg)) = Self::cluster(off) {
            return match reg {
                T_LO => self.latched(i) as u32,
                T_HI => (self.latched(i) >> 32) as u32,
                T_UPDATE | T_LOAD => 0,
                _ => self.regs.stored(off),
            };
        }
        match off {
            WDTFEED | INT_CLR => 0,
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
        if let Some((i, reg)) = Self::cluster(off) {
            match reg {
                T_CONFIG => self.write_config(i, value, cx),
                T_UPDATE => {
                    if value & UPDATE_BIT != 0 {
                        let cfg = self.counter_config(i);
                        self.engine.latch(i, &cfg, cx.now);
                    }
                }
                T_ALARMLO | T_ALARMHI => {
                    self.regs.poke(off, value);
                    self.rearm(i, cx);
                }
                T_LOADLO | T_LOADHI => self.regs.poke(off, value),
                T_LOAD => {
                    if value & 1 != 0 {
                        let cfg = self.counter_config(i);
                        self.engine.load(i, &cfg, cx.now);
                        self.rearm(i, cx);
                    }
                }
                // `lo`/`hi`: read-only.
                _ => {}
            }
            return;
        }
        match off {
            WDTCONFIG0..=WDTFEED => {
                // The enable bit is `wdtconfig0`'s alone; the gate is asked
                // about an arming edge only when that is the register being
                // written.
                let cfg0 = off == WDTCONFIG0;
                let old = self.regs.stored(off);
                let verdict = wdt_write(
                    self.wdt_unlocked(),
                    cfg0 && old & WDT_EN != 0,
                    cfg0 && value & WDT_EN != 0,
                );
                if verdict == WdtWrite::Locked {
                    log::debug!("{}: write to +{off:#05x} dropped, WDT locked", self.name);
                    return;
                }
                if off == WDTFEED {
                    return;
                }
                self.regs.poke(off, value);
                if verdict == WdtWrite::ArmedNow {
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
                    // ready about.
                    self.cali_rdy = false;
                    cx.sched.cancel(ev);
                }
            }
            RTCCALICFG1 => {}
            INT_ENA => {
                self.regs.poke(INT_ENA, value & 0b111);
                self.update_lines(cx);
            }
            INT_RAW | INT_ST => {}
            INT_CLR => {
                let raw = self.regs.stored(INT_RAW) & !(value & 0b111);
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
        let local = event_local(id);
        if local == EV_CALI {
            let max = (self.regs.stored(RTCCALICFG) >> CALI_MAX_SHIFT) & CALI_MAX_MASK;
            self.cali_value = cali_value_for(max.max(1));
            self.cali_rdy = true;
            return;
        }
        let i = usize::from(local);
        if i >= COUNTERS {
            return;
        }
        let cfg = self.config(i);
        let counter = self.counter_config(i);
        // The engine's verdict — is the alarm really due, or was the
        // compare moved out since this event was scheduled? — plus the
        // auto-reload, which is its state. The register work below is this
        // view's.
        if !self.engine.on_alarm(i, &counter, cx.now) {
            return;
        }
        // "Automatically cleared once an alarm occurs" (the PAC's field
        // doc).
        self.regs.poke(Self::t(i, T_CONFIG), cfg & !CFG_ALARM_EN);
        self.regs
            .poke(INT_RAW, self.regs.stored(INT_RAW) | (1 << i));
        self.update_lines(cx);
    }

    fn reg_name(&self, off: u32) -> Option<&'static str> {
        regs::TIMG0.name(off)
    }

    fn save_state(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(TIMG_LEN as usize + 80);
        out.extend_from_slice(&(self.index as u64).to_le_bytes());
        self.engine.save(&mut out);
        out.extend_from_slice(&u32::from(self.cali_rdy).to_le_bytes());
        out.extend_from_slice(&self.cali_value.to_le_bytes());
        out.extend_from_slice(&u32::from(self.warned_decrement).to_le_bytes());
        out.extend_from_slice(&self.regs.save_state());
        out
    }

    fn load_state(&mut self, bytes: &[u8]) {
        let mut r = Reader(bytes);
        let Some(index) = r.u64() else {
            log::warn!("{}: load_state blob too short, ignored", self.name);
            return;
        };
        let Some(used) = self.engine.load_state(r.0) else {
            log::warn!("{}: load_state blob too short, ignored", self.name);
            return;
        };
        r.0 = &r.0[used..];
        self.index = index as usize;
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

    /// The PAC's reset for one register of this block, from the generated
    /// table — the same value `with_names` seeded, so a test can never
    /// drift from the model by carrying its own copy.
    fn pac(off: u32) -> u32 {
        regs::TIMG0
            .reset(off)
            .expect("the PAC gives this register a non-zero reset")
    }

    /// esp-hal's `now()` (`timg.rs:474-486`) on counter `i`.
    fn now(sb: &mut Sandbox, t: &mut Timg, i: usize) -> u64 {
        sb.write(t, Timg::t(i, T_UPDATE), UPDATE_BIT);
        assert_eq!(
            sb.read(t, Timg::t(i, T_UPDATE)) & UPDATE_BIT,
            0,
            "update is a pulse"
        );
        let lo = sb.read(t, Timg::t(i, T_LO));
        let hi = sb.read(t, Timg::t(i, T_HI));
        (u64::from(hi) << 32) | u64::from(lo)
    }

    /// esp-hal's `OneShotTimer::schedule` on a TIMG timer, register for
    /// register (`timer/mod.rs:274-286` + `timg.rs:225-234`).
    fn schedule(sb: &mut Sandbox, t: &mut Timg, i: usize, ticks: u64) {
        let c = |r| Timg::t(i, r);
        let cfg = sb.read(t, c(T_CONFIG));
        sb.write(t, c(T_CONFIG), cfg & !CFG_EN); // stop
        sb.write(t, INT_CLR, 1 << i); // clear_interrupt
        let cfg = sb.read(t, c(T_CONFIG));
        sb.write(t, c(T_CONFIG), cfg & !CFG_ALARM_EN);
        sb.write(t, c(T_LOADLO), 0); // reset
        sb.write(t, c(T_LOADHI), 0);
        sb.write(t, c(T_LOAD), 1);
        let cfg = sb.read(t, c(T_CONFIG));
        sb.write(t, c(T_CONFIG), cfg & !CFG_AUTORELOAD);
        sb.write(t, c(T_ALARMLO), ticks as u32);
        sb.write(t, c(T_ALARMHI), (ticks >> 32) as u32);
        // start
        let cfg = sb.read(t, c(T_CONFIG));
        sb.write(t, c(T_CONFIG), cfg & !CFG_EN);
        let cfg = sb.read(t, c(T_CONFIG));
        sb.write(t, c(T_CONFIG), cfg & !CFG_ALARM_EN);
        sb.write(t, c(T_LOADLO), 0);
        sb.write(t, c(T_LOADHI), 0);
        sb.write(t, c(T_LOAD), 1);
        let cfg = sb.read(t, c(T_CONFIG));
        sb.write(t, c(T_CONFIG), cfg | CFG_INCREASE);
        let cfg = sb.read(t, c(T_CONFIG));
        sb.write(t, c(T_CONFIG), cfg | CFG_EN);
        let cfg = sb.read(t, c(T_CONFIG));
        sb.write(t, c(T_CONFIG), cfg | CFG_ALARM_EN);
    }

    /// The source clock is the register's, not an assumption: APB/2 =
    /// 40 MHz at the PAC reset, XTAL/2 = 20 MHz once esp-hal's function
    /// clock has set `use_xtal` on the counter.
    #[test]
    fn t0_counts_apb_at_reset_and_xtal_once_use_xtal_is_set() {
        let mut sb = Sandbox::new();
        let mut t = Timg::timg0();
        assert_eq!(sb.read(&mut t, T_CONFIG), pac(T_CONFIG));
        assert_eq!(pac(T_CONFIG) & CFG_USE_XTAL, 0, "APB at reset");
        sb.now = 2_400_000;
        assert_eq!(now(&mut sb, &mut t, 0), 0, "disabled at reset: holds 0");
        sb.write(&mut t, T_CONFIG, pac(T_CONFIG) | CFG_EN);
        sb.now = 4_800_000; // +10 ms = 400_000 ticks at 40 MHz
        assert_eq!(now(&mut sb, &mut t, 0), 400_000);
        // esp-hal's `configure_function_clock(XtalClk)`: `use_xtal` set on
        // the running counter. The count so far is kept; the rate changes.
        sb.write(&mut t, T_CONFIG, pac(T_CONFIG) | CFG_EN | CFG_USE_XTAL);
        sb.now = 7_200_000; // +10 ms = 200_000 ticks at 20 MHz
        assert_eq!(now(&mut sb, &mut t, 0), 600_000);
        // Stop freezes; restart continues.
        sb.write(&mut t, T_CONFIG, pac(T_CONFIG) | CFG_USE_XTAL);
        sb.now = 8_000_000;
        assert_eq!(now(&mut sb, &mut t, 0), 600_000);
        sb.write(&mut t, T_CONFIG, pac(T_CONFIG) | CFG_EN | CFG_USE_XTAL);
        sb.now = 8_000_012;
        assert_eq!(now(&mut sb, &mut t, 0), 600_001);
    }

    /// Two counters over one engine: T1 is at `+0x24`, counts on its own
    /// rate, raises its own source, and reloading T0 leaves it alone.
    #[test]
    fn t1_is_a_second_independent_counter_at_its_own_offset() {
        let mut sb = Sandbox::new();
        let mut t = Timg::timg0();
        t.attached(4);
        assert_eq!(t.reg_name(0x24), Some("t1.config"));
        assert_eq!(sb.read(&mut t, 0x24), pac(0x24));
        sb.now = 0;
        sb.write(&mut t, T_CONFIG, pac(T_CONFIG) | CFG_EN | CFG_USE_XTAL);
        // T1 at divider 40 on XTAL: 1 MHz.
        sb.write(
            &mut t,
            0x24,
            (40 << CFG_DIVIDER_SHIFT) | CFG_INCREASE | CFG_EN | CFG_USE_XTAL,
        );
        sb.now = memmap::CPU_HZ / 1_000;
        assert_eq!(now(&mut sb, &mut t, 0), 20_000);
        assert_eq!(now(&mut sb, &mut t, 1), 1_000);
        sb.write(&mut t, T_LOAD, 1);
        assert_eq!(now(&mut sb, &mut t, 0), 0);
        assert_eq!(
            now(&mut sb, &mut t, 1),
            1_000,
            "T1 untouched by T0's reload"
        );

        // T1's alarm raises TG0_T1_LEVEL, bit 1 of int_raw.
        sb.write(&mut t, INT_ENA, 0b10);
        schedule(&mut sb, &mut t, 1, 500); // 0.5 ms at 1 MHz
        let due = sb.sched.next_deadline().expect("scheduled");
        assert_eq!(due, sb.now + 500 * 240);
        sb.run_to(&mut t, due);
        assert!(sb.irq.level(source::TG0_T1_LEVEL));
        assert!(!sb.irq.level(source::TG0_T0_LEVEL));
        assert_eq!(sb.read(&mut t, INT_RAW), 0b10);
    }

    #[test]
    fn the_esp_rtos_tick_fires_ten_ms_after_schedule_and_clears_alarm_en() {
        let mut sb = Sandbox::new();
        let mut t = Timg::timg0();
        t.attached(4);
        // esp-hal's function clock first: XTAL on both counters.
        sb.write(&mut t, T_CONFIG, pac(T_CONFIG) | CFG_USE_XTAL);
        sb.write(&mut t, INT_ENA, 1);
        sb.now = 1_000_000;
        schedule(&mut sb, &mut t, 0, 200_000); // 10 ms at 20 MHz
        assert_eq!(sb.sched.next_deadline(), Some(1_000_000 + 2_400_000));
        sb.run_to(&mut t, 3_399_999);
        assert!(!sb.irq.level(source::TG0_T0_LEVEL));
        sb.run_to(&mut t, 3_400_000);
        assert!(sb.irq.level(source::TG0_T0_LEVEL));
        assert_eq!(sb.read(&mut t, INT_RAW), 1);
        assert_eq!(sb.read(&mut t, INT_ST), 1);
        assert_eq!(
            sb.read(&mut t, T_CONFIG) & CFG_ALARM_EN,
            0,
            "alarm_en clears itself"
        );
        // timer_tick_handler: clear_interrupt, then a fresh schedule.
        sb.write(&mut t, INT_CLR, 1);
        assert!(!sb.irq.level(source::TG0_T0_LEVEL));
        assert_eq!(sb.read(&mut t, INT_CLR), 0);
        schedule(&mut sb, &mut t, 0, 20_000); // 1 ms
        assert_eq!(sb.sched.next_deadline(), Some(3_400_000 + 240_000));
        assert_eq!(sb.sched.live(), 1, "the re-arm replaced, it did not add");
    }

    #[test]
    fn the_mwdt_is_write_protected_and_arming_it_leaves_a_note() {
        let buf = lp_emu_esp_common::trace::SharedBuffer::new();
        let mut sb = Sandbox::new();
        sb.trace = lp_emu_esp_common::Trace::to_sink(Box::new(buf.clone()));
        let mut t = Timg::timg1();
        assert_eq!(sb.read(&mut t, WDTCONFIG0), pac(WDTCONFIG0));
        assert_eq!(sb.read(&mut t, WDTWPROTECT), WDT_WKEY, "unlocked at reset");
        // esp_hal::init's disable: unlock, clear wdt_en, lock.
        sb.write(&mut t, WDTWPROTECT, WDT_WKEY);
        sb.write(&mut t, WDTCONFIG0, 0);
        sb.write(&mut t, WDTWPROTECT, 0);
        assert_eq!(sb.read(&mut t, WDTCONFIG0), 0);
        sb.write(&mut t, WDTCONFIG0, 0xffff_ffff);
        assert_eq!(sb.read(&mut t, WDTCONFIG0), 0, "locked now");
        assert!(buf.lines().is_empty());
        sb.write(&mut t, WDTWPROTECT, WDT_WKEY);
        sb.write(&mut t, WDTCONFIG0, WDT_EN);
        sb.write(&mut t, WDTFEED, 1);
        assert_eq!(sb.read(&mut t, WDTFEED), 0);
        assert_eq!(buf.lines().len(), 1);
        assert!(buf.lines()[0].contains("TIMG1 WDT ARMED"));
    }

    /// The calibration `calibrate_rtc_slow_clock` drives on this chip: the
    /// two esp-hal polls, and the value `store1` ends up holding.
    #[test]
    fn the_rtc_calibration_answers_the_two_esp_hal_polls() {
        let mut sb = Sandbox::new();
        let mut t = Timg::timg0();
        t.attached(2);
        let cfg = sb.read(&mut t, RTCCALICFG);
        assert!(cfg & CALI_START_CYCLING != 0);
        assert!(cfg & CALI_RDY != 0, "a cycling calibration has completed");
        sb.write(&mut t, RTCCALICFG2, 0);
        sb.write(&mut t, RTCCALICFG, cfg & !CALI_START);
        assert!(sb.read(&mut t, RTCCALICFG) & CALI_RDY != 0);
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
        sb.write(&mut t, T_CONFIG, pac(T_CONFIG) | CFG_EN);
        sb.write(&mut t, 0x24, pac(0x24) | CFG_EN | CFG_USE_XTAL);
        sb.now = 1_777;
        now(&mut sb, &mut t, 0);
        now(&mut sb, &mut t, 1);
        let blob = t.save_state();
        let mut other = Timg::timg0();
        other.load_state(&blob);
        for i in 0..COUNTERS {
            assert_eq!(other.count(i, sb.now), t.count(i, sb.now));
            assert_eq!(other.latched(i), t.latched(i));
        }
        assert_eq!(other.index, 9);
        assert_eq!(other.save_state(), blob);
    }
}
