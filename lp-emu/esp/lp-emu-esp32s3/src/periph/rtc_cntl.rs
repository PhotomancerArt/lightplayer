//! `RTC_CNTL` at `0x6000_8000` — the reset cause, the CPU stall key, **the
//! RWDT that really runs**, the super-watchdog, and the clock and store
//! registers the boot leans on.
//!
//! **The classic's view, parameterised by an offset table** (`m6/notes.md`
//! §3.0 row 7): the register *names* are the classic's, but the S3 inserts
//! `timer6` at `+0x30` and splits `time0/time1`, so everything from
//! `ana_conf` on is shifted **+4**, and the S3 adds the super-watchdog
//! pair. The table, every entry from `regs::RTC_CNTL`:
//!
//! | register | S3 | classic |
//! |---|---|---|
//! | `options0` | `+0x000` | `+0x000` |
//! | `ana_conf` | `+0x034` | `+0x030` |
//! | `reset_state` | `+0x038` | `+0x034` |
//! | `store1` | `+0x054` | `+0x050` |
//! | `wdtconfig0..4` | `+0x098..+0x0a8` | `+0x08c..+0x09c` |
//! | `wdtfeed` | `+0x0ac` | `+0x0a0` |
//! | `wdtwprotect` | `+0x0b0` | `+0x0a4` |
//! | `swd_conf` / `swd_wprotect` | `+0x0b4` / `+0x0b8` | — |
//! | `sw_cpu_stall` | `+0x0bc` | `+0x0ac` |
//! | `date` | `+0x1fc` | `+0x13c` |
//!
//! The copy lives here rather than parameterising the classic's file
//! because the plan's invariant is that the classic does not move by a
//! byte; the extraction into `lp-emu-esp-common` is M8's, and what it would
//! extract is exactly this table plus the stall-key arithmetic.
//!
//! # The reset cause is an input to the run, not a property of the part
//!
//! The mask ROM's `rtc_get_reset_reason` (`0x4004_456C`) reads
//! `reset_state` (`+0x38`) and masks six bits per core:
//!
//! ```text
//! 4004456f:  l32r   a8, (0x60008038)     ; RTC_CNTL + 0x38
//! 40044577:  l32i.n a2, a8, 0
//! 40044579:  extui  a2, a2, 0, 6         ; PRO: bits 5:0
//! 40044585:  extui  a2, a2, 6, 6         ; APP: bits 11:6
//! ```
//!
//! The PAC's reset for the register is `0x0000_3000` — both cause fields
//! zero, because an SVD cannot know why a chip is starting. This machine
//! asserts [`crate::loader::ResetCause`] into **both** fields, because a
//! power-on resets both cores. `1` is `POWERON_RESET`. **That is this
//! block's one deviation from the PAC**, listed in [`DEVIATIONS`].
//!
//! # Both halves of the stall key
//!
//! A core is stalled only when **two** fields in **two** registers hold the
//! right values at once — esp-hal spells the check out for this chip
//! (`soc/esp32s3/cpu_control.rs:55-77`):
//!
//! ```text
//! // sw_stall_appcpu_c1[5:0],  sw_stall_appcpu_c0[1:0]} == 0x86 will stall APP CPU
//! // sw_stall_procpu_c1[5:0],  sw_stall_procpu_c0[1:0]} == 0x86 will stall PRO CPU
//! let is_stalled = (c1 << 2) | c0;   …   is_stalled != 0x86
//! ```
//!
//! and parks a core by writing `c1 = 0x21` then `c0 = 0x02`
//! (`internal_park_core`, `:16-36`). The halves are `options0` bits 0:1 /
//! 2:3 and `sw_cpu_stall` bits 20:25 / 26:31 — the same bits as the
//! classic's, in a register four bytes further along. This block computes
//! the pair and publishes it through [`StallKey`], the third input of
//! `Machine::core_stalled` (the seam P03 named).
//!
//! # The RWDT — on the boot path, armed for the whole run
//!
//! This is the first watchdog in the plan that **runs**. The shipped
//! firmware (`m6/notes.md` §5.2; `lp-fw/fw-esp32s3/src/recovery/watchdog.rs`):
//!
//! | where | what |
//! |---|---|
//! | `board/esp32s3/init.rs:66-67` | `Rtc::new(peripherals.LPWR).rwdt` — returned **unarmed** (`esp_hal::init` has already disabled it, `lib.rs:755`) |
//! | `main.rs:311` | `WatchdogFeeder::start(rwdt, 0)` |
//! | `recovery/watchdog.rs:65-66` | `set_timeout(Stage0, BOOT_TIMEOUT_MS = 30_000)` then **`enable()`** |
//! | `recovery/watchdog.rs:82-85` | on the first feed, tightens to `WATCHDOG_TIMEOUT_MS = 8_000` |
//! | `recovery/watchdog.rs:92` | `feed()` — **only** if the io task was alive within `IO_SILENCE_LIMIT_MS = 2_000` |
//!
//! There is **no `disable()` anywhere** in that crate. So stage 0 is a real
//! counter on the scheduler here, with both keys honoured and both
//! directions tested — a model that never fired would hide the firmware's
//! deliberate withholding, and one that fired early would end every run
//! with no diagnostic. Register for register (esp-hal `rtc_cntl/mod.rs`):
//!
//! - `wdtconfig0..4` and `wdtfeed` take writes only while `wdtwprotect`
//!   holds [`super::WDT_WKEY`] (`:558-563`). ⚠️ `wdtwprotect` **resets to
//!   the key itself** in the PAC, so the RWDT comes out of reset unlocked —
//!   the part's surprise, the same one the MWDTs carry.
//! - `wdtconfig0` (PAC `rtc_cntl/wdtconfig0.rs`): `wdt_en` 31, `wdt_stg0`
//!   28:30 (`RwdtStageAction`: 0 off, 1 interrupt, 2 reset CPU, 3 reset
//!   core, 4 reset system — `:456-467`), `stg1..3` 25:27 / 22:24 / 19:21,
//!   `cpu_reset_length` 16:18, `sys_reset_length` 13:15,
//!   `flashboot_mod_en` 12, `pause_in_slp` 9. `enable()` writes
//!   `flashboot_mod_en = 0`, then `wdt_en | pause_in_slp`, then stage 0 =
//!   ResetSystem with both reset lengths 7 (`:566-596`); `disable()` writes
//!   0 (`:571-572`).
//! - `wdtconfig1` is stage 0's hold count, in RWDT ticks. `set_timeout`
//!   converts microseconds with the calibrated slow-clock period in
//!   `store1` (`clock/mod.rs:571-580`, `us_to_rtc_ticks`), then shifts
//!   **right by `1 + WDT_DELAY_SEL`** (`:613-614`; the eFuse field reads 0
//!   here, [`super::efuse`]) — which says the RWDT counts at
//!   `RC_SLOW / 2`. So stage 0 expires `hold × 2 / RC_SLOW_HZ` seconds
//!   after the last feed or arm — **modeled** from that shift and from
//!   [`super::RC_SLOW_HZ`]; nothing measured the RWDT's clock. With the
//!   firmware's 30 s boot timeout and the 136 kHz slow clock, `hold` is
//!   2,040,000 and expiry is 30 s, which is what the firmware asked for.
//! - `wdtfeed` bit 31 restarts the count (`:552-556`).
//! - On expiry, stage 0's action decides: `Interrupt` sets `int_raw.wdt`
//!   (bit 3, source `RTC_CORE` = 39 when `int_ena` lets it); any reset
//!   action asks the machine for a reset through
//!   [`lp_emu_esp_common::MachineRequest::Reset`], which the run reports as
//!   `Outcome::Reset` — the emulator has no boot chain to restart until
//!   P06, and "the RWDT expired at cycle N" is the answer a bring-up wants
//!   anyway. Stages 1..3 are stored, not modelled: the firmware leaves them
//!   `Off`.
//!
//! # The super-watchdog
//!
//! `swd_conf` (`+0xb4`) takes writes only while `swd_wprotect` holds
//! [`super::SWD_WKEY`] — **a different key from the RWDT's on this chip**
//! (`rtc_cntl/mod.rs:656-660`), and again the PAC's own reset for the
//! register, so the SWD is unlocked at power-on too. Bits (PAC
//! `rtc_cntl/swd_conf.rs`): `swd_reset_flag` 0, `swd_feed_int` 1,
//! `swd_bypass_rst` 17, `swd_signal_width` 18:27, `swd_disable` 30,
//! `swd_auto_feed_en` 31. `esp_hal::init` disables it by setting
//! `auto_feed_en` (`:667-674`, `lib.rs:752-753`); its expiry is **not
//! modelled**, and the reset `0x04b0_0000` — neither bit set, i.e. armed
//! from power-on — is left as the PAC states it.
//!
//! # The clock and store registers, each with its pin
//!
//! | register | who touches it | pin |
//! |---|---|---|
//! | `store1` (`+0x54`) | `calibrate_rtc_slow_clock` stashes the slow-clock period | `clock/mod.rs:526-535`; read back by `us_to_rtc_ticks` |
//! | `clk_conf` (`+0x74`) | `enable_rc_fast_clk`, the `ck8m` fields, `ana_clk_rtc_sel` | `soc/esp32s3/clocks.rs:266-283, 651-663` |
//! | `slow_clk_conf` (`+0x78`) | the RC_SLOW divider write-invalidate-write dance | `clocks.rs:307-327` |
//! | `timer1` / `timer3` / `timer5` / `timer6` | `RtcSleepConfig::base_settings` | `rtc_cntl/sleep/esp32s3.rs:458-485` |
//! | `dig_pwc` (`+0x90`), `ana_conf` (`+0x34`) | the same | `:449-456` |
//! | `date` (`+0x1fc`) | `ensure_voltage_raised` writes `ldo_slave` 13:18 into the **version register** | `clocks.rs:614-615, 640-641` |
//! | `options0` force bits | `enable_pll_clk_impl`'s `bb_i2c_force_pd` / `bbpll_force_pd` / `bbpll_i2c_force_pd` | `clocks.rs:147-153` |
//!
//! All of them are accept-and-remember at the PAC's resets: nothing in the
//! image reads a bit back that hardware would have changed.
//!
//! # `options0.sw_{app,pro}cpu_rst` are reported, not performed
//!
//! On this chip the software resets are **per core**: `sw_appcpu_rst` bit
//! 4 and `sw_procpu_rst` bit 5, write-only in the PAC (`options0.rs:223-
//! 228`); there is no whole-system `sw_sys_rst` bit 31 as on the classic.
//! A write to either leaves a trace note and a warning and is not
//! remembered; performing it is P06's.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use lp_emu_core::sched::EventId;
use lp_emu_esp_common::engine::timg::{WdtWrite, wdt_write};
use lp_emu_esp_common::regfile::{lane_of, merge_lane};
use lp_emu_esp_common::{BusCx, MachineRequest, Peripheral, RegFile, Strap, Width, event_id};

