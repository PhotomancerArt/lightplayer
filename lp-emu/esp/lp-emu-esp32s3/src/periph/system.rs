//! `SYSTEM` at `0x600C_0000` — the clock and reset gates, `cpu_per_conf`,
//! the hold on core 1, and **the four software interrupts**.
//!
//! A fresh view: the three chips have three unrelated layouts here
//! (`m6/notes.md` §3.0 row 6 — the classic's is `DPORT`, the C6's `PCR`)
//! and nothing ports. What it does is small and every piece is cited:
//!
//! | register | what this view does | source |
//! |---|---|---|
//! | `core_1_control_0` `+0x00` | reads and writes [`CoreOneControl`] through the machine's handle — the register `Machine::core_stalled` already reads. `runstall` 0, `clkgate_en` 1, `reseting` 2; PAC reset `0x04` | `esp32s3-0.35.2/src/system/core_1_control_0.rs`; esp-hal `cpu_control.rs:41-51, 80-101` |
//! | `core_1_control_1` `+0x04` | accept-and-remember | PAC: "it's only a R/W register, no function, software can write any value" |
//! | `cpu_peri_clk_en` `+0x08`, `cpu_peri_rst_en` `+0x0c`, `perip_clk_en0/1` `+0x18/+0x1c`, `perip_rst_en0/1` `+0x20/+0x24` | **written-and-read-back** at the PAC's resets (`perip_clk_en0` = `0xf9c1_e06f`, one bit per peripheral — `rmt_clk_en` 9, `systimer_clk_en` 29). esp-hal's `PeripheralClockControl` read-modify-writes them and reads nothing back that hardware would change | `soc/esp32s3/clocks.rs:741-753` (the `uart_mem` pair is the shape of all of them) |
//! | `cpu_per_conf` `+0x10` | accept: `cpuperiod_sel` 0:1, `pll_freq_sel` 2, `cpu_wait_mode_force_on` 3, `cpu_waiti_delay_num` 4:7; reset `0x0c` | `configure_cpu_clk_impl`, `clocks.rs:433-437` |
//! | `cpu_intr_from_cpu0..3` `+0x30..+0x3c` | **the software interrupts**: bit 0 (`cpu_intr`) *is* interrupt source `FROM_CPU_INTR0+n` (79..82). A write sets the level; a read returns it. They are not in an INTPRI as on the C6 and not in DPORT as on the classic | esp-hal `interrupt/software.rs:109-131` (`raise`: `SYSTEM::regs().cpu_intr_from_cpu(n).write(cpu_intr = 1)`, then a read-back), `:135-155` (`reset`: write 0) |
//! | `cache_control` `+0x48` | accept: `icache_clk_on` 0, `icache_reset` 1, `dcache_clk_on` 2, `dcache_reset` 3; reset `0x05` | PAC field docs; P06 reads it |
//! | `sysclk_conf` `+0x60` | accept: `pre_div_cnt` 0:9, `soc_clk_sel` 10:11, `clk_xtal_freq` 12:18; reset `0x01` | `clocks.rs:396-398, 440-443` |
//! | everything else to `date` `+0xffc` | accept-and-remember at the PAC's resets | — |
//!
//! The software-interrupt sources are levels the matrix routes
//! ([`crate::intmatrix`]): which CPU interrupt `FROM_CPU_INTR0` raises is
//! `core_N_intr_map[79]`'s business, never this file's. esp-hal's `raise`
//! reads the register back after writing it — "to ensure the write is
//! completed" — and that read is answered from the level, so a raise that
//! did not reach [`BusCx::irq`] would be visible to the guest at once.
//!
//! # The hold on core 1
//!
//! `core_1_control_0` is the second of the three inputs
//! `Machine::core_stalled` ORs (the module docs of `crate::machine`). The
//! handle is the machine's; this view is the door a guest writes it
//! through, and a write that changes whether the core is held yields to
//! the machine so the hold is re-evaluated before the guest runs another
//! instruction. A future S3 firmware with a second core would write
//! `start_core1`'s sequence here and meet a modelled hold — and nothing
//! behind it, because where a released core starts is a measurement nobody
//! has made on this part (`crate::machine`'s module docs).

use std::sync::Arc;

use lp_emu_esp_common::periph::RegGrade;
use lp_emu_esp_common::regfile::{lane_of, merge_lane};
use lp_emu_esp_common::{BusCx, Peripheral, RegFile, Width};

use crate::machine::{CoreOneControl, CoreOneHandle};
use crate::regs::{self, source};

/// The block's aperture: the generated table runs to `+0xffc` (`date`).
pub const SYSTEM_LEN: u32 = 0x1000;

