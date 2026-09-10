//! `TIMG0` / `TIMG1` at `0x3FF5_F000` / `0x3FF6_0000` — **the classic's whole
//! clock**, as a view over
//! [`TimgEngine`](lp_emu_esp_common::engine::timg::TimgEngine).
//!
//! # Why TIMG0 is three things at once
//!
//! The classic ESP32 has **no SYSTIMER**. Every other part on this roadmap
//! has a dedicated 52- or 54-bit system counter that `Instant::now()` reads;
//! this one does not, so the single timer group ends up carrying three
//! unrelated jobs at the same time, and every later reader of this crate will
//! trip over that fact before anything else:
//!
//! | use | registers | evidence |
//! |---|---|---|
//! | the esp-rtos tick / alarm | TIMG0 `t0` (`+0x00..+0x20`) | `board/esp32v3/init.rs` hands `timg0.timer0` to `esp_rtos::start` |
//! | the 1 ms io pacer, Priority 1 | TIMG0 `t1` (`+0x24..+0x44`) | L0's `[INIT] I/O task spawned (uart0 921600 8N1, swi2 executor prio2, timg0t1 pacer 1ms)` |
//! | `Instant::now()` | TIMG0 **LACT** (`+0x70..+0x94`) | `esp-hal-1.1.1/src/time.rs:713-758`, the `#[cfg(esp32)]` `implem` |
//! | the RTC clock calibration | TIMG0 `rtccalicfg`/`rtccalicfg1` (`+0x68`) | `esp-hal-1.1.1/src/clock/mod.rs:276-473` |
//!
//! So a single block answers the scheduler's tick, the I/O pacer's 1 ms
//! deadline, every timestamp the firmware takes, and the measurement that
//! decides what the crystal is — four consumers with four different rates,
//! none of which can be modelled by remembering what was written. That is
//! why this is the phase's largest view and why the shared engine takes a
//! **count** of counters rather than assuming one.
//!
//! TIMG1 is the same layout with `t0` and `t1` and no LACT consumer; the
//! image only ever disables its watchdog (P3's ledger, stop A6).
//!
//! # The classic's register layout is not the C6's
//!
//! Generated from `regs/timg0.rs`, never retyped:
//!
//! - the `T` cluster is **two** timers, `0x00..0x48` (the C6 has one at
//!   `0x00..0x24`);
//! - `INT_ENA`/`INT_RAW`/`INT_ST`/`INT_CLR` are at **`0x98`/`0x9c`/`0xa0`/
//!   `0xa4`** (the C6 and S3 put them at `0x70..0x7c`) — where the C6 keeps
//!   its calibration registers;
//! - `TCONFIG` carries `level_int_en` (bit 11) and `edge_int_en` (bit 12)
//!   where the C6's carries `divcnt_rst`;
//! - **LACT occupies `0x70..0x94`** with its own divider, alarm and load
//!   pair, and a fourth `int_raw` bit;
//! - `WDTCONFIG0..5`, `WDTFEED` and `WDTWPROTECT` sit at the same offsets on
//!   all three chips — the one place a copy would have been safe, and still
//!   generated rather than copied.
//!
//! # The rates, derived
//!
//! **`t0`/`t1` count APB ticks through a 16-bit prescaler.** The prescaler's
//! reading is the ESP32 TRM's, quoted verbatim inside esp-hal
//! (`timer/timg.rs:496-506`, "11.2.1 16-bit Prescaler and Clock Selection"):
//! 0 → 65536, 1 or 2 → 2, any other value → itself. There is no clock select
//! on this chip's `tconfig`; APB is the source.
//!
//! **LACT counts the same APB tick through its own divider**, and esp-hal
//! programs it to make a 16 MHz counter it then divides by 16 to get
//! microseconds:
//!
//! ```text
//! // esp-hal-1.1.1/src/time.rs:713-735, `#[cfg(esp32)] time_init`
//! let apb = crate::Clocks::get().apb_clock.as_hz();      // 80_000_000
//! tg0.lactconfig().write(|w| unsafe { w.bits(0) });
//! tg0.lactalarmhi/lo = u32::MAX;  tg0.lactload().load = 1;
//! tg0.lactconfig().write(|w| {
//!     w.divider().bits((apb / 16_000_000u32) as u16);    // 5
//!     w.increase(true); w.autoreload(true); w.en(true)
//! });
//! // …and `now()` divides the 64-bit count by 16 to get microseconds.
//! ```
//!
//! `80 MHz / 5 = 16 MHz`, `16 ticks = 1 µs`, and this view's tick rate is
//! `APB_HZ / (divider × CPU_HZ)` in the engine's exact-rational form — the
//! same expression the C6's view folds, with the classic's numbers.
//!
//! ⚠️ **`lactconfig.rtc_only` (bit 7) is not modelled.** Set, it would make
//! LACT count the RTC slow clock with `lactrtc`'s step instead of the APB
//! tick. The shipped image clears it (`lactconfig().write(bits(0))` above),
//! so the derivation holds for every run this machine has; a guest that sets
//! it gets one warning and a trace note rather than a silently wrong clock.
//!
//! ⚠️ **PD9: no host gate runs on emulated microseconds.** `Instant::now()`
//! is the guest reading a modelled counter. It is self-consistent and
//! deterministic; it is not a wall clock and nothing outside the guest may
//! treat it as one.
//!
//! # The APB rate is a stated constant, not a tracked node
//!
//! [`super::APB_HZ`] is 80 MHz — what `esp_hal::init` leaves the tree at and
//! the only rate the shipped image runs at. Before the PLL comes up APB is
//! the crystal, and this model does not follow that transition; nothing in
//! the window before `Clocks::init` programs a TIMG timer (P3's ledger: the
//! first TIMG access of the direct load is the calibration at cycle 109,989,
//! inside `Clocks::init` itself). Stated here because it is a *modeled*
//! simplification and not a measurement.
//!
//! # The RTC calibration — P3's finding 1, answered
//!
//! `rtccalicfg` starts a measurement of `rtc_cali_max` (bits 16:30) cycles
//! of the calibration clock `rtc_cali_clk_sel` (bits 13:14) picks, counting
//! **XTAL** cycles; `rtc_cali_rdy` (bit 15) says it is done and
//! `rtccalicfg1.rtc_cali_value` (bits 7:31) holds the count.
//!
//! With no model, P3's run had `rdy` never set: the firmware's `#[cfg(esp32)]`
//! timeout arm (`clock/mod.rs:433-440`) answered **0** twice,
//! `detect_xtal_freq` computed 0 MHz and picked `XtalClkConfig::_26` on a
//! board with a 40 MHz crystal, and `cal_val = 0` went into
//! `RTC_CNTL.store1`. This view computes the measurement instead:
//!
//! ```text
//! value  = XTAL_HZ × max / calibration_clock_hz
//! ready  = max cycles of the calibration clock later, counted in crystal
//!          cycles: max × XTAL_HZ / calibration_clock_hz guest cycles
//! ```
//!
//! (Why the crystal and not `CPU_HZ` for the *duration* — a real
//! consequence of this machine having one CPU rate where the guest has two —
//! is written out on [`Timg::cali_cycles`]. The *value* does not depend on
//! it.)
//!
//! Every number in it is a modelled clock, not a value chosen to make a boot
//! proceed:
//!
//! | name | Hz | source |
//! |---|---:|---|
//! | XTAL | 40 000 000 | the desk board's crystal (`../bench.md`) |
//! | RC_SLOW (`clk_sel` 0) | 150 000 | `esp-metadata-generated-0.4.0` `_generated_esp32.rs` `rc_slow_clk_frequency` |
//! | RC_FAST/256 (`clk_sel` 1) | 31 250 | same file: `rc_fast_clk_frequency` 8 MHz, `rc_fast_div_clk_frequency` = `/256` |
//! | XTAL32K (`clk_sel` 2) | 32 768 | same file: `xtal32k_clk_frequency` |
//!
//! and the `clk_sel` encoding is esp-hal's own
//! (`soc/esp32/clocks.rs:730-733`: `RcSlowClk => 0, RcFastDivClk => 1,
//! Xtal32kClk => 2`). `detect_xtal_freq` then computes
//! `31 250 × (40e6 × 10 / 31 250) / 10 = 40 MHz` and takes the 40 MHz arm.
//!
//! # `int_ena` does not gate an interrupt on this chip
//!
//! esp-hal says so in as many words (`timer/timg.rs:517-526`):
//!
//! > On ESP32 and S2, the `int_ena` register is ineffective - interrupts fire
//! > even without int_ena enabling them. We use level interrupts so that we
//! > have a status bit available.
//!
//! — and its `set_interrupt_enabled` writes `tconfig.level_int_en` instead.
//! So this view drives a timer's **level** interrupt source from
//! `int_raw & level_int_en`, and keeps `int_st = int_raw & int_ena` as the
//! PAC defines it. `edge_int_en` selects a different set of source numbers
//! (`TG0_LACT_EDGE` is 61, not 17) and is **not** modelled; setting it earns
//! one warning.
//!
//! # The MWDTs
//!
//! `esp_hal::init` disables both groups' watchdogs unconditionally
//! (`lp-fw/fw-esp32v3/src/board/esp32v3/init.rs:47-50`). The write-protect
//! key is honoured through the engine's [`wdt_write`] gate, so a
//! `WDTCONFIG0` write with the wrong key is *dropped* — "the disable
//! silently did nothing" is exactly the failure a lenient model hides.
//! Expiry is not modelled anywhere in the engine; an arming edge leaves a
//! trace note.