use super::systimer::Reader;
use super::{RC_SLOW_HZ, SWD_WKEY, WDT_WKEY};
use crate::loader::ResetCause;
use crate::memmap;
use crate::regs::{self, source};

/// The block's aperture, **tight**: the generated table runs to `+0x1fc`
/// (`date`). Tight so that an access into the gap is a strict stop naming
/// an undocumented offset, not a silent zero.
pub const RTC_CNTL_LEN: u32 = 0x200;

/// `options0` — one half of the stall key, the analog force bits, and the
/// per-core software resets.
pub const OPTIONS0: u32 = 0x000;
/// `ana_conf`.
pub const ANA_CONF: u32 = 0x034;
/// `reset_state` — the ROM's `rtc_get_reset_reason`, and this block's one
/// deviation from the PAC.
pub const RESET_STATE: u32 = 0x038;
const INT_ENA: u32 = 0x040;
const INT_RAW: u32 = 0x044;
const INT_ST: u32 = 0x048;
const INT_CLR: u32 = 0x04c;
/// `store1` — the calibrated slow-clock period.
pub const STORE1: u32 = 0x054;
/// `wdtconfig0`.
pub const WDTCONFIG0: u32 = 0x098;
/// `wdtconfig1` — stage 0's hold count.
pub const WDTCONFIG1: u32 = 0x09c;
const WDTCONFIG4: u32 = 0x0a8;
/// `wdtfeed`.
pub const WDTFEED: u32 = 0x0ac;
/// `wdtwprotect`.
pub const WDTWPROTECT: u32 = 0x0b0;
/// `swd_conf`.
pub const SWD_CONF: u32 = 0x0b4;
/// `swd_wprotect`.
pub const SWD_WPROTECT: u32 = 0x0b8;
/// `sw_cpu_stall` — the other half of the stall key.
pub const SW_CPU_STALL: u32 = 0x0bc;

