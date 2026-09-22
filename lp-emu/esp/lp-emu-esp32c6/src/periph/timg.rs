//! `TIMG0` / `TIMG1` at `0x6000_8000` / `0x6000_9000` — timer T0 modelled,
//! the RTC calibration modelled, TIMG0's MWDT modelled (TIMG1's accepted).
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
//! (`lib.rs:751-761`).
//!
//! **`wdtwprotect` resets to the write key.** The PAC gives TIMG0's
//! `wdtwprotect` a reset of `0x50d8_3aa1` — the same value a driver
//! writes to unlock it — so the MWDT is *unlocked* out of reset, and the
//! first write to `wdtconfig0` takes without a key. LP_WDT's own
//! `wdtwprotect` has no non-zero reset and starts locked. Nothing on the
//! boot path depends on the difference (esp-hal writes the key first
//! either way); it is stated because the model used to hold both locked.
//!
//! ## Stage 0, modelled — **TIMG0 only**
//!
//! `wdtconfig0`'s fields (`esp32c6` PAC 0.23.2, cross-checked against the
//! ROM's own `wdt_hal_init` at `0x40020cba..0x40020d60`, where every field
//! write is followed by `|= 1 << 22`): `wdt_en` bit 31, `wdt_stg0` bits
//! 30:29 — **two** bits, `0: off, 1: interrupt, 2: reset CPU, 3: reset
//! system`, and the mask the ROM ANDs with is `0x9fff_ffff` — `stg1..3`
//! below it, `conf_update_en` bit 22, `wdt_use_xtal` bit 21,
//! `cpu_reset_length` bits 20:18, `sys_reset_length` bits 17:15,
//! **`flashboot_mod_en` bit 14** (`wdt_hal_set_flashboot_en`,
//! `0x40020f88`). `wdtconfig1` bits 31:16 are the prescaler; `wdtconfig2`
//! is stage 0's timeout in prescaled ticks.
//!
//! **The ROM does not arm the MWDT — the reset values do.** The PAC's
//! `wdtconfig0` reset is `0x0004_c000`, which has `flashboot_mod_en`
//! already set, and nothing in the whole vendored ROM writes TIMG0 except
//! `wdt_hal_*`. MEASURED in a `--trace TIMG0` of a ROM-up boot of the
//! reference merged image (2026-09-22): the *first* TIMG0 access of the
//! whole boot is the second-stage bootloader disabling it —
//!
//! ```text
//! cyc=6496219 pc=0x40020ed6 W4 TIMG0+0x064 wdtwprotect = 0x50d83aa1
//! cyc=6496230 pc=0x40020f88 R4 TIMG0+0x048 wdtconfig0 = 0x0004c000
//! cyc=6496236 pc=0x40020f94 W4 TIMG0+0x048 wdtconfig0 = 0x00048000
//! cyc=6496240 pc=0x40020f9e W4 TIMG0+0x048 wdtconfig0 = 0x00448000
//! cyc=6496249 pc=0x40020ee6 W4 TIMG0+0x064 wdtwprotect = 0x00000000
//! ```
//!
//! — 40.6 ms into a boot whose flash-boot protection would have expired at
//! **0.325 s**. So the watchdog is counting from the reset vector and the
//! bootloader turns it off with room to spare; a bootloader that never gets
//! there resets the chip, which is the whole of
//! `docs/defects/2026-09-06-c6-analog-master-wedges-the-bootloader.md` —
//! whose board looped "about every 0.4 s", against 0.325 s of watchdog plus
//! the 40 ms of ROM in front of it.
//!
//! - **Counting** when `flashboot_mod_en` is set **and this boot is a flash
//!   boot** ([`Timg::set_flash_boot`] — the one *modeled* part, and the one
//!   with an argument on both sides; read its doc), or when `wdt_en` is set
//!   with a non-`Off` stage-0 action. A write to `wdtconfig0/1/2` re-arms
//!   from now; so does any write to `wdtfeed` (bit 31 is the documented
//!   one, and the ROM's `wdt_hal_feed` writes exactly that).
//! - **The tick** is `wdtconfig0.wdt_use_xtal`'s clock divided by
//!   `wdtconfig1`'s prescaler. The bit resets to 0 = `apb_clk`, and the
//!   rate comes from the PAC's own arithmetic: `WDT_CLK_PRESCALE`'s field
//!   doc is "MWDT clock period = **12.5 ns** \* TIMG_WDT_CLK_PRESCALE",
//!   i.e. **80 MHz** (see [`WDT_APB_HZ`]) — the number that makes the reset
//!   configuration come out at silicon's own loop period. The prescaler is
//!   a plain multiply; 0 is clamped to 1 rather than read as 65536 the way
//!   T0's *divider* is, because nothing documents 0 for this field.
//!   PCR's `timergroup0.wdt_clk_conf` (`+0x044`) gates that clock and could
//!   in principle reselect it; the gate is **not modelled**, and it is safe
//!   not to be — MEASURED in the same trace filtered to `PCR`, the register
//!   takes three writes in a 4 s boot, all from esp-hal's `TimerGroup`
//!   248 ms in, and every value it ever holds is `0x0040_0000` or
//!   `0x0000_0000`: the gate going on and off, never a different source.
//! - **On expiry**, stage 0's action decides. In flash-boot mode the stage
//!   field is `Off` (`0x0004_c000 >> 29 == 0`) and the reset is a **system**
//!   reset all the same — that is what the mode is: silicon printed
//!   `rst:0x7 (TG0_WDT_HPSYS)`, which is the HP-system code, and it is the
//!   only evidence there is for what flash-boot mode does. Outside it,
//!   `1` = interrupt (the existing `TGn_WDT_LEVEL` source), `2` = reset CPU,
//!   `3` = reset system — [`lp_emu_esp_common::MachineRequest::Reset`],
//!   which the machine turns into a reboot or an [`crate::machine::Outcome::Reset`].
//! - **TIMG1 stays accepted.** `regs::TIMG0` is the names/resets table for
//!   *both* instances, and `flashboot_mod_en` out of reset is a TIMG0 fact —
//!   only MWDT0 protects flash boot. Giving TIMG1 the same expiry would arm
//!   a watchdog out of a reset value that is not TIMG1's, so
//!   [`Timg::timg1`] keeps the old accept-and-note behaviour and prints the
//!   `WDT ARMED` note. If an image ever arms MWDT1 for real, that note is
//!   where it shows up.