use lp_emu_core::sched::EventId;
use lp_emu_esp_common::engine::timg::{
    CounterConfig, TickRate, TimgEngine, TimgEventIds, WdtWrite, wdt_write,
};
use lp_emu_esp_common::regfile::{lane_of, merge_lane};
use lp_emu_esp_common::{BusCx, Peripheral, RegFile, Width, event_id, event_local};

use super::{APB_HZ, RC_FAST_DIV_HZ, RC_SLOW_HZ, WDT_WKEY, XTAL_HZ, XTAL32K_HZ};
use crate::memmap;
use crate::regs;

/// A timer group's aperture, tight: the generated table runs to `+0xfc`
/// (`timgclk`); the PAC gives TIMG0 and TIMG1 one `RegisterBlock`, so one
/// length and one table serve both.
pub const TIMG_LEN: u32 = 0x100;

// The `T` cluster: two timers, nine words each (`regs::TIMG0`, `t0.*`/`t1.*`).
const T_STRIDE: u32 = 0x24;
const T0_BASE: u32 = 0x00;
const T1_BASE: u32 = 0x24;
const T_CONFIG: u32 = 0x00;
const T_LO: u32 = 0x04;
const T_HI: u32 = 0x08;
const T_UPDATE: u32 = 0x0c;
const T_ALARMLO: u32 = 0x10;
const T_ALARMHI: u32 = 0x14;
const T_LOADLO: u32 = 0x18;
const T_LOADHI: u32 = 0x1c;
const T_LOAD: u32 = 0x20;

