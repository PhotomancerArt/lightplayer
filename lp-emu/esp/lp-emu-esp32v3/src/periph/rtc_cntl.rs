//! `RTC_CNTL` at `0x3FF4_8000` — the reset cause, the CPU stall key, the
//! RWDT, and the clock and store registers the boot leans on.
//!
//! # The reset cause is an input to the run, not a property of the part
//!
//! The second strict stop of the direct load was the mask ROM's
//! `rtc_get_reset_reason+0xb` reading `reset_state` (`+0x34`):
//!
//! ```text
//! 400081df:  l32i.n  a2, a8, 0        ; RTC_CNTL + 0x34
//! 400081e1:  extui   a2, a2, 0, 6     ; PRO: bits 5:0
//! 400081ed:  extui   a2, a2, 6, 6     ; APP: bits 11:6
//! ```
//!
//! The PAC's reset for the register is `0x0000_3000` — both cause fields
//! zero, because an SVD cannot know why a chip is starting. This machine
//! asserts [`crate::loader::ResetCause`] into **both** fields, because a
//! power-on resets both cores and the ROM's own `rtc_get_reset_reason(1)`
//! would otherwise answer "no reason" for the APP core. `1` is
//! `POWERON_RESET`, the code L0's silicon banner printed as
//! `rst:0x1 (POWERON_RESET)` (`../bench.md`).
//!
//! **That is the one deviation from the PAC this block carries**, it was
//! P3's one deviation too, and it is listed in [`DEVIATIONS`].
//!
//! # Both halves of the stall key
//!
//! A core is stalled only when **two** fields in **two** registers hold the
//! right values at once — esp-hal spells the check out
//! (`soc/esp32/cpu_control.rs:57-81`):
//!
//! ```text
//! // sw_stall_appcpu_c1[5:0], sw_stall_appcpu_c0[1:0]} == 0x86 will stall APP CPU
//! // sw_stall_procpu_c1[5:0], sw_stall_procpu_c0[1:0]} == 0x86 will stall PRO CPU
//! let is_stalled = (c1 << 2) | c0;
//! …
//! is_stalled != 0x86
//! ```
//!
//! and parks a core by writing `c1 = 0x21` then `c0 = 0x02`
//! (`internal_park_core`, `:16-36`) — `(0x21 << 2) | 0x02 == 0x86`. The two
//! halves live at
//!
//! | field | register | bits |
//! |---|---|---|
//! | `sw_stall_appcpu_c0` | `options0` (`+0x00`) | 0:1 |
//! | `sw_stall_procpu_c0` | `options0` (`+0x00`) | 2:3 |
//! | `sw_stall_appcpu_c1` | `sw_cpu_stall` (`+0xac`) | 20:25 |
//! | `sw_stall_procpu_c1` | `sw_cpu_stall` (`+0xac`) | 26:31 |
//!
//! so a model that watched one register would say "running" through the
//! whole two-write sequence in one direction and "stalled" through it in the
//! other. This block computes the pair and publishes it through
//! [`StallKey`], a handle the machine holds: `Machine::core_stalled` ORs it
//! with its own field, and **P4 adds the third input**,
//! `DPORT.appcpu_ctrl_c.appcpu_runstall` (plus
//! `appcpu_ctrl_b.appcpu_clkgate_en`, which esp-hal checks first). The seam
//! is deliberately a plain shared cell rather than a peripheral-to-
//! peripheral call: two blocks in two phases each own one input to one
//! question the *machine* answers.
//!
//! # The RWDT
//!
//! `esp_hal::init` disables it (`Rtc::new` →
//! `lp-fw/fw-esp32v3/src/board/esp32v3/init.rs:47-50`): unlock with
//! [`super::WDT_WKEY`], clear `wdt_en`, lock. The key is **honoured** —
//! a `wdtconfig0` write without it is dropped — through the same
//! [`wdt_write`](lp_emu_esp_common::engine::timg::wdt_write) gate the MWDTs
//! use, because "the disable silently did nothing" is exactly the failure a
//! lenient model hides.
//!
//! **Expiry is not modelled**, here or anywhere in `engine::timg`: a write
//! that arms the RWDT leaves a `WDT ARMED` note in the trace so an image
//! that does arm one is visible rather than silently unprotected. In M3 the
//! shipped image never arms it and the RWDT never fires.
//!
//! ⚠️ `wdtwprotect` resets to the key itself in the PAC (`0x50D8_3AA1`), so
//! the RWDT comes out of reset **unlocked** — the same surprise TIMG0's MWDT
//! carries, and the part's, not the model's.
//!
//! # The clock and store registers, each with its pin
//!
//! | register | who touches it | pin |
//! |---|---|---|
//! | `clk_conf` (`+0x70`) | `detect_xtal_freq` sets and clears `dig_clk8m_d256_en` around the calibration | `soc/esp32/clocks.rs:141-146, 152-154` |
//! | `store1` (`+0x50`) | `calibrate_rtc_slow_clock` stashes the slow-clock period | `clock/mod.rs`, and P3's §3.1 (it held **0** before this phase) |
//! | `store4` (`+0xb0`) | the XTAL frequency in two 16-bit copies, for the ROM's patched `rtc_clk_xtal_freq_get` | `soc/esp32/clocks.rs:186-198` |
//! | `store6`/`store7` (`+0xb8`/`+0xbc`) | the ROM's `rtc_boot_control` — on the ROM-up path | P3's ledger §4.3 |
//! | `ana_conf` (`+0x30`) | the analog force bits around the BBPLL | PAC reset `0x0080_0000` |
//! | `options0` force bits | `xtl_force_pu`, `bbpll_force_pd`, the bias bits | PAC field docs |
//!
//! All of them are accept-and-remember at the PAC's resets: nothing in the
//! image reads a bit back that hardware would have changed, and the two
//! stores that matter (`store1`, `store4`) are written by the firmware with
//! values this phase's TIMG calibration now makes correct.
//!
//! # `options0.sw_sys_rst` is reported, not performed
//!
//! `0x8000_0000` into `options0` is the ROM's own software reset
//! (`_rtc_trigger_sw_system_reset` at `0x4000_FDC7`, which the eFuse
//! anti-glitch check jumps to when its comparison fails). This machine has
//! no boot chain to restart in M3, so the write leaves a trace note and a
//! warning and the run carries on into the `ill.n` the ROM puts after it —
//! which is a *fault*, i.e. loud. Performing the reset is M7's (the boot
//! chain) and the C6's `MachineRequest::Reset` is the shape it would take.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use lp_emu_esp_common::engine::timg::{WdtWrite, wdt_write};
use lp_emu_esp_common::regfile::{lane_of, merge_lane};
use lp_emu_esp_common::{BusCx, Peripheral, RegFile, Width};