use lp_emu_core::sched::EventId;
use lp_emu_esp_common::engine::timg::{
    CounterConfig, TickRate, TimgEngine, TimgEventIds, WdtWrite, wdt_write,
};
use lp_emu_esp_common::regfile::{lane_of, merge_lane};
use lp_emu_esp_common::{
    BusCx, MachineRequest, Peripheral, RegFile, ResetScope, ResetSource, Strap, Watchdog, Width,
    event_id, event_local,
};

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
/// The prescaler, bits 31:16.
const WDTCONFIG1: u32 = 0x4c;
/// Stage 0's timeout, in prescaled ticks.
const WDTCONFIG2: u32 = 0x50;
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
/// `wdtconfig0.wdt_flashboot_mod_en` — flash-boot protection, **set out of
/// reset** (`regs::TIMG0`'s `0x0004_c000`). See the module docs.
const WDT_FLASHBOOT_MOD_EN: u32 = 1 << 14;
/// `wdtconfig0.wdt_stg0`, two bits (the ROM's own mask is `0x9fff_ffff`).
const WDT_STG0_SHIFT: u32 = 29;
const WDT_STG_MASK: u32 = 0b11;
/// Stage actions, ESP-IDF's `wdt_stage_action_t`. The MWDT's field is two
/// bits wide, so `RESET_RTC` (4) cannot be spelled here at all.
const WDT_STG_OFF: u32 = 0;
const WDT_STG_INTERRUPT: u32 = 1;
const WDT_STG_RESET_CPU: u32 = 2;
const WDT_STG_RESET_SYSTEM: u32 = 3;
/// `wdtconfig0.wdt_use_xtal` — "choose WDT clock: 0-apb_clk, 1-xtal_clk"
/// (the PAC's own field doc). Resets to 0.
const WDT_USE_XTAL: u32 = 1 << 21;
/// The MWDT's clock when `wdt_use_xtal` is clear, before `wdtconfig1`'s
/// prescaler: **80 MHz**, from the PAC's own arithmetic —
/// `WDT_CLK_PRESCALE`'s field doc reads "MWDT clock period = **12.5 ns** \*
/// TIMG_WDT_CLK_PRESCALE", and 12.5 ns is 80 MHz. *Documented.*
const WDT_APB_HZ: u64 = 80_000_000;

const EV_WDT: u16 = 2;

const CALI_RDY: u32 = 1 << 15;
const CALI_START_CYCLING: u32 = 1 << 12;
const CALI_MAX_SHIFT: u32 = 16;
const CALI_MAX_MASK: u32 = 0x7fff;
const CALI_START: u32 = 1 << 31;
const CALI2_TIMEOUT: u32 = 1;

const EV_ALARM: u16 = 0;
const EV_CALI: u16 = 1;

/// The C6 TIMG has one timer (`timg0.rs:5`, `t: [T; 1]`). The S3 and the
/// classic have two, and the classic a third (LACT) besides — which is why
/// the engine takes a count and this view passes 1.
const COUNTERS: usize = 1;
/// This view's only counter, in the engine's `Vec`.
const T0: usize = 0;