// The MWDT.
const WDTCONFIG0: u32 = 0x48;
const WDTFEED: u32 = 0x60;
const WDTWPROTECT: u32 = 0x64;

// The RTC calibration.
const RTCCALICFG: u32 = 0x68;
const RTCCALICFG1: u32 = 0x6c;

// LACT — the classic's `Instant::now()`.
const LACTCONFIG: u32 = 0x70;
const LACTLO: u32 = 0x78;
const LACTHI: u32 = 0x7c;
const LACTUPDATE: u32 = 0x80;
const LACTALARMLO: u32 = 0x84;
const LACTALARMHI: u32 = 0x88;
const LACTLOADLO: u32 = 0x8c;
const LACTLOADHI: u32 = 0x90;
const LACTLOAD: u32 = 0x94;

// The interrupt registers, where the C6 keeps its calibration.
const INT_ENA: u32 = 0x98;
const INT_RAW: u32 = 0x9c;
const INT_ST: u32 = 0xa0;
const INT_CLR: u32 = 0xa4;

/// `t0config`/`t1config` and `lactconfig` bit positions
/// (`esp32-0.40.2/src/timg0/t/config.rs`, `.../lactconfig.rs`).
const CFG_RTC_ONLY: u32 = 1 << 7; // lactconfig only
const CFG_ALARM_EN: u32 = 1 << 10;
const CFG_LEVEL_INT_EN: u32 = 1 << 11;
const CFG_EDGE_INT_EN: u32 = 1 << 12;
const CFG_DIVIDER_SHIFT: u32 = 13;
const CFG_DIVIDER_MASK: u32 = 0xffff;
const CFG_AUTORELOAD: u32 = 1 << 29;
const CFG_INCREASE: u32 = 1 << 30;
const CFG_EN: u32 = 1 << 31;

/// `t*update` / `lactupdate` latch pulses: the PAC's `update` field is bits
/// 0:31 on the classic (a write of anything latches; esp-hal writes 1).
const UPDATE_ANY: u32 = 0xffff_ffff;

/// The classic's timers and LACT are **64-bit**: `t.hi` and `lacthi` are
/// full 32-bit fields (`esp32-0.40.2/src/timg0/t/hi.rs`), unlike the C6's
/// 54-bit counter.
const COUNTER_MASK: u64 = u64::MAX;

const WDT_EN: u32 = 1 << 31;

// `rtccalicfg` (`esp32-0.40.2/src/timg0/rtccalicfg.rs`).
const CALI_START_CYCLING: u32 = 1 << 12;
const CALI_CLK_SEL_SHIFT: u32 = 13;
const CALI_CLK_SEL_MASK: u32 = 0b11;
const CALI_RDY: u32 = 1 << 15;
const CALI_MAX_SHIFT: u32 = 16;
const CALI_MAX_MASK: u32 = 0x7fff;
const CALI_START: u32 = 1 << 31;
/// `rtccalicfg1.rtc_cali_value` is bits 7:31.
const CALI_VALUE_SHIFT: u32 = 7;
const CALI_VALUE_MASK: u32 = 0x01ff_ffff;