/// `core_1_control_0`.
pub const CORE_1_CONTROL_0: u32 = 0x000;
/// `core_1_control_1` — "no function, software can write any value".
pub const CORE_1_CONTROL_1: u32 = 0x004;
/// `cpu_per_conf`.
pub const CPU_PER_CONF: u32 = 0x010;
/// `perip_clk_en0`.
pub const PERIP_CLK_EN0: u32 = 0x018;
/// `cpu_intr_from_cpu0`; the four are one word apart.
pub const CPU_INTR_FROM_CPU0: u32 = 0x030;
/// One past `cpu_intr_from_cpu3`.
pub const CPU_INTR_FROM_CPU_END: u32 = CPU_INTR_FROM_CPU0 + 4 * SOFTWARE_INTERRUPTS;
/// `sysclk_conf`.
pub const SYSCLK_CONF: u32 = 0x060;

/// How many software interrupts the S3 has: `cpu_intr_from_cpu[0..4]`.
pub const SOFTWARE_INTERRUPTS: u32 = 4;

/// The interrupt **source** the `n`th software interrupt drives
/// (`regs::source::FROM_CPU_INTR0..3` = 79..82), or `None` past the fourth.
pub fn software_source(n: u32) -> Option<u16> {
    (n < SOFTWARE_INTERRUPTS).then(|| source::FROM_CPU_INTR0 + n as u16)
}

/// `SYSTEM` — a view over the machine's core-1 handle and the interrupt
/// source lines, with a [`RegFile`] behind everything else.
pub struct SystemView {
    file: RegFile,
    core1: CoreOneHandle,
}

impl SystemView {
    /// The block, sharing `core1` with the machine.
    ///
    /// The register file carries `regs::SYSTEM`'s names and all seventeen of
    /// the PAC's non-zero reset values. The two register groups this view
    /// intercepts read from their real homes instead, and the grade table
    /// says so: they are `Modeled` (a register we pretend about), where the
    /// plain read-write gates are `Documented`.
    pub fn new(core1: CoreOneHandle) -> Self {
        let mut file = RegFile::new("SYSTEM", SYSTEM_LEN)
            .with_names(regs::SYSTEM)
            .with_pac_grades()
            .with_grade(CORE_1_CONTROL_0, RegGrade::Modeled);
        for n in 0..SOFTWARE_INTERRUPTS {
            file = file.with_grade(CPU_INTR_FROM_CPU0 + 4 * n, RegGrade::Modeled);
        }
        Self { file, core1 }
    }

    fn core1(&self) -> CoreOneControl {
        *self.core1.lock().expect("core_1_control poisoned")
    }

    /// The word this view answers `off` with, ignoring the byte lane.
    fn read_word(&self, off: u32, cx: &BusCx<'_>) -> u32 {
        if off == CORE_1_CONTROL_0 {
            return self.core1().bits();
        }
        if (CPU_INTR_FROM_CPU0..CPU_INTR_FROM_CPU_END).contains(&off) {
            let n = (off - CPU_INTR_FROM_CPU0) / 4;
            let src = software_source(n).expect("n < 4");
            return u32::from(cx.irq.level(src));
        }
        self.file.effective(off)
    }
}

impl Peripheral for SystemView {
    fn name(&self) -> &'static str {
        "SYSTEM"
    }

    fn read(&mut self, off: u32, width: Width, cx: &mut BusCx<'_>) -> u32 {
        lane_of(self.read_word(off & !3, cx), off, width)
    }

    fn write(&mut self, off: u32, width: Width, value: u32, cx: &mut BusCx<'_>) {
        let word = off & !3;
        let old = self.read_word(word, cx);
        let next = merge_lane(old, off, width, value);

        if word == CORE_1_CONTROL_0 {
            let before = self.core1();
            let after = CoreOneControl::from_bits(next);
            *self.core1.lock().expect("core_1_control poisoned") = after;
            if before.holds_core1() != after.holds_core1() {
                // Whether slot 1 is held is the machine's question, asked
                // between slices; answer it before the guest runs on.
                cx.yield_to_machine();
            }
            return;
        }
        if (CPU_INTR_FROM_CPU0..CPU_INTR_FROM_CPU_END).contains(&word) {
            let n = (word - CPU_INTR_FROM_CPU0) / 4;
            let src = software_source(n).expect("n < 4");
            cx.irq.set_level(src, next & 1 != 0);
            return;
        }
        self.file.write(off, width, value, cx);
    }

    fn reg_name(&self, off: u32) -> Option<&'static str> {
        regs::SYSTEM.name(off)
    }

    fn reg_grade(&self, off: u32) -> Option<RegGrade> {
        self.file.reg_grade(off)
    }

    fn save_state(&self) -> Vec<u8> {
        // The core-1 handle is the machine's and rides in the snapshot as
        // its own field; the source levels are the bus's scalars.
        self.file.save_state()
    }

    fn load_state(&mut self, bytes: &[u8]) {
        self.file.load_state(bytes);
    }
}

impl std::fmt::Debug for SystemView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SystemView")
            .field("core1", &self.core1())
            .finish_non_exhaustive()
    }
}