/// `options0.sw_stall_appcpu_c0` (bits 0:1) and `sw_stall_procpu_c0`
/// (bits 2:3).
const C0_APP_SHIFT: u32 = 0;
const C0_PRO_SHIFT: u32 = 2;
const C0_MASK: u32 = 0b11;
/// `sw_cpu_stall.sw_stall_appcpu_c1` (bits 20:25) and `sw_stall_procpu_c1`
/// (bits 26:31).
const C1_APP_SHIFT: u32 = 20;
const C1_PRO_SHIFT: u32 = 26;
const C1_MASK: u32 = 0b11_1111;

/// `(c1 << 2) | c0` — the eight-bit key, and the one value that stalls.
/// esp-hal's own arithmetic and its own constant
/// (`soc/esp32s3/cpu_control.rs:55-77`).
pub const STALLED: u32 = 0x86;

/// `options0.sw_appcpu_rst` / `sw_procpu_rst` — write-only per-core
/// software resets.
const SW_CPU_RST: u32 = (1 << 4) | (1 << 5);

const WDT_EN: u32 = 1 << 31;
const STG0_SHIFT: u32 = 28;
const STG_MASK: u32 = 0b111;
const FEED_BIT: u32 = 1 << 31;
/// Stage actions (`RwdtStageAction`, `rtc_cntl/mod.rs:456-467`).
const STG_OFF: u32 = 0;
const STG_INTERRUPT: u32 = 1;
/// `int_raw.wdt` / `int_ena.wdt` — "RTC WDT interrupt raw", bit 3.
const INT_WDT: u32 = 1 << 3;

/// The RWDT's tick is the slow clock divided by this — `2^(1 + WDT_DELAY_SEL)`
/// with the eFuse field at 0 (see the module docs).
pub const RWDT_CLOCK_DIVIDER: u64 = 2;

const EV_STAGE0: u16 = 0;

/// The CPU stall key as this block computes it, shared with the machine.
///
/// One word: the APP core's eight-bit key in bits 0:7 and the PRO core's in
/// bits 8:15. A plain atomic rather than a channel, because the question —
/// "is core `n` held?" — is asked by the run loop between slices and
/// answered by whatever the guest last wrote, with no ordering to preserve.
#[derive(Clone, Debug, Default)]
pub struct StallKey(Arc<AtomicU32>);

impl StallKey {
    pub fn new() -> Self {
        Self::default()
    }

    /// The eight-bit key for `core` (0 = PRO, 1 = APP).
    pub fn key(&self, core: usize) -> u32 {
        let word = self.0.load(Ordering::Relaxed);
        if core == 0 { word >> 8 } else { word }
    }