use super::WDT_WKEY;
use crate::loader::ResetCause;
use crate::regs;

/// The block's aperture, **tight**: the generated table runs to `+0x13c`
/// (`date`); the PAC's next block (`RTC_IO`) is at `+0x400`. Tight so that
/// an access into the gap is a strict stop naming an undocumented offset,
/// not a silent zero.
pub const RTC_CNTL_LEN: u32 = 0x140;

/// `options0` — one half of the stall key, the analog force bits, and the
/// software resets.
pub const OPTIONS0: u32 = 0x000;
/// `ana_conf` — the analog force bits around the BBPLL; accept-and-remember
/// at the PAC's `0x0080_0000`.
pub const ANA_CONF: u32 = 0x030;
/// `reset_state` — the ROM's `rtc_get_reset_reason`, and this block's one
/// deviation from the PAC.
pub const RESET_STATE: u32 = 0x034;
const WDTCONFIG0: u32 = 0x08c;
const WDTFEED: u32 = 0x0a0;
const WDTWPROTECT: u32 = 0x0a4;
const SW_CPU_STALL: u32 = 0x0ac;

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
/// (`soc/esp32/cpu_control.rs:60-81`).
pub const STALLED: u32 = 0x86;

/// `options0.sw_sys_rst` — the ROM's software reset.
const SW_SYS_RST: u32 = 1 << 31;

const WDT_EN: u32 = 1 << 31;