/// `int_raw`/`int_ena`/`int_st`/`int_clr` bits
/// (`esp32-0.40.2/src/timg0/int_raw.rs`): t0, t1, wdt, lact.
const INT_T0: u32 = 1 << 0;
const INT_T1: u32 = 1 << 1;
const INT_WDT: u32 = 1 << 2;
const INT_LACT: u32 = 1 << 3;
const INT_ALL: u32 = INT_T0 | INT_T1 | INT_WDT | INT_LACT;

// Event ids, local to this peripheral.
const EV_ALARM_T0: u16 = 0;
const EV_ALARM_T1: u16 = 1;
const EV_ALARM_LACT: u16 = 2;
const EV_CALI: u16 = 3;

/// This group's counters, in the engine's `Vec`.
const T0: usize = 0;
const T1: usize = 1;
const LACT: usize = 2;

/// The peripheral interrupt source numbers this group drives, `(t0, t1, wdt,
/// lact)`, from the `esp32` PAC's `Interrupt` enum (`esp32-0.40.2/src/lib.rs`):
/// TIMG0 is 14..17 and TIMG1 is 18..21. The **edge** sources (58..61) are a
/// different set and are not modelled.
///
/// P4 owns the DPORT interrupt matrix these feed; until it lands the levels
/// are set and nothing reads them, which is the same shape the C6's view had
/// before its matrix existed.
#[derive(Copy, Clone, Debug)]
struct Sources {
    t0: u16,
    t1: u16,
    wdt: u16,
    lact: u16,
}

const TIMG0_SOURCES: Sources = Sources {
    t0: 14,
    t1: 15,
    wdt: 16,
    lact: 17,
};
const TIMG1_SOURCES: Sources = Sources {
    t0: 18,
    t1: 19,
    wdt: 20,
    lact: 21,
};

/// One timer group: a view over
/// [`TimgEngine`](lp_emu_esp_common::engine::timg::TimgEngine).
///
/// The counters, their alarms, the auto-reload and the watchdog gate are
/// behaviour and live in the engine. What lives here is everything that
/// would be wrong on another part: the offsets, the bit positions, the
/// `RegFile` and its PAC reset values, the APB/XTAL/RC rates, the 64-bit
/// counter width, the interrupt source numbers, the `EventId` packing — and
/// the RTC calibration, whose *content* is two chip clock rates even though
/// its shape is generic (the C6's view keeps its own for the same reason).
#[derive(Debug)]
pub struct Timg {
    name: &'static str,
    index: usize,
    regs: RegFile,
    engine: TimgEngine,
    sources: Sources,
    /// Does this group's LACT have a consumer? TIMG1's exists in silicon and
    /// this machine models it too — one code path, no `has_lact` flag — but
    /// the count is stated here because a reader will ask.
    cali_rdy: bool,
    cali_value: u32,
    warned_decrement: bool,
    warned_edge_int: bool,
    warned_rtc_only: bool,
}

impl Timg {
    fn new(name: &'static str, sources: Sources) -> Self {
        let regs = RegFile::new(name, TIMG_LEN)
            .with_names(regs::TIMG0)
            .with_pac_grades();
        Self {
            name,
            index: 0,
            regs,
            // Three counters: `t0`, `t1` and LACT. TIMG1 has the same
            // registers, so it gets the same three.
            engine: TimgEngine::new(3),
            sources,
            // `rtccalicfg` resets with `start_cycling` set and `max` = 1: a
            // cycling calibration that has already produced a result once.
            cali_rdy: true,
            cali_value: 0,
            warned_decrement: false,
            warned_edge_int: false,
            warned_rtc_only: false,
        }
    }

    pub fn timg0() -> Self {
        let mut t = Self::new("TIMG0", TIMG0_SOURCES);
        t.cali_value = t.cali_value_now();
        t
    }

    pub fn timg1() -> Self {
        let mut t = Self::new("TIMG1", TIMG1_SOURCES);
        t.cali_value = t.cali_value_now();
        t
    }

    /// The register offset of counter `i`'s `field`.
    fn t_reg(i: usize, field: u32) -> u32 {
        match i {
            T0 => T0_BASE + field,
            T1 => T1_BASE + field,
            _ => match field {
                T_CONFIG => LACTCONFIG,
                T_LO => LACTLO,
                T_HI => LACTHI,
                T_UPDATE => LACTUPDATE,
                T_ALARMLO => LACTALARMLO,
                T_ALARMHI => LACTALARMHI,
                T_LOADLO => LACTLOADLO,
                T_LOADHI => LACTLOADHI,
                _ => LACTLOAD,
            },
        }
    }