    /// Is `core` held by **RTC_CNTL's** two halves? The machine ORs this
    /// with its own field and with `SYSTEM.core_1_control_0`.
    pub fn stalled(&self, core: usize) -> bool {
        self.key(core) & 0xff == STALLED
    }

    fn store(&self, pro: u32, app: u32) {
        self.0
            .store(((pro & 0xff) << 8) | (app & 0xff), Ordering::Relaxed);
    }
}

/// The S3's RTC controller.
#[derive(Debug)]
pub struct RtcCntl {
    index: usize,
    regs: RegFile,
    stall: StallKey,
    /// Set once stage 0 has expired with a reset action, so the trace says
    /// so once.
    expired: bool,
    warned_cpu_rst: bool,
}

impl RtcCntl {
    /// The block, with `cause` asserted into both halves of `reset_state`
    /// and `stall` the handle the machine will read.
    pub fn new(cause: ResetCause, stall: StallKey) -> Self {
        let code = cause.rom_code();
        let regs = RegFile::new("RTC_CNTL", RTC_CNTL_LEN)
            .with_names(regs::RTC_CNTL)
            .with_reset(
                RESET_STATE,
                regs::RTC_CNTL.reset(RESET_STATE).unwrap_or(0) | code | (code << 6),
            )
            .with_pac_grades();
        let out = Self {
            index: 0,
            regs,
            stall,
            expired: false,
            warned_cpu_rst: false,
        };
        out.publish_stall();
        out
    }

    /// The handle the machine holds.
    pub fn stall_key(&self) -> StallKey {
        self.stall.clone()
    }

    /// Recompute both cores' keys from the two registers and publish them.
    fn publish_stall(&self) {
        let o0 = self.regs.stored(OPTIONS0);
        let c1 = self.regs.stored(SW_CPU_STALL);
        let key = |c1_shift: u32, c0_shift: u32| {
            (((c1 >> c1_shift) & C1_MASK) << 2) | ((o0 >> c0_shift) & C0_MASK)
        };
        self.stall.store(
            key(C1_PRO_SHIFT, C0_PRO_SHIFT),
            key(C1_APP_SHIFT, C0_APP_SHIFT),
        );
    }

    fn wdt_unlocked(&self) -> bool {
        self.regs.stored(WDTWPROTECT) == WDT_WKEY
    }

    fn swd_unlocked(&self) -> bool {
        self.regs.stored(SWD_WPROTECT) == SWD_WKEY
    }

    /// `wdt_en` with a non-`Off` stage 0.
    pub fn rwdt_armed(&self) -> bool {
        let cfg = self.regs.stored(WDTCONFIG0);
        cfg & WDT_EN != 0 && (cfg >> STG0_SHIFT) & STG_MASK != STG_OFF
    }

    /// Emulated cycles from a feed to stage-0 expiry.
    pub fn stage0_cycles(&self) -> u64 {
        let hold = u64::from(self.regs.stored(WDTCONFIG1));
        hold.saturating_mul(RWDT_CLOCK_DIVIDER)
            .saturating_mul(memmap::CPU_HZ)
            / RC_SLOW_HZ
    }

    fn update_lines(&self, cx: &mut BusCx<'_>) {
        let st = self.regs.stored(INT_RAW) & self.regs.stored(INT_ENA);
        cx.irq.set_level(source::RTC_CORE, st != 0);
    }

    fn rearm(&mut self, cx: &mut BusCx<'_>) {
        let ev = event_id(self.index, EV_STAGE0);
        cx.sched.cancel(ev);
        if self.rwdt_armed() {
            cx.sched.schedule_in(cx.now, self.stage0_cycles(), ev);
        }
    }

    fn read_word(&self, off: u32) -> u32 {
        match off {
            // The pulses: write-only in the PAC, and a read of one must not
            // look like a request that never finished.
            WDTFEED | INT_CLR => 0,
            INT_ST => self.regs.stored(INT_RAW) & self.regs.stored(INT_ENA),
            other => self.regs.stored(other),
        }
    }