/// The CPU stall key as this block computes it, shared with the machine.
///
/// One word: the APP core's eight-bit key in bits 0:7 and the PRO core's in
/// bits 8:15. A plain atomic rather than a channel, because the question —
/// "is core `n` held?" — is asked by the run loop between slices and
/// answered by whatever the guest last wrote, with no ordering to preserve.
///
/// P4 wires `DPORT.appcpu_ctrl_c.appcpu_runstall` alongside it; this handle
/// carries only RTC_CNTL's two halves and says so.
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
    /// with its own field and, from P4, with DPORT's `appcpu_runstall`.
    pub fn stalled(&self, core: usize) -> bool {
        self.key(core) & 0xff == STALLED
    }

    fn store(&self, pro: u32, app: u32) {
        self.0
            .store(((pro & 0xff) << 8) | (app & 0xff), Ordering::Relaxed);
    }
}

/// The classic's RTC controller.
#[derive(Debug)]
pub struct RtcCntl {
    regs: RegFile,
    stall: StallKey,
    warned_sys_rst: bool,
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
            regs,
            stall,
            warned_sys_rst: false,
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

    fn read_word(&self, off: u32) -> u32 {
        match off {
            // The pulses: write-only in the PAC, and a read of one must not
            // look like a request that never finished.
            WDTFEED => 0,
            other => self.regs.stored(other),
        }
    }

    fn write_word(&mut self, off: u32, value: u32, cx: &mut BusCx<'_>) {
        match off {
            OPTIONS0 => {
                if value & SW_SYS_RST != 0 && !self.warned_sys_rst {
                    self.warned_sys_rst = true;
                    let line = format!(
                        "cyc={} pc=0x{:08x} RTC_CNTL options0.sw_sys_rst written (a software \
                         system reset; this machine has no boot chain to restart in M3, so the \
                         run carries on)",
                        cx.now, cx.pc
                    );
                    cx.trace.note(&line);
                    log::warn!(
                        "RTC_CNTL: sw_sys_rst written at pc={:#010x}; the reset is reported, \
                         not performed",
                        cx.pc
                    );
                }
                // `sw_sys_rst` and the two `sw_*_rst` bits are write-only
                // pulses in the PAC; everything else in the word is
                // accept-and-remember, the stall halves included.
                self.regs.poke(OPTIONS0, value & !SW_SYS_RST);
                self.publish_stall();
            }
            SW_CPU_STALL => {
                self.regs.poke(SW_CPU_STALL, value);
                self.publish_stall();
            }
            WDTCONFIG0..=WDTFEED => {
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
                if off == WDTFEED {
                    return;
                }
                self.regs.poke(off, value);
                if verdict == WdtWrite::ArmedNow {
                    let line = format!(
                        "cyc={} pc=0x{:08x} RTC_CNTL RWDT ARMED (its expiry is not modelled)",
                        cx.now, cx.pc
                    );
                    cx.trace.note(&line);
                    log::warn!("RTC_CNTL: RWDT armed; its expiry is not modelled");
                }
            }
            other => self.regs.poke(other, value),
        }
    }
}

impl Peripheral for RtcCntl {
    fn name(&self) -> &'static str {
        "RTC_CNTL"
    }

    fn read(&mut self, off: u32, width: Width, _cx: &mut BusCx<'_>) -> u32 {
        lane_of(self.read_word(off & !3), off, width)
    }

    fn write(&mut self, off: u32, width: Width, value: u32, cx: &mut BusCx<'_>) {
        let word = off & !3;
        let merged = merge_lane(self.read_word(word), off, width, value);
        self.write_word(word, merged, cx);
    }

    fn reg_name(&self, off: u32) -> Option<&'static str> {
        regs::RTC_CNTL.name(off)
    }

    fn reg_grade(&self, off: u32) -> Option<lp_emu_esp_common::periph::RegGrade> {
        self.regs.reg_grade(off)
    }

    fn save_state(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(RTC_CNTL_LEN as usize + 4);
        out.extend_from_slice(&u32::from(self.warned_sys_rst).to_le_bytes());
        out.extend_from_slice(&self.regs.save_state());
        out
    }

    fn load_state(&mut self, bytes: &[u8]) {
        if bytes.len() < 4 {
            log::warn!("RTC_CNTL: load_state blob too short, ignored");
            return;
        }
        self.warned_sys_rst = u32::from_le_bytes(bytes[..4].try_into().expect("four bytes")) != 0;
        self.regs.load_state(&bytes[4..]);
        self.publish_stall();
    }
}

