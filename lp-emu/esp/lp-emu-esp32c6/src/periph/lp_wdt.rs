//! `LP_WDT` at `0x600B_1C00` — the RTC watchdog (RWDT) and the super
//! watchdog (SWD). The first honest watchdog: **our firmware arms the
//! RWDT** (`WatchdogFeeder::start(rwdt, 0)`, `main.rs:251`), so a firmware
//! that stops feeding dies here as it does on silicon.
//!
//! Registers (`regs::LP_WDT`, discovery §3):
//!
//! - `wdtconfig0..4` and `wdtfeed` take writes only while `wdtwprotect`
//!   holds `0x50D8_3AA1` (`rtc_cntl/mod.rs:558-566`); `swd_conf` only
//!   while `swd_wprotect` does (the C6 key is the same, `:656-666`).
//! - `wdtconfig0`: `wdt_en` bit 31, `wdt_stg0` bits 28:30 (0 off,
//!   1 interrupt, 2 reset CPU, 3 reset core, 4 reset system), `wdt_stg1..3`
//!   below it. `wdtconfig1` is stage 0's hold count, in RWDT ticks.
//! - `wdtfeed` bit 31 restarts the count.
//! - `int_raw/int_st/int_ena/int_clr` bit 0 = `wdt`; the source is
//!   `LP_WDT` (18).
//!
//! # Stage 0, modelled
//!
//! esp-hal's `set_timeout` converts microseconds to slow-clock ticks with
//! the calibrated period, then shifts **right by `1 + WDT_DELAY_SEL`**
//! (`rtc_cntl/mod.rs:600-620`; the eFuse field reads 0 here). ESP-IDF's
//! `wdt_hal` does the same for the RWDT, which says the RWDT counts at
//! `slow_clk / 2^(1 + WDT_DELAY_SEL)`. So stage 0 expires
//! `hold * 2 / RC_SLOW_HZ` seconds after the last feed or arm — **modeled**
//! from that shift and from [`super::RC_SLOW_HZ`]; nothing here measured the
//! RWDT's clock. With the firmware's 30 s boot timeout and the 136 kHz
//! slow clock, `hold` is 2,040,000 and expiry is 30 s, which is the number
//! the firmware asked for.
//!
//! On expiry, stage 0's action decides: `Interrupt` sets `int_raw.wdt`;
//! any reset action asks the machine for a reset through
//! [`lp_emu_esp_common::MachineRequest::Reset`], which the run reports as
//! [`crate::machine::Outcome::Reset`] — the emulator cannot yet reset the
//! chip (M7), and "the RWDT expired at cycle N" is the answer a bring-up
//! wants anyway. Stages 1..3 are stored, not modelled: the firmware leaves
//! them `Off`.
//!
//! The SWD is disabled by `esp_hal::init` (auto-feed on, `:667-674`); its
//! expiry is not modelled.
//!
//! `0x054` is undocumented in the PAC and hit 9R/9W by the vendor
//! emulator's trace; it is accepted and named `reserved_054`.

use lp_emu_core::sched::EventId;
use lp_emu_esp_common::regfile::{lane_of, merge_lane};
use lp_emu_esp_common::{BusCx, MachineRequest, Peripheral, RegFile, Strap, Width, event_id};

use super::systimer::Reader;
use super::{RC_SLOW_HZ, WDT_WKEY};
use crate::memmap;
use crate::regs::{self, source};

const WDTCONFIG0: u32 = 0x00;
const WDTCONFIG1: u32 = 0x04;
const WDTCONFIG4: u32 = 0x10;
const WDTFEED: u32 = 0x14;
const WDTWPROTECT: u32 = 0x18;
const SWD_CONF: u32 = 0x1c;
const SWD_WPROTECT: u32 = 0x20;
const INT_RAW: u32 = 0x24;
const INT_ST: u32 = 0x28;
const INT_ENA: u32 = 0x2c;
const INT_CLR: u32 = 0x30;
const RESERVED_054: u32 = 0x54;