    fn write_word(&mut self, off: u32, value: u32, cx: &mut BusCx<'_>) {
        match off {
            OPTIONS0 => {
                if value & SW_CPU_RST != 0 && !self.warned_cpu_rst {
                    self.warned_cpu_rst = true;
                    let line = format!(
                        "cyc={} pc=0x{:08x} RTC_CNTL options0.sw_{{app,pro}}cpu_rst written (a \
                         per-core software reset; this machine has no boot chain to restart \
                         before P06, so the run carries on)",
                        cx.now, cx.pc
                    );
                    cx.trace.note(&line);
                    log::warn!(
                        "RTC_CNTL: a CPU software reset was written at pc={:#010x}; reported, \
                         not performed",
                        cx.pc
                    );
                }
                // The two reset bits are write-only pulses in the PAC;
                // everything else in the word is accept-and-remember, the
                // stall halves included.
                self.regs.poke(OPTIONS0, value & !SW_CPU_RST);
                self.publish_stall();
            }
            SW_CPU_STALL => {
                self.regs.poke(SW_CPU_STALL, value);
                self.publish_stall();
            }
            WDTCONFIG0..=WDTCONFIG4 => {
                let cfg0 = off == WDTCONFIG0;
                let old = self.regs.stored(off);
                let verdict = wdt_write(
                    self.wdt_unlocked(),
                    cfg0 && old & WDT_EN != 0,
                    cfg0 && value & WDT_EN != 0,
                );
                if verdict == WdtWrite::Locked {
                    log::debug!("RTC_CNTL: write to +{off:#05x} dropped, RWDT locked");
                    return;
                }
                self.regs.poke(off, value);
                if verdict == WdtWrite::ArmedNow {
                    let line = format!(
                        "cyc={} pc=0x{:08x} RTC_CNTL RWDT ARMED: stage 0 action {} expires in {} \
                         cycles unless fed",
                        cx.now,
                        cx.pc,
                        (value >> STG0_SHIFT) & STG_MASK,
                        self.stage0_cycles()
                    );
                    cx.trace.note(&line);
                }
                self.rearm(cx);
            }
            WDTFEED => {
                if !self.wdt_unlocked() {
                    log::debug!("RTC_CNTL: feed dropped, RWDT locked");
                    return;
                }
                if value & FEED_BIT != 0 {
                    self.rearm(cx);
                }
            }
            SWD_CONF => {
                if !self.swd_unlocked() {
                    log::debug!("RTC_CNTL: swd_conf write dropped, SWD locked");
                    return;
                }
                self.regs.poke(SWD_CONF, value);
            }
            INT_ENA => {
                self.regs.poke(INT_ENA, value);
                self.update_lines(cx);
            }
            INT_RAW | INT_ST => {}
            INT_CLR => {
                let raw = self.regs.stored(INT_RAW) & !value;
                self.regs.poke(INT_RAW, raw);
                self.update_lines(cx);
            }
            other => self.regs.poke(other, value),
        }
    }
}

impl Peripheral for RtcCntl {
    fn name(&self) -> &'static str {
        "RTC_CNTL"
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

    fn on_event(&mut self, _id: EventId, cx: &mut BusCx<'_>) {
        if !self.rwdt_armed() {
            return;
        }
        let action = (self.regs.stored(WDTCONFIG0) >> STG0_SHIFT) & STG_MASK;
        if action == STG_INTERRUPT {
            self.regs.poke(INT_RAW, self.regs.stored(INT_RAW) | INT_WDT);
            self.update_lines(cx);
            return;
        }
        let source = match action {
            2 => "RTC_CNTL RWDT stage 0 (ResetCpu)",
            3 => "RTC_CNTL RWDT stage 0 (ResetCore)",
            _ => "RTC_CNTL RWDT stage 0 (ResetSystem)",
        };
        if !self.expired {
            self.expired = true;
            let line = format!(
                "cyc={} pc=0x{:08x} RTC_CNTL RWDT EXPIRED: {source}",
                cx.now, cx.pc
            );
            cx.trace.note(&line);
        }
        let at = cx.now;
        // A watchdog reset boots the app: the strap pin is not involved.
        cx.request(MachineRequest::Reset {
            source,
            at,
            strap: Strap::App,
        });
    }

    fn reg_name(&self, off: u32) -> Option<&'static str> {
        regs::RTC_CNTL.name(off)
    }

    fn reg_grade(&self, off: u32) -> Option<lp_emu_esp_common::periph::RegGrade> {
        self.regs.reg_grade(off)
    }

    fn save_state(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(RTC_CNTL_LEN as usize + 16);
        out.extend_from_slice(&(self.index as u64).to_le_bytes());
        out.extend_from_slice(&u32::from(self.expired).to_le_bytes());
        out.extend_from_slice(&u32::from(self.warned_cpu_rst).to_le_bytes());
        out.extend_from_slice(&self.regs.save_state());
        out
    }

    fn load_state(&mut self, bytes: &[u8]) {
        let mut r = Reader(bytes);
        let (Some(index), Some(expired), Some(warned)) = (r.u64(), r.u32(), r.u32()) else {
            log::warn!("RTC_CNTL: load_state blob too short, ignored");
            return;
        };
        self.index = index as usize;
        self.expired = expired != 0;
        self.warned_cpu_rst = warned != 0;
        self.regs.load_state(r.0);
        self.publish_stall();
    }
}

/// Where this block's power-on state differs from what the PAC states, and
/// why. `(offset, what this machine reads instead, why)`.
pub const DEVIATIONS: &[(u32, u32, &str)] = &[(
    RESET_STATE,
    0x0000_3041,
    "reset_state's two reset_cause fields are an input to the run, not a property of the part: \
     the machine asserts POWERON_RESET (1) for both cores, the value the ROM's \
     rtc_get_reset_reason (0x4004_456C) masks out with `extui 0,6` / `extui 6,6`",
)];

#[cfg(test)]
mod tests {
    use super::*;
    use lp_emu_esp_common::Sandbox;

    fn block() -> RtcCntl {
        RtcCntl::new(ResetCause::PowerOn, StallKey::new())
    }

    /// The PAC's reset for one register, from the generated table.
    fn pac(off: u32) -> u32 {
        regs::RTC_CNTL
            .reset(off)
            .expect("the PAC gives this register a non-zero reset")
    }