    fn config(&self, i: usize) -> u32 {
        self.regs.stored(Self::t_reg(i, T_CONFIG))
    }

    /// The ESP32 TRM's prescaler reading, as esp-hal quotes it
    /// (`timer/timg.rs:496-506`): 0 → 65536, 1 or 2 → 2, else the value.
    ///
    /// The same 16-bit `DIVIDER` field, at the same bits, in `t*config` and
    /// in `lactconfig` — one rule, cited once.
    fn divider(&self, i: usize) -> u64 {
        match (self.config(i) >> CFG_DIVIDER_SHIFT) & CFG_DIVIDER_MASK {
            0 => 65536,
            1 | 2 => 2,
            n => u64::from(n),
        }
    }

    fn pair(&self, lo: u32, hi: u32) -> u64 {
        (u64::from(self.regs.stored(hi)) << 32) | u64::from(self.regs.stored(lo))
    }

    /// Everything the engine needs about counter `i`, read out of this chip's
    /// own registers.
    ///
    /// **The rate.** Every counter in this block counts the APB tick through
    /// its own 16-bit prescaler, so `numer / denom` is
    /// `APB_HZ / (divider × CPU_HZ)`: the count is
    /// `delta × APB_HZ / (divider × CPU_HZ)` and the re-arm
    /// `(ticks × divider × CPU_HZ).div_ceil(APB_HZ)`, both in `u128`.
    /// `divider × CPU_HZ` is at most `65536 × 240_000_000 ≈ 2^43.9`, so
    /// folding it into a `u64` `denom` cannot overflow.
    fn counter_config(&self, i: usize) -> CounterConfig {
        let cfg = self.config(i);
        CounterConfig {
            enabled: cfg & CFG_EN != 0,
            alarm_enabled: cfg & CFG_ALARM_EN != 0,
            auto_reload: cfg & CFG_AUTORELOAD != 0,
            rate: TickRate {
                numer: APB_HZ,
                denom: self.divider(i) * memmap::CPU_HZ,
            },
            mask: COUNTER_MASK,
            alarm: self.pair(Self::t_reg(i, T_ALARMLO), Self::t_reg(i, T_ALARMHI)),
            load_value: self.pair(Self::t_reg(i, T_LOADLO), Self::t_reg(i, T_LOADHI)),
        }
    }

    /// Counter `i`'s live count at `now`.
    pub fn count(&self, i: usize, now: u64) -> u64 {
        self.engine.count(i, &self.counter_config(i), now)
    }

    /// What a read of counter `i`'s `lo`/`hi` pair returns: the value the
    /// last `update` pulse latched.
    pub fn latched(&self, i: usize) -> u64 {
        self.engine.latched(i)
    }

    /// LACT's count in microseconds, the way `Instant::now()` computes it
    /// (`time.rs:757`: `Instant::from_ticks(ticks / 16)`), for a test that
    /// wants to read the clock the guest reads.
    pub fn lact_micros(&self) -> u64 {
        self.latched(LACT) / 16
    }

    fn ids(&self, i: usize) -> TimgEventIds {
        let local = match i {
            T0 => EV_ALARM_T0,
            T1 => EV_ALARM_T1,
            _ => EV_ALARM_LACT,
        };
        TimgEventIds {
            alarm: event_id(self.index, local),
        }
    }

    fn rearm(&mut self, i: usize, cx: &mut BusCx<'_>) {
        let cfg = self.counter_config(i);
        let ids = self.ids(i);
        self.engine.rearm(i, &cfg, ids, cx);
    }

    /// `int_raw` bit for counter `i`.
    fn int_bit(i: usize) -> u32 {
        match i {
            T0 => INT_T0,
            T1 => INT_T1,
            _ => INT_LACT,
        }
    }

    /// Drive the four interrupt sources.
    ///
    /// `int_ena` does not gate an interrupt on this chip (see the module
    /// docs); the per-timer `level_int_en` does, and the watchdog — whose
    /// expiry is not modelled — has no such bit, so its line is `int_raw`
    /// alone.
    fn update_lines(&self, cx: &mut BusCx<'_>) {
        let raw = self.regs.stored(INT_RAW);
        let level = |i: usize, bit: u32| raw & bit != 0 && self.config(i) & CFG_LEVEL_INT_EN != 0;
        cx.irq.set_level(self.sources.t0, level(T0, INT_T0));
        cx.irq.set_level(self.sources.t1, level(T1, INT_T1));
        cx.irq.set_level(self.sources.lact, level(LACT, INT_LACT));
        cx.irq.set_level(self.sources.wdt, raw & INT_WDT != 0);
    }