const WDTCONFIG0_RESET: u32 = 0x0001_3214;
const SWD_CONF_RESET: u32 = 0x12c0_0000;
const WDT_EN: u32 = 1 << 31;
const STG0_SHIFT: u32 = 28;
const STG_MASK: u32 = 0b111;
const FEED_BIT: u32 = 1 << 31;

/// Stage actions (`RwdtStageAction`, `rtc_cntl/mod.rs:456-467`).
const STG_OFF: u32 = 0;
const STG_INTERRUPT: u32 = 1;

/// The RWDT's tick is the slow clock divided by this (see the module docs).
pub const RWDT_CLOCK_DIVIDER: u64 = 2;

const EV_STAGE0: u16 = 0;

/// The RTC watchdog block.
#[derive(Debug)]
pub struct LpWdt {
    index: usize,
    regs: RegFile,
    /// Set once stage 0 has expired with a reset action, so the trace says
    /// so once.
    expired: bool,
}

impl Default for LpWdt {
    fn default() -> Self {
        Self::new()
    }
}

impl LpWdt {
    pub fn new() -> Self {
        Self {
            index: 0,
            regs: RegFile::new("LP_WDT", 0x400)
                .with_names(regs::LP_WDT)
                .with_reset(WDTCONFIG0, WDTCONFIG0_RESET)
                .with_reset(SWD_CONF, SWD_CONF_RESET),
            expired: false,
        }
    }

    fn wdt_unlocked(&self) -> bool {
        self.regs.stored(WDTWPROTECT) == WDT_WKEY
    }

    fn swd_unlocked(&self) -> bool {
        self.regs.stored(SWD_WPROTECT) == WDT_WKEY
    }

    /// `wdt_en` with a non-`Off` stage 0.
    pub fn armed(&self) -> bool {
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
        cx.irq.set_level(source::LP_WDT, st & 1 != 0);
    }

    fn rearm(&mut self, cx: &mut BusCx<'_>) {
        let ev = event_id(self.index, EV_STAGE0);
        cx.sched.cancel(ev);
        if self.armed() {
            cx.sched.schedule_in(cx.now, self.stage0_cycles(), ev);
        }
    }

    fn read_word(&self, off: u32) -> u32 {
        match off {
            WDTFEED | INT_CLR => 0,
            INT_ST => self.regs.stored(INT_RAW) & self.regs.stored(INT_ENA),
            other => self.regs.stored(other),
        }
    }

    fn write_word(&mut self, off: u32, value: u32, cx: &mut BusCx<'_>) {
        match off {
            WDTCONFIG0..=WDTCONFIG4 => {
                if !self.wdt_unlocked() {
                    log::debug!("LP_WDT: write to +{off:#05x} dropped, locked");
                    return;
                }
                self.regs.poke(off, value);
                self.rearm(cx);
            }
            WDTFEED => {
                if !self.wdt_unlocked() {
                    log::debug!("LP_WDT: feed dropped, locked");
                    return;
                }
                if value & FEED_BIT != 0 {
                    self.rearm(cx);
                }
            }
            SWD_CONF => {
                if !self.swd_unlocked() {
                    log::debug!("LP_WDT: swd_conf write dropped, locked");
                    return;
                }
                self.regs.poke(SWD_CONF, value);
            }
            INT_ENA => {
                self.regs.poke(INT_ENA, value & 1);
                self.update_lines(cx);
            }
            INT_RAW | INT_ST => {}
            INT_CLR => {
                let raw = self.regs.stored(INT_RAW) & !(value & 1);
                self.regs.poke(INT_RAW, raw);
                self.update_lines(cx);
            }
            other => self.regs.poke(other, value),
        }
    }
}