    /// esp-hal's `set_timeout(Stage0, …)` + `enable()` as the feeder does
    /// at boot, register for register (`rtc_cntl/mod.rs:566-619`).
    fn arm(sb: &mut Sandbox, r: &mut RtcCntl, hold: u32) {
        // set_timeout: unlock, hold, lock.
        sb.write(r, WDTWPROTECT, WDT_WKEY);
        let v = sb.read(r, WDTCONFIG1);
        sb.write(r, WDTCONFIG1, (v & !0xffff_ffff) | hold);
        sb.write(r, WDTWPROTECT, 0);
        // enable: unlock; flashboot_mod_en = 0 (a plain write); wdt_en |
        // pause_in_slp; stg0 = ResetSystem, reset lengths 7, stg1..3 off,
        // wdt_en; lock.
        sb.write(r, WDTWPROTECT, WDT_WKEY);
        sb.write(r, WDTCONFIG0, 0);
        let v = sb.read(r, WDTCONFIG0);
        sb.write(r, WDTCONFIG0, v | WDT_EN | (1 << 9));
        let v = sb.read(r, WDTCONFIG0) & !(0b111 << 28) & !(0b111 << 16) & !(0b111 << 13);
        sb.write(
            r,
            WDTCONFIG0,
            v | (4 << STG0_SHIFT) | (7 << 16) | (7 << 13) | WDT_EN,
        );
        sb.write(r, WDTWPROTECT, 0);
    }

    /// esp-hal's `feed()`: unlock, feed, lock.
    fn feed(sb: &mut Sandbox, r: &mut RtcCntl) {
        sb.write(r, WDTWPROTECT, WDT_WKEY);
        sb.write(r, WDTFEED, FEED_BIT);
        sb.write(r, WDTWPROTECT, 0);
    }

    /// `us_to_rtc_ticks(30 s) >> 1` at 136 kHz.
    const HOLD_30S: u32 = (30 * 136_000 / 2) as u32;

    #[test]
    fn reset_cause_reads_poweron_for_both_cores() {
        let mut sb = Sandbox::new();
        let mut r = block();
        assert_eq!(r.reg_name(RESET_STATE), Some("reset_state"));
        let word = sb.read(&mut r, RESET_STATE);
        assert_eq!(word & 0x3f, 1, "PRO: rtc_get_reset_reason(0)");
        assert_eq!((word >> 6) & 0x3f, 1, "APP: rtc_get_reset_reason(1)");
        assert_eq!(word & !0xfff, 0x3000, "the rest of the word is the PAC's");
        assert_eq!(ResetCause::PowerOn.rom_name(), "POWERON_RESET");
    }

    /// `internal_park_core` (`soc/esp32s3/cpu_control.rs:16-36`), write for
    /// write: `c1` first, then `c0`, and only the pair stalls.
    #[test]
    fn stall_key_needs_both_halves() {
        let mut sb = Sandbox::new();
        let mut r = block();
        let key = r.stall_key();
        assert_eq!(r.reg_name(SW_CPU_STALL), Some("sw_cpu_stall"));
        assert!(!key.stalled(1), "nothing written yet");

        let c1 = sb.read(&mut r, SW_CPU_STALL);
        sb.write(&mut r, SW_CPU_STALL, c1 | (0x21 << C1_APP_SHIFT));
        assert!(!key.stalled(1), "one half is not the key");
        let o0 = sb.read(&mut r, OPTIONS0);
        sb.write(&mut r, OPTIONS0, o0 | (0x02 << C0_APP_SHIFT));
        assert!(key.stalled(1), "both halves: (0x21 << 2) | 0x02 == 0x86");
        assert_eq!(key.key(1) & 0xff, STALLED);
        assert!(!key.stalled(0));

        let o0 = sb.read(&mut r, OPTIONS0);
        sb.write(&mut r, OPTIONS0, o0 & !(C0_MASK << C0_APP_SHIFT));
        assert!(!key.stalled(1), "clearing one half un-stalls");

        let c1 = sb.read(&mut r, SW_CPU_STALL);
        sb.write(&mut r, SW_CPU_STALL, c1 | (0x21 << C1_PRO_SHIFT));
        let o0 = sb.read(&mut r, OPTIONS0);
        sb.write(&mut r, OPTIONS0, o0 | (0x02 << C0_PRO_SHIFT));
        assert!(key.stalled(0));
        assert!(!key.stalled(1));
    }

