//! `INTPRI` at `0x600C_5000` — software interrupts, and nothing else.
//!
//! The C6 is `interrupt_controller = "plic"`, so `cpu_int_enable`,
//! `cpu_int_pri`, `cpu_int_thresh`, `cpu_int_type` and `cpu_int_clear` here
//! are **never touched by esp-hal** (discovery §2a — that is the C2/C3
//! path). The one thing it uses is `cpu_intr_from_cpu[n]` at `+0x90 + 4n`,
//! bit 0 (`interrupt/software.rs:110-157`): `raise` writes 1, `reset`
//! writes 0, and the bit **is** the level of source `FROM_CPU_INTR0 + n`
//! (22..=25). esp-rtos's context switch is `raise` on SWI0 and `reset` from
//! inside `swint_handler` (`task/riscv.rs:313-322`).
//!
//! Everything else in the block is accept-and-remember.

use lp_emu_esp_common::{BusCx, Peripheral, RegFile, Width};

use crate::regs::{self, source};

const CPU_INTR_FROM_CPU0: u32 = 0x90;
const CPU_INTR_FROM_CPU3: u32 = 0x9c;

/// The block.
#[derive(Debug)]
pub struct Intpri {
    regs: RegFile,
}

impl Default for Intpri {
    fn default() -> Self {
        Self::new()
    }
}

impl Intpri {
    pub fn new() -> Self {
        Self {
            regs: RegFile::new("INTPRI", 0x400).with_names(regs::INTPRI),
        }
    }
}

impl Peripheral for Intpri {
    fn name(&self) -> &'static str {
        "INTPRI"
    }

    fn read(&mut self, off: u32, width: Width, cx: &mut BusCx<'_>) -> u32 {
        self.regs.read(off, width, cx)
    }

    fn write(&mut self, off: u32, width: Width, value: u32, cx: &mut BusCx<'_>) {
        self.regs.write(off, width, value, cx);
        let word = off & !3;
        if (CPU_INTR_FROM_CPU0..=CPU_INTR_FROM_CPU3).contains(&word) {
            let n = ((word - CPU_INTR_FROM_CPU0) / 4) as u16;
            cx.irq
                .set_level(source::FROM_CPU_INTR0 + n, self.regs.stored(word) & 1 != 0);
        }
    }

    fn reg_name(&self, off: u32) -> Option<&'static str> {
        regs::INTPRI.name(off)
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
    fn raise_and_reset_are_the_level_of_source_twenty_two_plus_n() {
        let mut sb = Sandbox::new();
        let mut p = Intpri::new();
        for n in 0..4u32 {
            let reg = CPU_INTR_FROM_CPU0 + 4 * n;
            assert!(!sb.irq.level(22 + n as u16));
            sb.write(&mut p, reg, 1); // raise
            assert_eq!(sb.read(&mut p, reg), 1, "read-back fence");
            assert!(sb.irq.level(22 + n as u16));
            sb.write(&mut p, reg, 0); // reset
            assert!(!sb.irq.level(22 + n as u16));
        }
        assert_eq!(p.reg_name(CPU_INTR_FROM_CPU0), Some("cpu_intr_from_cpu0"));
        // The rest of the block is plain memory.
        sb.write(&mut p, 0x8c, 5);
        assert_eq!(sb.read(&mut p, 0x8c), 5);
        assert!(!sb.irq.any());
    }
}
