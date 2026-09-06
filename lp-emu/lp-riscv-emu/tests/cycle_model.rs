//! Cycle-model accounting tests that exercise the rv32 emulator.
//!
//! [`CycleModel`]/[`InstClass`] live in `lp-emu-core`, but these tests drive
//! them through [`Riscv32Emulator`], so they live here (lp-emu-core cannot
//! dev-depend on lp-riscv-emu without a cycle).

use lp_emu_core::{CycleModel, InstClass};
use lp_riscv_emu::Riscv32Emulator;
use lp_riscv_inst::{Gpr, encode};

#[test]
fn instruction_count_model_matches_instruction_count() {
    let code = loop_addi_bne_program();
    let mut emu = Riscv32Emulator::new(code, vec![0u8; 4096]);
    emu.set_cycle_model(CycleModel::InstructionCount);
    emu.run_until_ebreak().expect("ebreak");
    let n = emu.get_instruction_count();
    assert_eq!(emu.get_cycle_count(), n);
}

#[test]
fn esp32c6_jump_class_costs_match_legacy_jal_jalr() {
    let m = CycleModel::Esp32C6;
    assert_eq!(m.cycles_for(InstClass::JalCall), 2);
    assert_eq!(m.cycles_for(InstClass::JalTail), 2);
    assert_eq!(m.cycles_for(InstClass::JalrCall), 3);
    assert_eq!(m.cycles_for(InstClass::JalrReturn), 3);
    assert_eq!(m.cycles_for(InstClass::JalrIndirect), 3);
}

#[test]
fn esp32c6_cycle_count_matches_loop_arithmetic() {
    let code = loop_addi_bne_program();
    let mut emu = Riscv32Emulator::new(code, vec![0u8; 4096]);
    emu.set_cycle_model(CycleModel::Esp32C6);
    emu.run_until_ebreak().expect("ebreak");
    assert_eq!(emu.get_instruction_count(), 13);
    // 2 setup ALU + 5×(ALU + branch): 4 taken + 1 not-taken + EBREAK system
    assert_eq!(emu.get_cycle_count(), 20);
}

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