impl Peripheral for LpWdt {
    fn name(&self) -> &'static str {
        "LP_WDT"
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
        if !self.armed() {
            return;
        }
        let action = (self.regs.stored(WDTCONFIG0) >> STG0_SHIFT) & STG_MASK;
        if action == STG_INTERRUPT {
            self.regs.poke(INT_RAW, self.regs.stored(INT_RAW) | 1);
            self.update_lines(cx);
            return;
        }
        let source = match action {
            2 => "LP_WDT stage 0 (ResetCpu)",
            3 => "LP_WDT stage 0 (ResetCore)",
            _ => "LP_WDT stage 0 (ResetSystem)",
        };
        if !self.expired {
            self.expired = true;
            let line = format!(
                "cyc={} pc=0x{:08x} LP_WDT RWDT EXPIRED: {source}",
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
        if off & !3 == RESERVED_054 {
            return Some("reserved_054");
        }
        regs::LP_WDT.name(off)
    }

    fn save_state(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(0x400 + 16);
        out.extend_from_slice(&(self.index as u64).to_le_bytes());
        out.extend_from_slice(&u32::from(self.expired).to_le_bytes());
        out.extend_from_slice(&self.regs.save_state());
        out
    }

    fn load_state(&mut self, bytes: &[u8]) {
        let mut r = Reader(bytes);
        let (Some(index), Some(expired)) = (r.u64(), r.u32()) else {
            log::warn!("LP_WDT: load_state blob too short, ignored");
            return;
        };
        self.index = index as usize;
        self.expired = expired != 0;
        self.regs.load_state(r.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_emu_esp_common::Sandbox;

    /// esp-hal's `Rwdt::enable` + `set_timeout(Stage0, 30 s)` as the
    /// feeder does at boot: unlock, config, lock. `hold` is what
    /// `us_to_rtc_ticks(30_000_000) >> 1` gives at 136 kHz.
    const HOLD_30S: u32 = 30 * 136_000 / 2;

    fn arm(sb: &mut Sandbox, w: &mut LpWdt, hold: u32) {
        sb.write(w, WDTWPROTECT, WDT_WKEY);
        sb.write(w, WDTCONFIG1, hold);
        // wdt_en, stg0 = ResetSystem(4), reset lengths 7/7, pause_in_slp.
        sb.write(
            w,
            WDTCONFIG0,
            WDT_EN | (4 << STG0_SHIFT) | (7 << 16) | (7 << 13) | (1 << 9),
        );
        sb.write(w, WDTWPROTECT, 0);
    }

    fn feed(sb: &mut Sandbox, w: &mut LpWdt) {
        sb.write(w, WDTWPROTECT, WDT_WKEY);
        sb.write(w, WDTFEED, FEED_BIT);
        sb.write(w, WDTWPROTECT, 0);
    }

    #[test]
    fn disabled_at_reset_and_write_protected() {
        let mut sb = Sandbox::new();
        let mut w = LpWdt::new();
        assert_eq!(sb.read(&mut w, WDTCONFIG0), WDTCONFIG0_RESET);
        assert!(!w.armed());
        sb.write(&mut w, WDTCONFIG0, WDT_EN | (4 << STG0_SHIFT));
        assert_eq!(sb.read(&mut w, WDTCONFIG0), WDTCONFIG0_RESET, "locked");
        assert!(sb.sched.next_deadline().is_none());
        // esp_hal::init's disable: unlock, clear, lock.
        sb.write(&mut w, WDTWPROTECT, WDT_WKEY);
        sb.write(&mut w, WDTCONFIG0, 0);
        sb.write(&mut w, WDTWPROTECT, 0);
        assert_eq!(sb.read(&mut w, WDTCONFIG0), 0);
        // The SWD: its own key, auto-feed on.
        sb.write(&mut w, SWD_CONF, 1 << 18);
        assert_eq!(sb.read(&mut w, SWD_CONF), SWD_CONF_RESET, "locked");
        sb.write(&mut w, SWD_WPROTECT, WDT_WKEY);
        sb.write(&mut w, SWD_CONF, 1 << 18);
        sb.write(&mut w, SWD_WPROTECT, 0);
        assert_eq!(sb.read(&mut w, SWD_CONF), 1 << 18);
    }

    #[test]
    fn the_thirty_second_boot_timeout_expires_thirty_seconds_after_arming() {
        let mut sb = Sandbox::new();
        let mut w = LpWdt::new();
        w.attached(6);
        sb.now = 1000;
        arm(&mut sb, &mut w, HOLD_30S);
        assert!(w.armed());
        let expiry = 1000 + u64::from(HOLD_30S) * 2 * memmap::CPU_HZ / RC_SLOW_HZ;
        assert_eq!(sb.sched.next_deadline(), Some(expiry));
        assert_eq!(expiry - 1000, 30 * memmap::CPU_HZ, "30 s of emulated time");
    }

    #[test]
    fn a_feed_pushes_expiry_out_and_a_missed_feed_asks_for_a_reset() {
        let buf = lp_emu_esp_common::trace::SharedBuffer::new();
        let mut sb = Sandbox::new();
        sb.trace = lp_emu_esp_common::Trace::to_sink(Box::new(buf.clone()));
        let mut w = LpWdt::new();
        sb.now = 0;
        arm(&mut sb, &mut w, 136_000 / 2 * 8); // 8 s
        let period = 8 * memmap::CPU_HZ;
        assert_eq!(sb.sched.next_deadline(), Some(period));
        sb.run_to(&mut w, 5 * memmap::CPU_HZ);
        feed(&mut sb, &mut w);
        assert_eq!(sb.sched.next_deadline(), Some(5 * memmap::CPU_HZ + period));
        assert_eq!(sb.sched.live(), 1);
        assert!(sb.request.is_none());
        // No more feeds: it bites.
        sb.run_to(&mut w, 5 * memmap::CPU_HZ + period);
        assert_eq!(
            sb.request,
            Some(MachineRequest::Reset {
                source: "LP_WDT stage 0 (ResetSystem)",
                at: 5 * memmap::CPU_HZ + period,
                strap: Strap::App,
            })
        );
        assert_eq!(buf.lines().len(), 1);
        assert!(buf.lines()[0].contains("RWDT EXPIRED"));
    }

    #[test]
    fn an_interrupt_stage_raises_the_source_instead() {
        let mut sb = Sandbox::new();
        let mut w = LpWdt::new();
        sb.write(&mut w, WDTWPROTECT, WDT_WKEY);
        sb.write(&mut w, WDTCONFIG1, 136);
        sb.write(&mut w, WDTCONFIG0, WDT_EN | (STG_INTERRUPT << STG0_SHIFT));
        sb.write(&mut w, WDTWPROTECT, 0);
        sb.write(&mut w, INT_ENA, 1);
        sb.run_to(&mut w, sb.sched.next_deadline().unwrap());
        assert!(sb.irq.level(source::LP_WDT));
        assert_eq!(sb.read(&mut w, INT_ST), 1);
        assert!(sb.request.is_none());
        sb.write(&mut w, INT_CLR, 1);
        assert!(!sb.irq.level(source::LP_WDT));
    }

    #[test]
    fn disarming_cancels_and_the_state_round_trips() {
        let mut sb = Sandbox::new();
        let mut w = LpWdt::new();
        arm(&mut sb, &mut w, HOLD_30S);
        sb.write(&mut w, WDTWPROTECT, WDT_WKEY);
        sb.write(&mut w, WDTCONFIG0, 0);
        assert_eq!(sb.sched.next_deadline(), None);
        assert_eq!(w.reg_name(RESERVED_054), Some("reserved_054"));
        assert_eq!(w.reg_name(WDTFEED), Some("wdtfeed"));
        let blob = w.save_state();
        let mut other = LpWdt::new();
        other.load_state(&blob);
        assert_eq!(other.regs.stored(WDTCONFIG1), HOLD_30S);
    }
}