/// Where this block's power-on state differs from what the PAC states, and
/// why — the same list `crate::periph::accept` keeps for the accept blocks,
/// held here because the block is a view now. `(offset, what this machine
/// reads instead, why)`.
pub const DEVIATIONS: &[(u32, u32, &str)] = &[(
    RESET_STATE,
    0x0000_3041,
    "reset_state's two reset_cause fields are an input to the run, not a property of the part: \
     the machine asserts POWERON_RESET (1) for both cores, the value the ROM's \
     rtc_get_reset_reason masks out (extui 0,6 / 6,6) and the value the silicon banner printed",
)];

#[cfg(test)]
mod tests {
    use super::*;
    use lp_emu_esp_common::Sandbox;

    fn block() -> RtcCntl {
        RtcCntl::new(ResetCause::PowerOn, StallKey::new())
    }

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

    /// `internal_park_core` (`soc/esp32/cpu_control.rs:16-36`), write for
    /// write: `c1` first, then `c0`, and only the pair stalls.
    #[test]
    fn stall_key_needs_both_halves() {
        let mut sb = Sandbox::new();
        let mut r = block();
        let key = r.stall_key();
        assert_eq!(r.reg_name(SW_CPU_STALL), Some("sw_cpu_stall"));
        assert!(!key.stalled(1), "nothing written yet");

        // Park the APP core: `sw_stall_appcpu_c1 = 0x21`, then
        // `sw_stall_appcpu_c0 = 0x02`.
        let c1 = sb.read(&mut r, SW_CPU_STALL);
        sb.write(&mut r, SW_CPU_STALL, c1 | (0x21 << C1_APP_SHIFT));
        assert!(
            !key.stalled(1),
            "one half is not the key: the core is still running"
        );
        let o0 = sb.read(&mut r, OPTIONS0);
        sb.write(&mut r, OPTIONS0, o0 | (0x02 << C0_APP_SHIFT));
        assert!(key.stalled(1), "both halves: (0x21 << 2) | 0x02 == 0x86");
        assert_eq!(key.key(1) & 0xff, STALLED);
        // The PRO core's own pair is untouched by the APP core's.
        assert!(!key.stalled(0));

        // Clearing ONE half alone un-stalls, which is the other direction of
        // the same rule — and the reason a model that watched one register
        // would be wrong twice.
        let o0 = sb.read(&mut r, OPTIONS0);
        sb.write(&mut r, OPTIONS0, o0 & !(C0_MASK << C0_APP_SHIFT));
        assert!(!key.stalled(1));

        // And the PRO core's halves are at their own bits.
        let c1 = sb.read(&mut r, SW_CPU_STALL);
        sb.write(&mut r, SW_CPU_STALL, c1 | (0x21 << C1_PRO_SHIFT));
        let o0 = sb.read(&mut r, OPTIONS0);
        sb.write(&mut r, OPTIONS0, o0 | (0x02 << C0_PRO_SHIFT));
        assert!(key.stalled(0));
        assert!(!key.stalled(1));
    }