    /// `esp_hal::init`'s disable, then the firmware's arm: 30 s of emulated
    /// time to expiry, exactly.
    #[test]
    fn the_thirty_second_boot_timeout_expires_thirty_seconds_after_arming() {
        let buf = lp_emu_esp_common::trace::SharedBuffer::new();
        let mut sb = Sandbox::new();
        sb.trace = lp_emu_esp_common::Trace::to_sink(Box::new(buf.clone()));
        let mut r = block();
        r.attached(5);
        assert_eq!(sb.read(&mut r, WDTWPROTECT), WDT_WKEY, "unlocked at reset");
        assert_eq!(sb.read(&mut r, WDTCONFIG0), pac(WDTCONFIG0));
        assert!(
            !r.rwdt_armed(),
            "the PAC's 0x0001_3214 has wdt_en clear: not armed at power-on"
        );
        // `Rtc::new` → `rwdt.disable()` (`lib.rs:755`).
        sb.write(&mut r, WDTWPROTECT, WDT_WKEY);
        sb.write(&mut r, WDTCONFIG0, 0);
        sb.write(&mut r, WDTWPROTECT, 0);
        assert_eq!(sb.sched.next_deadline(), None);
        // Locked: a write is dropped, not taken.
        sb.write(&mut r, WDTCONFIG0, 0xffff_ffff);
        assert_eq!(sb.read(&mut r, WDTCONFIG0), 0, "locked");

        sb.now = 1000;
        arm(&mut sb, &mut r, HOLD_30S);
        assert!(r.rwdt_armed());
        let expiry = 1000 + u64::from(HOLD_30S) * 2 * memmap::CPU_HZ / RC_SLOW_HZ;
        assert_eq!(sb.sched.next_deadline(), Some(expiry));
        assert_eq!(expiry - 1000, 30 * memmap::CPU_HZ, "30 s of emulated time");
        assert!(buf.lines().iter().any(|l| l.contains("RWDT ARMED")));

        // No feed: it bites, and asks the machine for a reset.
        sb.run_to(&mut r, expiry - 1);
        assert!(sb.request.is_none());
        sb.run_to(&mut r, expiry);
        assert_eq!(
            sb.request,
            Some(MachineRequest::Reset {
                source: "RTC_CNTL RWDT stage 0 (ResetSystem)",
                at: expiry,
                strap: Strap::App,
            })
        );
        assert!(buf.lines().iter().any(|l| l.contains("RWDT EXPIRED")));
    }

    /// The other direction: a fed watchdog does not fire. The firmware's
    /// tightened 8 s timeout, fed every second for a minute.
    #[test]
    fn a_fed_watchdog_does_not_fire_and_a_withheld_feed_does() {
        let mut sb = Sandbox::new();
        let mut r = block();
        r.attached(5);
        sb.now = 0;
        let hold_8s = (8 * 136_000 / 2) as u32;
        arm(&mut sb, &mut r, hold_8s);
        let period = 8 * memmap::CPU_HZ;
        assert_eq!(sb.sched.next_deadline(), Some(period));
        for second in 1..=60u64 {
            sb.run_to(&mut r, second * memmap::CPU_HZ);
            assert!(
                sb.request.is_none(),
                "fed within 8 s, no reset at {second} s"
            );
            feed(&mut sb, &mut r);
            assert_eq!(
                sb.sched.next_deadline(),
                Some(second * memmap::CPU_HZ + period),
                "a feed pushes expiry out by the full hold"
            );
            assert_eq!(sb.sched.live(), 1, "re-armed, not stacked");
        }
        // The io task goes silent: the feeder withholds, and 8 s later it
        // bites — the firmware's deliberate recovery path, visible.
        sb.run_to(&mut r, 60 * memmap::CPU_HZ + period - 1);
        assert!(sb.request.is_none());
        sb.run_to(&mut r, 60 * memmap::CPU_HZ + period);
        assert!(matches!(
            sb.request,
            Some(MachineRequest::Reset { at, .. }) if at == 60 * memmap::CPU_HZ + period
        ));
        // A feed with the key locked is dropped — the disable-that-did-
        // nothing failure, in the feed direction.
        let mut sb2 = Sandbox::new();
        let mut r2 = block();
        r2.attached(5);
        arm(&mut sb2, &mut r2, hold_8s);
        sb2.now = 4 * memmap::CPU_HZ;
        sb2.write(&mut r2, WDTFEED, FEED_BIT);
        assert_eq!(
            sb2.sched.next_deadline(),
            Some(period),
            "a locked feed moved nothing"
        );
        assert_eq!(sb2.read(&mut r2, WDTFEED), 0, "a pulse reads 0");
    }

    #[test]
    fn an_interrupt_stage_raises_rtc_core_instead_of_resetting() {
        let mut sb = Sandbox::new();
        let mut r = block();
        r.attached(5);
        sb.write(&mut r, WDTWPROTECT, WDT_WKEY);
        sb.write(&mut r, WDTCONFIG1, 136);
        sb.write(&mut r, WDTCONFIG0, WDT_EN | (STG_INTERRUPT << STG0_SHIFT));
        sb.write(&mut r, INT_ENA, INT_WDT);
        sb.write(&mut r, WDTWPROTECT, 0);
        let due = sb.sched.next_deadline().expect("armed");
        sb.run_to(&mut r, due);
        assert!(sb.irq.level(source::RTC_CORE));
        assert_eq!(sb.read(&mut r, INT_ST), INT_WDT);
        assert!(sb.request.is_none());
        sb.write(&mut r, INT_CLR, INT_WDT);
        assert!(!sb.irq.level(source::RTC_CORE));
        assert_eq!(sb.read(&mut r, INT_CLR), 0);
    }