    fn write_config(&mut self, i: usize, value: u32, cx: &mut BusCx<'_>) {
        let old = self.config(i);
        if value & CFG_INCREASE == 0 && value & CFG_EN != 0 && !self.warned_decrement {
            self.warned_decrement = true;
            log::warn!(
                "{}: counter {i} enabled with `increase` clear; decrementing mode is not modelled",
                self.name
            );
        }
        if value & CFG_EDGE_INT_EN != 0 && !self.warned_edge_int {
            self.warned_edge_int = true;
            let line = format!(
                "cyc={} pc=0x{:08x} {} counter {i} edge_int_en set (the edge interrupt sources \
                 58..61 are not modelled)",
                cx.now, cx.pc, self.name
            );
            cx.trace.note(&line);
            log::warn!("{}: edge_int_en is not modelled", self.name);
        }
        if i == LACT && value & CFG_RTC_ONLY != 0 && !self.warned_rtc_only {
            self.warned_rtc_only = true;
            let line = format!(
                "cyc={} pc=0x{:08x} {} lactconfig.rtc_only set (LACT counting the RTC slow \
                 clock through `lactrtc` is not modelled; it stays an APB counter)",
                cx.now, cx.pc, self.name
            );
            cx.trace.note(&line);
            log::warn!("{}: lactconfig.rtc_only is not modelled", self.name);
        }
        if (old ^ value) & CFG_EN != 0 {
            // The engine freezes against the configuration as it stood
            // *before* this write — its divider included — because that is
            // the rate the count it is freezing was produced at.
            let before = self.counter_config(i);
            self.engine
                .set_enabled(i, &before, value & CFG_EN != 0, cx.now);
        }
        self.regs.poke(Self::t_reg(i, T_CONFIG), value);
        self.rearm(i, cx);
        self.update_lines(cx);
    }

    fn wdt_unlocked(&self) -> bool {
        self.regs.stored(WDTWPROTECT) == WDT_WKEY
    }

    /// The calibration clock `rtc_cali_clk_sel` selects, in Hz.
    ///
    /// The encoding is esp-hal's (`soc/esp32/clocks.rs:730-733`); the rates
    /// are the generated clock tree's. A `clk_sel` of 3 is not a clock any
    /// source names — it answers RC_SLOW, the field's zero, and says so.
    fn cali_clock_hz(&self) -> u64 {
        match (self.regs.stored(RTCCALICFG) >> CALI_CLK_SEL_SHIFT) & CALI_CLK_SEL_MASK {
            0 => RC_SLOW_HZ,
            1 => RC_FAST_DIV_HZ,
            2 => XTAL32K_HZ,
            _ => {
                log::warn!(
                    "{}: rtc_cali_clk_sel = 3 is not a clock this part documents; answering \
                     RC_SLOW",
                    self.name
                );
                RC_SLOW_HZ
            }
        }
    }

    fn cali_max(&self) -> u64 {
        u64::from(((self.regs.stored(RTCCALICFG) >> CALI_MAX_SHIFT) & CALI_MAX_MASK).max(1))
    }

    /// The measurement the currently configured calibration would produce:
    /// XTAL cycles counted over `max` cycles of the calibration clock.
    fn cali_value_now(&self) -> u32 {
        (XTAL_HZ * self.cali_max() / self.cali_clock_hz()) as u32
    }

    /// How long that measurement takes, in guest cycles: `max` cycles of the
    /// calibration clock, counted in **crystal** cycles.
    ///
    /// ⚠️ **Why the crystal and not [`memmap::CPU_HZ`].** This machine has
    /// one guest-cycle rate for the whole run (`TimeGrade::T1`: cycles are
    /// instructions, 240 per emulated microsecond, i.e. a 240 MHz CPU) while
    /// the *guest* has two: `Clocks::init` runs the CPU on the crystal
    /// (`soc/esp32/clocks.rs:119`, `configure_cpu_clk(CpuClkConfig::Xtal)`)
    /// for the XTAL-detection calibration and switches to the 240 MHz PLL
    /// before the slow-clock one. Measured on the direct load: the
    /// `ets_delay_us(320)` that precedes the first poll spends **12,816**
    /// guest cycles — 40 ticks per microsecond, the crystal — not the 76,800
    /// a 240 MHz CPU would.
    ///
    /// So one duration is wrong for one of the two calibrations whatever it
    /// is expressed in, and the two errors are not symmetric:
    ///
    /// - **early** is unobservable: the driver delays and *then* polls
    ///   (`clock/mod.rs:404-407`, "otherwise the CPU may read back the
    ///   previous state of the completion flag"), and a `rdy` it finds
    ///   already set is the answer it was waiting for. The start write
    ///   clears `rdy`, so there is no stale flag to find.
    /// - **late** is very observable: the `#[cfg(esp32)]` timeout arm
    ///   answers **0**, `detect_xtal_freq` picks 26 MHz on a 40 MHz board
    ///   and `store1` gets a zero period — precisely the divergence P3
    ///   recorded (`§3.1` of the stop ledger) and this phase exists to
    ///   remove.
    ///
    /// The crystal is the lower of the two rates and therefore the early
    /// one. **The number the firmware consumes is not affected**: the
    /// measurement itself ([`Timg::cali_value_now`]) is a ratio of modelled
    /// clocks and does not depend on this at all.
    ///
    /// When a later phase gives the machine a CPU-rate-aware time base, this
    /// is the constant that should follow it.
    fn cali_cycles(&self) -> u64 {
        self.cali_max() * XTAL_HZ / self.cali_clock_hz()
    }