/// One timer group: **a view over
/// [`TimgEngine`](lp_emu_esp_common::engine::timg::TimgEngine)**.
///
/// The counters, their alarms and the watchdog gate are behaviour and live
/// in the engine. What lives here is everything that would be wrong on
/// another part: the offsets, the bit positions, the `RegFile` and its PAC
/// reset values, the XTAL and CPU rates, the 54-bit counter width, the
/// interrupt source numbers, the `EventId` packing — **and the RTC
/// calibration**, which is generic in shape but two chip clock rates in
/// content, has exactly one consumer, and whose classic counterpart runs off
/// a different clock path. It stays here deliberately; its absence from the
/// engine is not an oversight.
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
    t0_source: u16,
    wdt_source: u16,
    warned_decrement: bool,
    /// Whether this instance's MWDT really expires. TIMG0 only — see the
    /// module docs' "TIMG1 stays accepted".
    wdt_expiry: bool,
    /// Set once the MWDT has expired with a reset action, so the trace says
    /// so once per boot rather than once per slice.
    wdt_expired: bool,
    /// Whether this boot **is** a flash boot, and therefore whether
    /// `flashboot_mod_en` counts. See [`Timg::set_flash_boot`].
    flash_boot: bool,
}

impl Timg {
    fn new(name: &'static str, t0_source: u16, wdt_source: u16, wdt_expiry: bool) -> Self {
        Self {
            name,
            index: 0,
            regs: RegFile::new(name, 0x100).with_names(regs::TIMG0),
            engine: TimgEngine::new(COUNTERS),
            // Reset: `start_cycling` set → a cycling calibration that has
            // already completed once (modeled).
            cali_rdy: true,
            cali_value: cali_value_for(1),
            t0_source,
            wdt_source,
            warned_decrement: false,
            wdt_expiry,
            wdt_expired: false,
            // A chip that comes out of reset strapped for flash boot, which
            // is every boot but a download-mode one; the machine tells the
            // block otherwise.
            flash_boot: true,
        }
    }

    pub fn timg0() -> Self {
        Self::new("TIMG0", source::TG0_T0_LEVEL, source::TG0_WDT_LEVEL, true)
    }