    /// The super-watchdog: its own key, and `esp_hal::init`'s disable.
    #[test]
    fn the_super_watchdog_has_its_own_key() {
        let mut sb = Sandbox::new();
        let mut r = block();
        assert_eq!(sb.read(&mut r, SWD_WPROTECT), SWD_WKEY, "unlocked at reset");
        assert_eq!(sb.read(&mut r, SWD_CONF), 0x04b0_0000, "the PAC's reset");
        // Lock it with the RWDT's key, which is the wrong one here.
        sb.write(&mut r, SWD_WPROTECT, WDT_WKEY);
        sb.write(&mut r, SWD_CONF, 1 << 31);
        assert_eq!(
            sb.read(&mut r, SWD_CONF),
            0x04b0_0000,
            "not the SWD's key: dropped"
        );
        // esp-hal's `Swd::disable`: unlock with 0x8F1D_312A, auto_feed_en
        // = 1, lock.
        sb.write(&mut r, SWD_WPROTECT, SWD_WKEY);
        sb.write(&mut r, SWD_CONF, 1 << 31);
        sb.write(&mut r, SWD_WPROTECT, 0);
        assert_eq!(sb.read(&mut r, SWD_CONF), 1 << 31);
        sb.write(&mut r, SWD_CONF, 0);
        assert_eq!(sb.read(&mut r, SWD_CONF), 1 << 31, "locked");
    }

    #[test]
    fn a_cpu_software_reset_is_reported_not_performed() {
        let buf = lp_emu_esp_common::trace::SharedBuffer::new();
        let mut sb = Sandbox::new();
        sb.trace = lp_emu_esp_common::Trace::to_sink(Box::new(buf.clone()));
        let mut r = block();
        let o0 = sb.read(&mut r, OPTIONS0);
        sb.write(&mut r, OPTIONS0, o0 | (1 << 5));
        assert_eq!(buf.lines().len(), 1);
        assert!(buf.lines()[0].contains("sw_{app,pro}cpu_rst"));
        assert_eq!(
            sb.read(&mut r, OPTIONS0),
            o0,
            "a write-only pulse, not remembered"
        );
    }

    #[test]
    fn the_only_deviation_from_the_pacs_resets_is_the_listed_one() {
        let r = block();
        let mut unlisted = Vec::new();
        for (off, _) in regs::RTC_CNTL.entries {
            if *off >= RTC_CNTL_LEN {
                continue;
            }
            let want = regs::RTC_CNTL.reset(*off).unwrap_or(0);
            let got = r.regs.stored(*off);
            if got == want {
                continue;
            }
            match DEVIATIONS.iter().find(|(o, _, _)| o == off) {
                Some((_, expected, why)) => {
                    assert_eq!(
                        got, *expected,
                        "+{off:#05x} is listed but reads {got:#010x}"
                    );
                    assert!(why.len() > 40, "+{off:#05x} has no reason");
                }
                None => unlisted.push(format!(
                    "  +{off:#05x} {}: reads {got:#010x}, the PAC says {want:#010x}",
                    regs::RTC_CNTL.name(*off).unwrap_or("?")
                )),
            }
        }
        assert!(
            unlisted.is_empty(),
            "these registers do not read what the PAC says and are not on DEVIATIONS:\n{}",
            unlisted.join("\n")
        );
        for (off, _, _) in DEVIATIONS {
            assert_ne!(r.regs.stored(*off), regs::RTC_CNTL.reset(*off).unwrap_or(0));
        }
    }

    /// The clock and store registers the boot leans on, at the PAC's
    /// resets and at the S3's offsets.
    #[test]
    fn the_clock_and_store_registers_are_accept_and_remember_at_the_s3_offsets() {
        let mut sb = Sandbox::new();
        let mut r = block();
        assert_eq!(r.reg_name(0x074), Some("clk_conf"));
        assert_eq!(sb.read(&mut r, 0x074), 0x1158_321c, "the PAC's reset");
        assert_eq!(r.reg_name(ANA_CONF), Some("ana_conf"));
        assert_eq!(sb.read(&mut r, ANA_CONF), 0x0044_0000);
        assert_eq!(r.reg_name(STORE1), Some("store1"));
        // `calibrate_rtc_slow_clock` stores the period here; the RWDT's
        // `set_timeout` divides by it.
        sb.write(&mut r, STORE1, 3_855_052);
        assert_eq!(sb.read(&mut r, STORE1), 3_855_052);
        assert_eq!(r.reg_name(0x1fc), Some("date"));
        assert_eq!(sb.read(&mut r, 0x1fc), 0x0210_1271);
        // The classic's offsets are NOT this chip's: +0x30 is `timer6`
        // here, and +0xac is the feed, not the stall register.
        assert_eq!(r.reg_name(0x030), Some("timer6"));
        assert_eq!(r.reg_name(0x0ac), Some("wdtfeed"));
    }

    #[test]
    fn the_state_round_trips() {
        let mut sb = Sandbox::new();
        let mut r = block();
        r.attached(5);
        sb.write(&mut r, SW_CPU_STALL, 0x21 << C1_APP_SHIFT);
        sb.write(&mut r, OPTIONS0, 0x02);
        arm(&mut sb, &mut r, HOLD_30S);
        let blob = r.save_state();

        let other_key = StallKey::new();
        let mut other = RtcCntl::new(ResetCause::PowerOn, other_key.clone());
        assert!(!other_key.stalled(1));
        other.load_state(&blob);
        assert!(other_key.stalled(1), "the key is republished on restore");
        assert!(other.rwdt_armed());
        assert_eq!(other.regs.stored(WDTCONFIG1), HOLD_30S);
        assert_eq!(other.save_state(), blob);
    }
}