    fn read_word(&self, off: u32) -> u32 {
        // The `T` cluster's two timers, by counter index; LACT's own words
        // and the group registers fall through below.
        if off < T1_BASE + T_STRIDE {
            let i = if off < T1_BASE { T0 } else { T1 };
            return match off - if i == T0 { T0_BASE } else { T1_BASE } {
                T_LO => self.latched(i) as u32,
                T_HI => (self.latched(i) >> 32) as u32,
                // The pulses are write-only in the PAC; a read of one must
                // not look like a request that never finished.
                T_UPDATE | T_LOAD => 0,
                field => self.regs.stored(Self::t_reg(i, field)),
            };
        }
        match off {
            LACTLO => self.latched(LACT) as u32,
            LACTHI => (self.latched(LACT) >> 32) as u32,
            LACTUPDATE | LACTLOAD | WDTFEED | INT_CLR => 0,
            RTCCALICFG => {
                let v = self.regs.stored(RTCCALICFG) & !CALI_RDY;
                if self.cali_rdy { v | CALI_RDY } else { v }
            }
            RTCCALICFG1 => (self.cali_value & CALI_VALUE_MASK) << CALI_VALUE_SHIFT,
            INT_ST => self.regs.stored(INT_RAW) & self.regs.stored(INT_ENA),
            other => self.regs.stored(other),
        }
    }

    fn write_word(&mut self, off: u32, value: u32, cx: &mut BusCx<'_>) {
        // The `T` cluster, both timers, by counter index.
        if off < T1_BASE + T_STRIDE {
            let i = if off < T1_BASE { T0 } else { T1 };
            let field = off - if i == T0 { T0_BASE } else { T1_BASE };
            return self.write_counter(i, field, value, cx);
        }
        match off {
            LACTCONFIG => self.write_counter(LACT, T_CONFIG, value, cx),
            LACTUPDATE => self.write_counter(LACT, T_UPDATE, value, cx),
            LACTALARMLO => self.write_counter(LACT, T_ALARMLO, value, cx),
            LACTALARMHI => self.write_counter(LACT, T_ALARMHI, value, cx),
            LACTLOADLO => self.write_counter(LACT, T_LOADLO, value, cx),
            LACTLOADHI => self.write_counter(LACT, T_LOADHI, value, cx),
            LACTLOAD => self.write_counter(LACT, T_LOAD, value, cx),
            LACTLO | LACTHI => {}
            WDTCONFIG0..=WDTFEED => self.write_wdt(off, value, cx),
            RTCCALICFG => self.write_cali(value, cx),
            RTCCALICFG1 => {}
            INT_ENA => {
                self.regs.poke(INT_ENA, value & INT_ALL);
                self.update_lines(cx);
            }
            INT_RAW | INT_ST => {}
            INT_CLR => {
                let raw = self.regs.stored(INT_RAW) & !(value & INT_ALL);
                self.regs.poke(INT_RAW, raw);
                self.update_lines(cx);
            }
            other => self.regs.poke(other, value),
        }
    }

    fn write_counter(&mut self, i: usize, field: u32, value: u32, cx: &mut BusCx<'_>) {
        match field {
            T_CONFIG => self.write_config(i, value, cx),
            T_UPDATE => {
                if value & UPDATE_ANY != 0 {
                    let cfg = self.counter_config(i);
                    self.engine.latch(i, &cfg, cx.now);
                }
            }
            T_ALARMLO | T_ALARMHI => {
                self.regs.poke(Self::t_reg(i, field), value);
                self.rearm(i, cx);
            }
            T_LOADLO | T_LOADHI => self.regs.poke(Self::t_reg(i, field), value),
            T_LOAD => {
                if value != 0 {
                    let cfg = self.counter_config(i);
                    self.engine.load(i, &cfg, cx.now);
                    self.rearm(i, cx);
                }
            }
            // `lo`/`hi` are the controller's output.
            _ => {}
        }
    }

