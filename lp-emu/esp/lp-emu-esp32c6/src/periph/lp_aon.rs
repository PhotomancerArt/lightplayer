//! `LP_AON`: the accept-and-remember block of `accept::lp_aon`, plus the one
//! bit in it that is not a memory — `sys_cfg.hpsys_sw_reset` (bit 31 of
//! `+0x034`), the chip's software reset of the HP system.
//!
//! The mask ROM's `software_reset` (`0x4001_973c`, reached from esp-hal's
//! `software_reset()` through the `0x4000_0090` trampoline) is a
//! read-modify-write of that register and a `ret`:
//!
//! ```text
//! 4001973c  lui  a4, 0x600b1
//! 40019740  lw   a5, 0x34(a4)
//! 40019742  lui  a3, 0x80000
//! 40019746  or   a5, a5, a3
//! 40019748  sw   a5, 0x34(a4)
//! 4001974a  ret
//! ```
//!
//! On silicon the chip is in reset before the `ret` matters. Until
//! 2026-10-04 this block was a bare `RegFile`, so the store was remembered
//! and nothing happened: the ROM returned into esp-hal's `-> !` function,
//! which falls through into whatever follows it in flash, and the RTC
//! watchdog rebooted the chip seconds later with the wrong cause
//! (`docs/defects/2026-09-29-the-emulated-c6-does-not-perform-a-software-reset.md`).
//!
//! Now the store raises [`MachineRequest::Reset`] with
//! [`ResetSource::Software`] — which this chip's ROM table names
//! `rst:0x3 (LP_SW_HPSYS)` (`crate::loader::ResetCause::LpSwHpSys`) — and
//! ends the slice, so the guest runs no instruction after the store. With
//! `--reboot-on-reset` the machine reboots at once (HP domain restored, LP
//! domain kept, exactly as a watchdog reset); without it the run ends with
//! `Outcome::Reset`, as it does for the watchdog.
//!
//! **The bit does not stay set.** The PAC gives `hpsys_sw_reset` a writer and
//! no reader, and the block is `Domain::Lp`, so it survives the reset it
//! caused. Kept, the next read-modify-write of `sys_cfg` (a
//! `force_download_boot` change, bit 30) would write it back and reset the
//! chip again. So it is stored cleared: a strobe, the way silicon behaves as
//! far as anything that survives the reset can see.
//!
//! Everything else in the block is the `RegFile`'s, unchanged — the same
//! names, grades, domain and snapshot bytes.

use lp_emu_esp_common::{
    BusCx, Domain, MachineRequest, Peripheral, RegFile, ResetSource, Strap, Width, periph::RegGrade,
};

use super::accept;

/// `LP_AON.sys_cfg`.
pub const SYS_CFG: u32 = 0x034;
/// `sys_cfg.hpsys_sw_reset`: write 1 to reset the HP system.
pub const HPSYS_SW_RESET: u32 = 1 << 31;

/// The `LP_AON` block: `accept::lp_aon`'s registers with the software-reset
/// strobe performed.
pub struct LpAon {
    regs: RegFile,
}

impl LpAon {
    pub fn new() -> Self {
        Self {
            regs: accept::lp_aon(),
        }
    }
}

impl Default for LpAon {
    fn default() -> Self {
        Self::new()
    }
}

impl Peripheral for LpAon {
    fn name(&self) -> &'static str {
        self.regs.name()
    }

    fn domain(&self) -> Domain {
        self.regs.domain()
    }

    fn read(&mut self, off: u32, width: Width, cx: &mut BusCx<'_>) -> u32 {
        self.regs.read(off, width, cx)
    }

    fn write(&mut self, off: u32, width: Width, value: u32, cx: &mut BusCx<'_>) {
        self.regs.write(off, width, value, cx);
        if off & !3 != SYS_CFG {
            return;
        }
        let stored = self.regs.stored(SYS_CFG);
        if stored & HPSYS_SW_RESET == 0 {
            return;
        }
        self.regs.poke(SYS_CFG, stored & !HPSYS_SW_RESET);
        let line = format!(
            "cyc={} pc=0x{:08x} LP_AON sys_cfg.hpsys_sw_reset: software reset of the HP system",
            cx.now, cx.pc
        );
        cx.trace.note(&line);
        let at = cx.now;
        // A software reset boots the app: the strap pins decide that, and
        // nothing about this reset touches them.
        cx.request(MachineRequest::Reset {
            source: "LP_AON sys_cfg.hpsys_sw_reset",
            at,
            strap: Strap::App,
            cause: ResetSource::Software,
        });
        // The reset happens on the store: not one more guest instruction.
        cx.yield_to_machine();
    }

    fn reg_name(&self, off: u32) -> Option<&'static str> {
        self.regs.reg_name(off)
    }

    fn reg_grade(&self, off: u32) -> Option<RegGrade> {
        self.regs.reg_grade(off)
    }

    fn save_state(&self) -> Vec<u8> {
        self.regs.save_state()
    }

    fn load_state(&mut self, bytes: &[u8]) {
        self.regs.load_state(bytes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_emu_esp_common::Sandbox;

    #[test]
    fn setting_the_bit_asks_for_a_software_reset_and_ends_the_slice() {
        let mut sb = Sandbox::new();
        let mut aon = LpAon::new();
        sb.now = 1234;
        // The ROM's read-modify-write.
        let old = sb.read(&mut aon, SYS_CFG);
        sb.write(&mut aon, SYS_CFG, old | HPSYS_SW_RESET);
        assert_eq!(
            sb.request,
            Some(MachineRequest::Reset {
                source: "LP_AON sys_cfg.hpsys_sw_reset",
                at: 1234,
                strap: Strap::App,
                cause: ResetSource::Software,
            })
        );
        assert!(sb.yield_now, "the machine acts before the next instruction");
        assert_eq!(
            crate::loader::ResetCause::for_source(ResetSource::Software).rom_code(),
            0x3,
            "rst:0x3 (LP_SW_HPSYS), the ROM table's name for it"
        );
    }

    #[test]
    fn the_bit_is_a_strobe_so_a_later_read_modify_write_does_not_reset_again() {
        let mut sb = Sandbox::new();
        let mut aon = LpAon::new();
        sb.write(&mut aon, SYS_CFG, HPSYS_SW_RESET | (1 << 30));
        assert!(sb.request.take().is_some());
        sb.yield_now = false;
        assert_eq!(
            sb.read(&mut aon, SYS_CFG),
            1 << 30,
            "the other bits are remembered, the strobe is not"
        );
        // `force_download_boot` cleared by a read-modify-write: no reset.
        let v = sb.read(&mut aon, SYS_CFG);
        sb.write(&mut aon, SYS_CFG, v & !(1 << 30));
        assert!(sb.request.is_none());
        assert!(!sb.yield_now);
    }

    #[test]
    fn every_other_register_is_the_accept_block_unchanged() {
        let mut sb = Sandbox::new();
        let mut aon = LpAon::new();
        // `store1`, the calibration value the firmware reads back.
        sb.write(&mut aon, 0x004, 0xdead_beef);
        assert_eq!(sb.read(&mut aon, 0x004), 0xdead_beef);
        assert!(sb.request.is_none());
        assert_eq!(aon.domain(), Domain::Lp);
        assert_eq!(aon.name(), "LP_AON");
        assert_eq!(aon.reg_name(SYS_CFG), Some("sys_cfg"));
        let plain = accept::lp_aon();
        assert_eq!(aon.save_state().len(), plain.save_state().len());
    }
}