    pub fn timg1() -> Self {
        Self::new("TIMG1", source::TG1_T0_LEVEL, source::TG1_WDT_LEVEL, false)
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

    /// The scheduler ids this block has assigned to T0's events. The engine
    /// never packs one: it does not know the peripheral index.
    fn ids(&self) -> TimgEventIds {
        TimgEventIds {
            alarm: event_id(self.index, EV_ALARM),
        }
    }

    /// Everything the engine needs about T0, read out of this chip's own
    /// registers. Cheap enough to build unconditionally at each call site.
    ///
    /// **The rate.** T0 counts XTAL ticks at `divider` against a CPU running
    /// at `memmap::CPU_HZ`, so `numer / denom` is `XTAL_HZ / (divider ×
    /// CPU_HZ)` — which is the block's own arithmetic, folded: the count was
    /// `delta × XTAL_HZ / (divider × CPU_HZ)` and the re-arm
    /// `(ticks × divider × CPU_HZ).div_ceil(XTAL_HZ)`, both in `u128`, and
    /// both become `delta × numer / denom` and `(ticks × denom)
    /// .div_ceil(numer)` in the same `u128`. `divider × CPU_HZ` is at most
    /// `65536 × 160_000_000 ≈ 2^43.3`, so folding it into a `u64` `denom`
    /// cannot overflow and the products are the same integers they were.
    fn counter_config(&self) -> CounterConfig {
        let cfg = self.config();
        CounterConfig {
            enabled: cfg & CFG_EN != 0,
            alarm_enabled: cfg & CFG_ALARM_EN != 0,
            auto_reload: cfg & CFG_AUTORELOAD != 0,
            rate: TickRate {
                numer: XTAL_HZ,
                denom: self.divider() * memmap::CPU_HZ,
            },
            mask: COUNTER_MASK,
            alarm: self.alarm(),
            load_value: self.load_value(),
        }
    }

    /// The count at `now`.
    pub fn count(&self, now: u64) -> u64 {
        self.engine.count(T0, &self.counter_config(), now)
    }

    /// What a read of `t0lo`/`t0hi` returns: the value the last `update`
    /// pulse latched.
    pub fn latched(&self) -> u64 {
        self.engine.latched(T0)
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
        let cfg = self.counter_config();
        let ids = self.ids();
        self.engine.rearm(T0, &cfg, ids, cx);
    }

    fn write_config(&mut self, value: u32, cx: &mut BusCx<'_>) {
        let old = self.config();
        let mut new = value;
        if new & CFG_INCREASE == 0 && !self.warned_decrement {
            self.warned_decrement = true;
            log::warn!(
                "{}: t0config.increase cleared; decrementing mode is not modelled",
                self.name
            );
        }
        if (old ^ new) & CFG_EN != 0 {
            // The engine freezes against the configuration as it stood
            // *before* this write — its divider included — because that is
            // the rate the count it is freezing was produced at.
            let before = self.counter_config();
            self.engine
                .set_enabled(T0, &before, new & CFG_EN != 0, cx.now);
        }
        // The divider-counter reset is a pulse.
        new &= !CFG_DIVCNT_RST;
        self.regs.poke(T0_CONFIG, new);
        self.rearm(cx);
    }

    fn wdt_unlocked(&self) -> bool {
        self.regs.stored(WDTWPROTECT) == WDT_WKEY
    }

    /// Whether this boot is a flash boot, which is what decides whether
    /// `flashboot_mod_en` counts. Set by the machine from the strapping, and
    /// re-set on every reboot; the default is `true`.
    ///
    /// **Modeled**, and the one part of this watchdog with no direct
    /// evidence. Against it: the block cannot see the strapping pins and
    /// nothing in the vendored ROM ever writes TIMG0, so on the register
    /// level a download-mode chip looks exactly like a flash-booting one.
    /// For it: the register's own name, the PAC's own field doc ("When set,
    /// **Flash boot** protection is enabled"), the ESP32-family TRM's
    /// wording ("MWDT is enabled in flash boot protection *procedure*") —
    /// and the fact that a board really does sit in the ROM's download
    /// console indefinitely while esptool talks to it, which it could not do
    /// if this watchdog reset it every 0.325 s. The strapping IS a hardware
    /// signal latched at reset, so a hardware gate on it is the reading that
    /// fits both observations. Promoting or refuting it needs a bench
    /// sitting: hold a board in download mode and read `wdtconfig0`.
    pub fn set_flash_boot(&mut self, flash_boot: bool, cx: &mut BusCx<'_>) {
        self.flash_boot = flash_boot;
        self.rearm_wdt(cx);
    }

    /// `wdtconfig0.wdt_stg0`.
    fn wdt_stage0(&self) -> u32 {
        (self.regs.stored(WDTCONFIG0) >> WDT_STG0_SHIFT) & WDT_STG_MASK
    }

    /// Is the MWDT counting? Flash-boot protection counts on its own; the
    /// ordinary enable needs a stage-0 action to do anything with.
    pub fn wdt_armed(&self) -> bool {
        self.flash_boot_armed() || self.stage_armed()
    }

    /// Flash-boot protection, counting. See [`Timg::set_flash_boot`] for the
    /// second term.
    fn flash_boot_armed(&self) -> bool {
        self.flash_boot && self.regs.stored(WDTCONFIG0) & WDT_FLASHBOOT_MOD_EN != 0
    }

    /// The ordinary enable, with something for stage 0 to do.
    fn stage_armed(&self) -> bool {
        self.regs.stored(WDTCONFIG0) & WDT_EN != 0 && self.wdt_stage0() != WDT_STG_OFF
    }

    /// `wdtconfig1`'s prescaler. **Not** the timer's 0 → 65536 reading: the
    /// PAC states the MWDT's own formula as a plain multiply ("MWDT clock
    /// period = 12.5 ns \* TIMG_WDT_CLK_PRESCALE"), which says nothing about
    /// 0, so 0 is clamped to 1 rather than given a meaning nothing measured.
    /// Unreachable on any path here: the reset value is 1 and both the ROM's
    /// `wdt_hal_init` and esp-hal write 1.
    fn wdt_prescaler(&self) -> u64 {
        u64::from(self.regs.stored(WDTCONFIG1) >> 16).max(1)
    }

    /// The MWDT's source clock, per `wdtconfig0.wdt_use_xtal`.
    fn wdt_src_hz(&self) -> u64 {
        if self.regs.stored(WDTCONFIG0) & WDT_USE_XTAL != 0 {
            XTAL_HZ
        } else {
            WDT_APB_HZ
        }
    }

    /// Emulated cycles from an arm or a feed to stage-0 expiry.
    pub fn wdt_stage0_cycles(&self) -> u64 {
        u64::from(self.regs.stored(WDTCONFIG2))
            .saturating_mul(self.wdt_prescaler())
            .saturating_mul(memmap::CPU_HZ)
            / self.wdt_src_hz()
    }

    /// Put a value in `wdtconfig0` from outside the guest, ignoring the
    /// write protection. The one caller is
    /// [`crate::loader::disable_flash_boot_watchdog`], which stands in for
    /// the second-stage bootloader on a direct load; it runs before
    /// `start_peripherals`, so no schedule exists yet to re-arm.
    pub fn poke_wdtconfig0(&mut self, value: u32) {
        self.regs.poke(WDTCONFIG0, value);
    }

    fn rearm_wdt(&mut self, cx: &mut BusCx<'_>) {
        if !self.wdt_expiry {
            return;
        }
        let ev = event_id(self.index, EV_WDT);
        cx.sched.cancel(ev);
        if self.wdt_armed() {
            cx.sched.schedule_in(cx.now, self.wdt_stage0_cycles(), ev);
        }
    }

    fn read_word(&self, off: u32) -> u32 {
        match off {
            T0_LO => self.latched() as u32,
            T0_HI => (self.latched() >> 32) as u32,
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
                    let cfg = self.counter_config();
                    self.engine.latch(T0, &cfg, cx.now);
                }
            }
            T0_ALARMLO | T0_ALARMHI => {
                self.regs.poke(off, value);
                self.rearm(cx);
            }
            T0_LOADLO | T0_LOADHI => self.regs.poke(off, value),
            T0_LOAD => {
                if value & 1 != 0 {
                    let cfg = self.counter_config();
                    self.engine.load(T0, &cfg, cx.now);
                    self.rearm(cx);
                }
            }
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
                    // "Feeding" restarts the count. The documented bit is 31
                    // and the ROM's `wdt_hal_feed` writes exactly that; the
                    // register is write-only and has no other field, so any
                    // write is taken as the feed it must be.
                    self.rearm_wdt(cx);
                    return;
                }
                self.regs.poke(off, value);
                if verdict == WdtWrite::ArmedNow && !self.wdt_expiry {
                    let line = format!(
                        "cyc={} pc=0x{:08x} {} WDT ARMED (MWDT expiry is not modelled)",
                        cx.now, cx.pc, self.name
                    );
                    cx.trace.note(&line);
                    log::warn!("{}: MWDT armed; its expiry is not modelled", self.name);
                }
                if matches!(off, WDTCONFIG0 | WDTCONFIG1 | WDTCONFIG2) {
                    self.rearm_wdt(cx);
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

impl Timg {
    /// Stage 0 came due. Flash-boot protection resets the system whatever
    /// the (reset-valued, `Off`) stage field says; outside it the stage
    /// action decides. See the module docs for why the two disagree.
    fn on_wdt_expiry(&mut self, cx: &mut BusCx<'_>) {
        if !self.wdt_armed() {
            return;
        }
        let flashboot = self.flash_boot_armed();
        // Flash-boot protection is not a stage action: the stage field reads
        // `Off` out of reset and the mode resets the chip anyway. Silicon
        // printed the HP-system code for it, so it is read as `ResetSystem`.
        let action = if flashboot {
            WDT_STG_RESET_SYSTEM
        } else {
            self.wdt_stage0()
        };
        if action == WDT_STG_INTERRUPT {
            self.regs.poke(INT_RAW, self.regs.stored(INT_RAW) | 2);
            self.update_lines(cx);
            return;
        }
        // `wdt_expiry` is TIMG0's alone, so the instance name is a constant
        // here — `source` is a `&'static str` the machine logs verbatim.
        let (source, scope) = match (flashboot, action) {
            (true, _) => ("TIMG0 MWDT flash-boot protection", ResetScope::Core),
            (_, WDT_STG_RESET_CPU) => ("TIMG0 MWDT stage 0 (ResetCpu)", ResetScope::Cpu),
            _ => ("TIMG0 MWDT stage 0 (ResetSystem)", ResetScope::Core),
        };
        if !self.wdt_expired {
            self.wdt_expired = true;
            let line = format!(
                "cyc={} pc=0x{:08x} {} MWDT EXPIRED: {source}",
                cx.now, cx.pc, self.name
            );
            cx.trace.note(&line);
        }
        let at = cx.now;
        // A watchdog reset boots the app: the strap pin is not involved.
        cx.request(MachineRequest::Reset {
            source,
            at,
            strap: Strap::App,
            cause: ResetSource::Watchdog {
                watchdog: Watchdog::Mwdt(0),
                scope,
            },
        });
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

    /// **The chip comes out of reset with MWDT0 already counting.**
    /// `wdtconfig0`'s reset word has `flashboot_mod_en` set, so the first
    /// expiry is due before any guest instruction runs and nothing will ever
    /// write the register to schedule it. This is the one hook that can arm
    /// it (`Peripheral::started`, guest time zero, a full `BusCx`).
    fn started(&mut self, cx: &mut BusCx<'_>) {
        self.rearm_wdt(cx);
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
                let counter = self.counter_config();
                // The engine's verdict — is the alarm really due, or was the
                // compare moved out since this event was scheduled? — plus
                // the auto-reload, which is its state. The register work
                // below is this view's.
                if !self.engine.on_alarm(T0, &counter, cx.now) {
                    return;
                }
                // "Automatically cleared once an alarm occurs" (the PAC's
                // field doc).
                self.regs.poke(T0_CONFIG, cfg & !CFG_ALARM_EN);
                self.regs.poke(INT_RAW, self.regs.stored(INT_RAW) | 1);
                self.update_lines(cx);
            }
            EV_CALI => {
                let max = (self.regs.stored(RTCCALICFG) >> CALI_MAX_SHIFT) & CALI_MAX_MASK;
                self.cali_value = cali_value_for(max.max(1));
                self.cali_rdy = true;
            }
            EV_WDT => self.on_wdt_expiry(cx),
            _ => {}
        }
    }

    fn reg_name(&self, off: u32) -> Option<&'static str> {
        regs::TIMG0.name(off)
    }