    fn write_wdt(&mut self, off: u32, value: u32, cx: &mut BusCx<'_>) {
        let cfg0 = off == WDTCONFIG0;
        let old = self.regs.stored(off);
        let verdict = wdt_write(
            self.wdt_unlocked(),
            cfg0 && old & WDT_EN != 0,
            cfg0 && value & WDT_EN != 0,
        );
        if verdict == WdtWrite::Locked {
            log::debug!("{}: write to +{off:#05x} dropped, MWDT locked", self.name);
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

    fn write_cali(&mut self, value: u32, cx: &mut BusCx<'_>) {
        let old = self.regs.stored(RTCCALICFG);
        self.regs.poke(RTCCALICFG, value & !CALI_RDY);
        let ev = event_id(self.index, EV_CALI);
        let running = CALI_START | CALI_START_CYCLING;
        if value & CALI_START != 0 && old & CALI_START == 0 {
            // A one-shot measurement starts: not ready until it ends.
            self.cali_rdy = false;
            cx.sched.cancel(ev);
            cx.sched.schedule_in(cx.now, self.cali_cycles(), ev);
        } else if value & running == 0 && old & running != 0 {
            // Neither one-shot nor cycling any more: nothing to be ready
            // about. Clearing `start` while `start_cycling` stays set leaves
            // the cycling result readable.
            self.cali_rdy = false;
            cx.sched.cancel(ev);
        }
    }
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
        let i = match event_local(id) {
            EV_ALARM_T0 => T0,
            EV_ALARM_T1 => T1,
            EV_ALARM_LACT => LACT,
            EV_CALI => {
                self.cali_value = self.cali_value_now();
                self.cali_rdy = true;
                return;
            }
            _ => return,
        };
        let cfg = self.config(i);
        let counter = self.counter_config(i);
        // The engine's verdict — is the alarm really due, or was the compare
        // moved out since this event was scheduled? — plus the auto-reload,
        // which is its state. The register work below is this view's.
        if !self.engine.on_alarm(i, &counter, cx.now) {
            return;
        }
        // "When set alarm is enabled" — the part clears it when the alarm
        // occurs, so the view does (the PAC's field doc; the C6's `t0` does
        // the same).
        self.regs
            .poke(Self::t_reg(i, T_CONFIG), cfg & !CFG_ALARM_EN);
        self.regs
            .poke(INT_RAW, self.regs.stored(INT_RAW) | Self::int_bit(i));
        self.update_lines(cx);
    }

    fn reg_name(&self, off: u32) -> Option<&'static str> {
        regs::TIMG0.name(off)
    }

    fn reg_grade(&self, off: u32) -> Option<lp_emu_esp_common::periph::RegGrade> {
        self.regs.reg_grade(off)
    }

    fn save_state(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(TIMG_LEN as usize + 96);
        out.extend_from_slice(&(self.index as u64).to_le_bytes());
        // Exactly where `base_ticks`, `base_cycle` and `latched` were written
        // before the engine held them.
        self.engine.save(&mut out);
        out.extend_from_slice(&u32::from(self.cali_rdy).to_le_bytes());
        out.extend_from_slice(&self.cali_value.to_le_bytes());
        out.extend_from_slice(&u32::from(self.warned_decrement).to_le_bytes());
        out.extend_from_slice(&u32::from(self.warned_edge_int).to_le_bytes());
        out.extend_from_slice(&u32::from(self.warned_rtc_only).to_le_bytes());
        out.extend_from_slice(&self.regs.save_state());
        out
    }

    fn load_state(&mut self, bytes: &[u8]) {
        let mut r = Reader(bytes);
        let Some(index) = r.u64() else {
            log::warn!("{}: load_state blob too short, ignored", self.name);
            return;
        };
        // The engine parses before it applies, so a short blob leaves it
        // untouched rather than half-loaded.
        let Some(used) = self.engine.load_state(r.0) else {
            log::warn!("{}: load_state blob too short, ignored", self.name);
            return;
        };
        r.0 = &r.0[used..];
        self.index = index as usize;
        self.cali_rdy = r.u32().unwrap_or(1) != 0;
        self.cali_value = r.u32().unwrap_or(0);
        self.warned_decrement = r.u32().unwrap_or(0) != 0;
        self.warned_edge_int = r.u32().unwrap_or(0) != 0;
        self.warned_rtc_only = r.u32().unwrap_or(0) != 0;
        self.regs.load_state(r.0);
    }
}

struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn u64(&mut self) -> Option<u64> {
        let (head, rest) = self.0.split_first_chunk::<8>()?;
        self.0 = rest;
        Some(u64::from_le_bytes(*head))
    }

    fn u32(&mut self) -> Option<u32> {
        let (head, rest) = self.0.split_first_chunk::<4>()?;
        self.0 = rest;
        Some(u32::from_le_bytes(*head))
    }
}