    #[test]
    fn the_rwdt_honours_its_key_and_never_fires() {
        let buf = lp_emu_esp_common::trace::SharedBuffer::new();
        let mut sb = Sandbox::new();
        sb.trace = lp_emu_esp_common::Trace::to_sink(Box::new(buf.clone()));
        let mut r = block();
        // Unlocked at reset: `wdtwprotect` resets to the key itself.
        assert_eq!(sb.read(&mut r, WDTWPROTECT), WDT_WKEY);
        assert_eq!(
            sb.read(&mut r, WDTCONFIG0),
            0x0000_4c80,
            "the PAC's reset for RTC_CNTL.wdtconfig0"
        );

        // `Rtc::new`'s disable: unlock, clear `wdt_en`, lock.
        sb.write(&mut r, WDTWPROTECT, WDT_WKEY);
        sb.write(&mut r, WDTCONFIG0, 0);
        sb.write(&mut r, WDTWPROTECT, 0);
        assert_eq!(sb.read(&mut r, WDTCONFIG0), 0);
        // Locked: a write is dropped, not taken.
        sb.write(&mut r, WDTCONFIG0, 0xffff_ffff);
        assert_eq!(sb.read(&mut r, WDTCONFIG0), 0, "locked");
        assert!(buf.lines().is_empty(), "and nothing was armed");

        // A ten-second emulated run with the RWDT disabled is quiet: this
        // block schedules nothing at all, so there is no deadline to reach.
        assert_eq!(sb.sched.next_deadline(), None);
        sb.run_to(&mut r, 10 * crate::memmap::CPU_HZ);
        assert_eq!(sb.sched.next_deadline(), None);
        assert!(buf.lines().is_empty());

        // Arming it *is* visible, because its expiry is not modelled.
        sb.write(&mut r, WDTWPROTECT, WDT_WKEY);
        sb.write(&mut r, WDTCONFIG0, WDT_EN);
        assert_eq!(buf.lines().len(), 1);
        assert!(buf.lines()[0].contains("RWDT ARMED"));
        // `wdtfeed` is a pulse.
        sb.write(&mut r, WDTFEED, 1);
        assert_eq!(sb.read(&mut r, WDTFEED), 0);
    }

    #[test]
    fn the_software_system_reset_is_reported_not_performed() {
        let buf = lp_emu_esp_common::trace::SharedBuffer::new();
        let mut sb = Sandbox::new();
        sb.trace = lp_emu_esp_common::Trace::to_sink(Box::new(buf.clone()));
        let mut r = block();
        // What `_rtc_trigger_sw_system_reset` (0x4000FDC7) stores.
        sb.write(&mut r, OPTIONS0, 0x8000_0000);
        assert_eq!(buf.lines().len(), 1);
        assert!(buf.lines()[0].contains("sw_sys_rst"));
        assert_eq!(
            sb.read(&mut r, OPTIONS0) & SW_SYS_RST,
            0,
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
        // …and the listed one is still real.
        for (off, _, _) in DEVIATIONS {
            assert_ne!(r.regs.stored(*off), regs::RTC_CNTL.reset(*off).unwrap_or(0));
        }
    }

    /// The clock and store registers the boot leans on, at the PAC's resets.
    #[test]
    fn the_clock_and_store_registers_are_accept_and_remember() {
        let mut sb = Sandbox::new();
        let mut r = block();
        assert_eq!(r.reg_name(0x070), Some("clk_conf"));
        assert_eq!(sb.read(&mut r, 0x070), 0x0000_2210, "the PAC's reset");
        assert_eq!(r.reg_name(ANA_CONF), Some("ana_conf"));
        assert_eq!(sb.read(&mut r, ANA_CONF), 0x0080_0000);
        // `store4` holds the XTAL frequency in two 16-bit copies; the boot
        // writes 40 MHz there now that the calibration works.
        assert_eq!(r.reg_name(0x0b0), Some("store4"));
        sb.write(&mut r, 0x0b0, 0x0028_0028);
        assert_eq!(sb.read(&mut r, 0x0b0), 0x0028_0028);
        // `store1` is where the slow-clock period lands.
        assert_eq!(r.reg_name(0x050), Some("store1"));
        // `store6`/`store7` are the ROM's `rtc_boot_control` words.
        assert_eq!(r.reg_name(0x0b8), Some("store6"));
        assert_eq!(r.reg_name(0x0bc), Some("store7"));
        sb.write(&mut r, 0x0b8, 0xdead_beef);
        assert_eq!(sb.read(&mut r, 0x0b8), 0xdead_beef);
    }

    #[test]
    fn the_state_round_trips() {
        let mut sb = Sandbox::new();
        let mut r = block();
        sb.write(&mut r, SW_CPU_STALL, 0x21 << C1_APP_SHIFT);
        sb.write(&mut r, OPTIONS0, 0x02);
        let blob = r.save_state();

        let other_key = StallKey::new();
        let mut other = RtcCntl::new(ResetCause::PowerOn, other_key.clone());
        assert!(!other_key.stalled(1));
        other.load_state(&blob);
        assert!(other_key.stalled(1), "the key is republished on restore");
        assert_eq!(other.save_state(), blob);
    }
}