    /// The downcast seam the direct loader needs: see
    /// [`Timg::poke_wdtconfig0`].
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }

    fn save_state(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(0x100 + 48);
        out.extend_from_slice(&(self.index as u64).to_le_bytes());
        // Exactly where `base_ticks`, `base_cycle` and `latched` were
        // written before the engine held them: the blob's bytes do not move
        // when a block becomes a view.
        self.engine.save(&mut out);
        out.extend_from_slice(&u32::from(self.cali_rdy).to_le_bytes());
        out.extend_from_slice(&self.cali_value.to_le_bytes());
        out.extend_from_slice(&u32::from(self.warned_decrement).to_le_bytes());
        // After the three words that were always here and before the
        // register file, which is where every other block puts its own
        // additions: the blob is only ever read back by this type.
        out.extend_from_slice(&u32::from(self.wdt_expired).to_le_bytes());
        out.extend_from_slice(&u32::from(self.flash_boot).to_le_bytes());
        out.extend_from_slice(&self.regs.save_state());
        out
    }

    fn load_state(&mut self, bytes: &[u8]) {
        let mut r = Reader(bytes);
        // The engine's three words per counter sit where they always did.
        // It parses before it applies, so a short blob leaves it untouched
        // rather than half-loaded — which is what this early return needs.
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
        self.wdt_expired = r.u32().unwrap_or(0) != 0;
        self.flash_boot = r.u32().unwrap_or(1) != 0;
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
        assert_eq!(sb.read(&mut t, T0_CONFIG), pac(T0_CONFIG));
        // Disabled at reset: holds 0.
        sb.now = 1_600_000;
        assert_eq!(now(&mut sb, &mut t), 0);
        sb.write(&mut t, T0_CONFIG, pac(T0_CONFIG) | CFG_EN);
        sb.now = 3_200_000; // +10 ms = 200_000 ticks at 20 MHz
        assert_eq!(now(&mut sb, &mut t), 200_000);
        // Stop freezes; restart continues.
        sb.write(&mut t, T0_CONFIG, pac(T0_CONFIG));
        sb.now = 4_000_000;
        assert_eq!(now(&mut sb, &mut t), 200_000);
        sb.write(&mut t, T0_CONFIG, pac(T0_CONFIG) | CFG_EN);
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
        sb.write(&mut t, T0_CONFIG, pac(T0_CONFIG) | CFG_EN);
        sb.now = 5_000 + 8 * 100;
        sb.write(&mut t, T0_ALARMLO, 50);
        let cfg = sb.read(&mut t, T0_CONFIG);
        sb.write(&mut t, T0_CONFIG, cfg | CFG_ALARM_EN);
        assert_eq!(sb.sched.next_deadline(), Some(5_800));
        sb.write(&mut t, T0_CONFIG, pac(T0_CONFIG)); // en = 0
        assert_eq!(sb.sched.next_deadline(), None);
    }

    #[test]
    fn the_mwdt_is_write_protected_and_timg1_still_only_leaves_a_note() {
        let buf = lp_emu_esp_common::trace::SharedBuffer::new();
        let mut sb = Sandbox::new();
        sb.trace = lp_emu_esp_common::Trace::to_sink(Box::new(buf.clone()));
        let mut t = Timg::timg1();
        assert_eq!(sb.read(&mut t, WDTCONFIG0), pac(WDTCONFIG0));
        // **TIMG0 comes out of reset UNLOCKED.** `wdtwprotect` resets to
        // the write key itself (`0x50d8_3aa1`), which the reset sweep
        // brought in from the PAC; this model used to hold it at 0 and
        // refuse the first write. LP_WDT is the other way round — its own
        // `wdtwprotect` has no non-zero reset, so it starts locked — and
        // the two blocks disagreeing is the part, not a mistake.
        assert_eq!(sb.read(&mut t, WDTWPROTECT), WDT_WKEY, "unlocked at reset");
        sb.write(&mut t, WDTCONFIG0, 0);
        assert_eq!(sb.read(&mut t, WDTCONFIG0), 0, "and the write took");
        sb.write(&mut t, WDTCONFIG0, pac(WDTCONFIG0));
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
        assert!(
            sb.sched.next_deadline().is_none(),
            "TIMG1's MWDT is still accepted, so nothing was scheduled"
        );
        assert!(sb.request.is_none());
    }

    /// **The reset values arm MWDT0, and nothing else does.** The first
    /// thing this block does on a cold chip is schedule its own expiry.
    #[test]
    fn mwdt0_comes_out_of_reset_counting_for_the_flash_boot_and_timg1_does_not() {
        let mut sb = Sandbox::new();
        let mut t = Timg::timg0();
        t.attached(3);
        // The three reset words the schedule is computed from, straight out
        // of the generated PAC table — a test that carried its own copies
        // could not notice the table changing.
        assert_eq!(pac(WDTCONFIG0), 0x0004_c000, "flashboot_mod_en set");
        assert_eq!(pac(WDTCONFIG0) & WDT_FLASHBOOT_MOD_EN, WDT_FLASHBOOT_MOD_EN);
        assert_eq!(pac(WDTCONFIG0) & WDT_EN, 0, "…and wdt_en is NOT");
        assert_eq!(
            (pac(WDTCONFIG0) >> WDT_STG0_SHIFT) & WDT_STG_MASK,
            WDT_STG_OFF,
            "…nor is any stage action: flash-boot mode is its own thing"
        );
        assert_eq!(pac(WDTCONFIG1) >> 16, 1, "prescaler 1");
        assert_eq!(pac(WDTCONFIG2), 26_000_000, "stage-0 timeout, in ticks");

        assert_eq!(pac(WDTCONFIG0) & WDT_USE_XTAL, 0, "apb_clk, not the XTAL");

        assert!(t.wdt_armed());
        t.started(&mut sb.cx());
        // 26,000,000 ticks at 12.5 ns = 0.325 s, which against silicon's
        // "about every 0.4 s" leaves the 40 ms of ROM in front of it.
        let expiry = 26_000_000 * memmap::CPU_HZ / WDT_APB_HZ;
        assert_eq!(expiry, 325 * memmap::CPU_HZ / 1000);
        assert_eq!(sb.sched.next_deadline(), Some(expiry));

        // …and the other source, when a driver asks for it.
        sb.write(&mut t, WDTCONFIG0, pac(WDTCONFIG0) | WDT_USE_XTAL);
        assert_eq!(
            sb.sched.next_deadline(),
            Some(26_000_000 * memmap::CPU_HZ / XTAL_HZ),
            "the XTAL is half the APB clock, so twice the timeout"
        );

        // TIMG1 shares the names table — and therefore the reset word — but
        // not the behaviour.
        let mut sb1 = Sandbox::new();
        let mut t1 = Timg::timg1();
        t1.attached(4);
        assert!(t1.wdt_armed(), "the same register says the same thing");
        t1.started(&mut sb1.cx());
        assert_eq!(sb1.sched.next_deadline(), None, "and nothing happens");
    }

    /// **A download-mode boot is not a flash boot**, so the protection does
    /// not count — see [`Timg::set_flash_boot`], which is where the argument
    /// for and against this lives. The ordinary enable is unaffected, which
    /// is the whole point of keeping the two terms apart.
    #[test]
    fn a_download_mode_boot_does_not_count_the_flash_boot_protection() {
        let mut sb = Sandbox::new();
        let mut t = Timg::timg0();
        t.attached(3);
        t.set_flash_boot(false, &mut sb.cx());
        assert!(
            !t.wdt_armed(),
            "the register still says so, the boot does not"
        );
        assert_eq!(
            sb.read(&mut t, WDTCONFIG0) & WDT_FLASHBOOT_MOD_EN,
            WDT_FLASHBOOT_MOD_EN,
            "and the bit reads back set, because silicon's would"
        );
        t.started(&mut sb.cx());
        assert_eq!(sb.sched.next_deadline(), None);
        sb.run_to(&mut t, 4 * memmap::CPU_HZ);
        assert!(
            sb.request.is_none(),
            "four seconds in the console, no reset"
        );

        // A guest that arms the MWDT properly still gets it.
        sb.write(&mut t, WDTCONFIG2, 80_000);
        sb.write(
            &mut t,
            WDTCONFIG0,
            WDT_EN | (WDT_STG_RESET_SYSTEM << WDT_STG0_SHIFT),
        );
        assert!(t.wdt_armed());
        let due = sb.sched.next_deadline().expect("armed");
        sb.run_to(&mut t, due);
        assert!(sb.request.is_some());
    }

    /// The bootloader's own disable, register for register out of a
    /// `--trace TIMG0` of a ROM-up boot (see the module docs), and the feed
    /// that would have kept it alive instead.
    #[test]
    fn the_bootloaders_disable_cancels_the_flash_boot_expiry_and_a_feed_pushes_it_out() {
        let mut sb = Sandbox::new();
        let mut t = Timg::timg0();
        t.attached(3);
        t.started(&mut sb.cx());
        let period = t.wdt_stage0_cycles();
        assert_eq!(sb.sched.next_deadline(), Some(period));

        // A feed 0.3 s in re-arms from there, and does not stack.
        sb.run_to(&mut t, 3 * memmap::CPU_HZ / 10);
        sb.write(&mut t, WDTFEED, 1 << 31);
        assert_eq!(
            sb.sched.next_deadline(),
            Some(3 * memmap::CPU_HZ / 10 + period)
        );
        assert_eq!(sb.sched.live(), 1, "re-armed, not stacked");

        // `wdt_hal_set_flashboot_en(ctx, false)`: unlock, clear bit 14, set
        // `conf_update_en`, lock.
        sb.write(&mut t, WDTWPROTECT, WDT_WKEY);
        let cfg = sb.read(&mut t, WDTCONFIG0);
        sb.write(&mut t, WDTCONFIG0, cfg & !WDT_FLASHBOOT_MOD_EN);
        let cfg = sb.read(&mut t, WDTCONFIG0);
        sb.write(&mut t, WDTCONFIG0, cfg | (1 << 22));
        sb.write(&mut t, WDTWPROTECT, 0);
        assert_eq!(sb.read(&mut t, WDTCONFIG0), 0x0044_8000, "the trace's word");
        assert!(!t.wdt_armed());
        assert_eq!(sb.sched.next_deadline(), None, "and the expiry is gone");

        // A locked write cannot re-arm it.
        sb.write(&mut t, WDTCONFIG0, pac(WDTCONFIG0));
        assert_eq!(sb.sched.next_deadline(), None);
    }

    /// **The loop the first-flash defect recorded.** A bootloader that never
    /// reaches the disable above gets a system reset carrying TIMG0's cause,
    /// which `ResetCause::for_source` turns into `0x7 (TG0_WDT_HPSYS)`.
    #[test]
    fn an_undisabled_flash_boot_watchdog_asks_for_a_tg0_reset() {
        let buf = lp_emu_esp_common::trace::SharedBuffer::new();
        let mut sb = Sandbox::new();
        sb.trace = lp_emu_esp_common::Trace::to_sink(Box::new(buf.clone()));
        let mut t = Timg::timg0();
        t.attached(3);
        t.started(&mut sb.cx());
        let expiry = t.wdt_stage0_cycles();

        sb.run_to(&mut t, expiry - 1);
        assert!(sb.request.is_none(), "not yet");
        sb.run_to(&mut t, expiry);
        let request = sb.request.expect("the MWDT asked for a reset");
        assert_eq!(
            request,
            MachineRequest::Reset {
                source: "TIMG0 MWDT flash-boot protection",
                at: expiry,
                strap: Strap::App,
                cause: ResetSource::Watchdog {
                    watchdog: Watchdog::Mwdt(0),
                    scope: ResetScope::Core,
                },
            }
        );
        let MachineRequest::Reset { cause, .. } = request;
        let cause = crate::loader::ResetCause::for_source(cause);
        assert_eq!(cause.rom_code(), 0x7);
        assert_eq!(cause.rom_name(), "TG0_WDT_HPSYS");
        assert!(cause.records_saved_pc(), "and the ROM prints a Saved PC");
        assert_eq!(buf.lines().len(), 1);
        assert!(buf.lines()[0].contains("TIMG0 MWDT EXPIRED"));
    }

    /// Outside flash-boot mode the stage field decides, as it does on
    /// LP_WDT: `1` raises the block's WDT interrupt and asks for nothing.
    #[test]
    fn an_armed_mwdt_honours_its_stage_action() {
        for (action, expected) in [
            (
                WDT_STG_RESET_CPU,
                Some(("TIMG0 MWDT stage 0 (ResetCpu)", ResetScope::Cpu)),
            ),
            (
                WDT_STG_RESET_SYSTEM,
                Some(("TIMG0 MWDT stage 0 (ResetSystem)", ResetScope::Core)),
            ),
            (WDT_STG_INTERRUPT, None),
        ] {
            let mut sb = Sandbox::new();
            let mut t = Timg::timg0();
            t.attached(3);
            t.started(&mut sb.cx());
            sb.write(&mut t, INT_ENA, 0b10);
            // Flash-boot off, `wdt_en` on with this stage action, and a
            // 1 ms timeout: 80,000 ticks of the 80 MHz APB clock.
            sb.write(&mut t, WDTCONFIG2, 80_000);
            sb.write(
                &mut t,
                WDTCONFIG0,
                WDT_EN | (action << WDT_STG0_SHIFT) | (1 << 22),
            );
            assert!(t.wdt_armed());
            let due = sb.sched.next_deadline().expect("armed");
            assert_eq!(due, memmap::CPU_HZ / 1000, "1 ms");
            sb.run_to(&mut t, due);
            match expected {
                Some((source, scope)) => {
                    assert_eq!(
                        sb.request,
                        Some(MachineRequest::Reset {
                            source,
                            at: due,
                            strap: Strap::App,
                            cause: ResetSource::Watchdog {
                                watchdog: Watchdog::Mwdt(0),
                                scope,
                            },
                        })
                    );
                    assert!(!sb.irq.level(source::TG0_WDT_LEVEL));
                }
                None => {
                    assert!(sb.request.is_none(), "an interrupt stage asks for nothing");
                    assert!(sb.irq.level(source::TG0_WDT_LEVEL));
                    assert_eq!(sb.read(&mut t, INT_ST), 0b10);
                    sb.write(&mut t, INT_CLR, 0b10);
                    assert!(!sb.irq.level(source::TG0_WDT_LEVEL));
                }
            }
        }
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
        sb.write(&mut t, T0_CONFIG, pac(T0_CONFIG) | CFG_EN);
        sb.now = 1_777;
        now(&mut sb, &mut t);
        let blob = t.save_state();
        let mut other = Timg::timg0();
        other.load_state(&blob);
        assert_eq!(other.count(sb.now), t.count(sb.now));
        assert_eq!(other.latched(), t.latched());
        assert_eq!(other.index, 9);
        assert_eq!(other.regs.stored(T0_CONFIG), pac(T0_CONFIG) | CFG_EN);
    }
}