/// A fresh handle at the PAC's reset, for a view built outside a machine.
pub fn core1_handle() -> CoreOneHandle {
    Arc::new(std::sync::Mutex::new(CoreOneControl::reset()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_emu_esp_common::Sandbox;

    #[test]
    fn the_gates_are_written_and_read_back_at_the_pacs_resets() {
        let mut sb = Sandbox::new();
        let mut s = SystemView::new(core1_handle());
        assert_eq!(s.reg_name(PERIP_CLK_EN0), Some("perip_clk_en0"));
        assert_eq!(
            sb.read(&mut s, PERIP_CLK_EN0),
            0xf9c1_e06f,
            "the PAC's reset"
        );
        // `PeripheralClockControl::enable(Rmt)`: set bit 9.
        let v = sb.read(&mut s, PERIP_CLK_EN0);
        sb.write(&mut s, PERIP_CLK_EN0, v | (1 << 9));
        assert_eq!(sb.read(&mut s, PERIP_CLK_EN0), 0xf9c1_e06f | (1 << 9));
        assert_eq!(sb.read(&mut s, CPU_PER_CONF), 0x0c);
        assert_eq!(sb.read(&mut s, SYSCLK_CONF), 0x01);
        assert_eq!(s.reg_name(0xffc), Some("date"));
        assert_eq!(s.reg_grade(PERIP_CLK_EN0), Some(RegGrade::Documented));
        assert_eq!(s.reg_grade(CORE_1_CONTROL_0), Some(RegGrade::Modeled));
    }

    /// The register the machine's hold reads, written through the view:
    /// the PAC reset first, then esp-hal's `start_core1` sequence, whose
    /// last write clears every hold input this register carries.
    #[test]
    fn a_guest_write_to_core_1_control_0_changes_the_hold() {
        let mut sb = Sandbox::new();
        let handle = core1_handle();
        let mut s = SystemView::new(Arc::clone(&handle));
        assert_eq!(sb.read(&mut s, CORE_1_CONTROL_0), 0x04, "reseting at reset");
        assert!(handle.lock().unwrap().holds_core1());

        // `start_core1` (`cpu_control.rs:80-101`): clkgate_en set, runstall
        // clear, reseting set then clear.
        let v = sb.read(&mut s, CORE_1_CONTROL_0);
        sb.write(&mut s, CORE_1_CONTROL_0, v | 0b010);
        let v = sb.read(&mut s, CORE_1_CONTROL_0);
        sb.write(&mut s, CORE_1_CONTROL_0, v & !0b001);
        let v = sb.read(&mut s, CORE_1_CONTROL_0);
        sb.write(&mut s, CORE_1_CONTROL_0, v | 0b100);
        assert!(handle.lock().unwrap().holds_core1(), "still in reset");
        assert!(!sb.yield_now, "no change in the hold yet");
        let v = sb.read(&mut s, CORE_1_CONTROL_0);
        sb.write(&mut s, CORE_1_CONTROL_0, v & !0b100);
        assert!(
            !handle.lock().unwrap().holds_core1(),
            "released by the register"
        );
        assert!(sb.yield_now, "and the machine was asked to look");
        assert_eq!(sb.read(&mut s, CORE_1_CONTROL_0), 0b010);
        // `runstall` set holds it again.
        sb.write(&mut s, CORE_1_CONTROL_0, 0b011);
        assert!(handle.lock().unwrap().holds_core1());
    }

    /// esp-hal's `raise` and `reset`, register for register: a write sets
    /// the source level, the read-back sees it, a write of 0 clears it.
    #[test]
    fn a_software_interrupt_write_is_a_source_level() {
        let mut sb = Sandbox::new();
        let mut s = SystemView::new(core1_handle());
        assert_eq!(
            s.reg_name(CPU_INTR_FROM_CPU0 + 8),
            Some("cpu_intr_from_cpu2")
        );
        assert_eq!(software_source(2), Some(81));
        assert_eq!(software_source(4), None);
        sb.write(&mut s, CPU_INTR_FROM_CPU0 + 8, 1);
        assert!(sb.irq.level(source::FROM_CPU_INTR2));
        assert!(!sb.irq.level(source::FROM_CPU_INTR0));
        assert_eq!(sb.read(&mut s, CPU_INTR_FROM_CPU0 + 8), 1, "the read-back");
        sb.write(&mut s, CPU_INTR_FROM_CPU0 + 8, 0);
        assert!(!sb.irq.level(source::FROM_CPU_INTR2));
        assert_eq!(sb.read(&mut s, CPU_INTR_FROM_CPU0 + 8), 0);
    }

    #[test]
    fn the_state_round_trips() {
        let mut sb = Sandbox::new();
        let mut s = SystemView::new(core1_handle());
        sb.write(&mut s, CORE_1_CONTROL_1, 0xdead_beef);
        sb.write(&mut s, PERIP_CLK_EN0, 0x1234);
        let blob = s.save_state();
        let mut other = SystemView::new(core1_handle());
        other.load_state(&blob);
        assert_eq!(sb.read(&mut other, CORE_1_CONTROL_1), 0xdead_beef);
        assert_eq!(sb.read(&mut other, PERIP_CLK_EN0), 0x1234);
    }
}
