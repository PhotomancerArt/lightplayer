//! Per-instruction cost classes for cycle accounting (not a hardware perf counter).

/// Identifies the CPU whose cycle behaviour is being estimated by the
/// emulator's per-instruction cost model.
///
/// Only [`CycleModel::Esp32C6`] is implemented today; additional variants
/// can be added without touching the run loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CycleModel {
    /// Per-instruction estimate ignored; cycle count tracks instruction count 1:1.
    InstructionCount,

    /// ESP32-C6 (Andes N22-class single-issue in-order RV32IMAC core).
    ///
    /// Reference: <https://ctrlsrc.io/posts/2023/counting-cpu-cycles-on-esp32c3-esp32c6/>
    ///
    /// This is a coarse approximation: per-class fixed costs plus
    /// branch-taken vs not-taken. ICache misses, branch-predictor warm-up,
    /// variable DIV cycles, and load-use hazards are not modelled.
    #[default]
    Esp32C6,
}

/// Cost bucket for [`CycleModel::cycles_for`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstClass {
    Alu,
    Mul,
    DivRem,
    Load,
    Store,
    BranchTaken,
    BranchNotTaken,
    Jal,
    Jalr,
    Lui,
    Auipc,
    System,
    Fence,
    Atomic,
}

impl CycleModel {
    pub fn cycles_for(self, class: InstClass) -> u8 {
        match self {
            CycleModel::InstructionCount => 1,
            CycleModel::Esp32C6 => match class {
                InstClass::Alu | InstClass::Mul | InstClass::Lui | InstClass::Auipc => 1,
                InstClass::DivRem => 32,
                InstClass::Load => 2,
                InstClass::Store => 1,
                InstClass::BranchNotTaken => 1,
                InstClass::BranchTaken => 2,
                InstClass::Jal => 2,
                InstClass::Jalr => 3,
                InstClass::System => 4,
                InstClass::Fence => 4,
                InstClass::Atomic => 4,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate alloc;
    use alloc::vec::Vec;

    use lp_riscv_inst::{Gpr, encode};

    use crate::Riscv32Emulator;

    use super::CycleModel;

    fn push_u32(code: &mut Vec<u8>, word: u32) {
        code.extend_from_slice(&word.to_le_bytes());
    }

    fn loop_addi_bne_program() -> Vec<u8> {
        let mut code = Vec::new();
        push_u32(&mut code, encode::addi(Gpr::new(5), Gpr::new(0), 0));
        push_u32(&mut code, encode::addi(Gpr::new(6), Gpr::new(0), 5));
        push_u32(&mut code, encode::addi(Gpr::new(5), Gpr::new(5), 1));
        push_u32(&mut code, encode::bne(Gpr::new(5), Gpr::new(6), -4));
        push_u32(&mut code, encode::ebreak());
        code
    }

    #[test]
    fn instruction_count_model_matches_instruction_count() {
        let code = loop_addi_bne_program();
        let mut emu = Riscv32Emulator::new(code, alloc::vec![0u8; 4096]);
        emu.set_cycle_model(CycleModel::InstructionCount);
        emu.run_until_ebreak().expect("ebreak");
        let n = emu.get_instruction_count();
        assert_eq!(emu.get_cycle_count(), n);
    }

    #[test]
    fn esp32c6_cycle_count_matches_loop_arithmetic() {
        let code = loop_addi_bne_program();
        let mut emu = Riscv32Emulator::new(code, alloc::vec![0u8; 4096]);
        emu.set_cycle_model(CycleModel::Esp32C6);
        emu.run_until_ebreak().expect("ebreak");
        assert_eq!(emu.get_instruction_count(), 13);
        // 2 setup ALU + 5×(ALU + branch): 4 taken + 1 not-taken + EBREAK system
        assert_eq!(emu.get_cycle_count(), 20);
    }
}
