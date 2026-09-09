//! Gate item 7: the stack guard, hart + bus + matrix, no firmware.
//!
//! A tiny guest arms trigger 0 the way esp-hal's `set_watchpoint(0, guard,
//! 4)` does (`debugger.rs:194-233`), then stores through the guard. The
//! store must trap with `mcause = 3` (Breakpoint), `mtval = guard`,
//! `mepc` at the store (`mcontrol` action 0 fires *before* the access,
//! discovery §4d), `tdata1.hit` set, and the guard word untouched.
//!
//! The trap handler is deliberately at an address nothing maps: the trap is
//! taken (the CSRs are written), the handler fetch faults, and the machine
//! stops with [`Outcome::Fault`] — leaving the first trap's CSRs exactly as
//! delivered, where a real handler's own `ebreak` would overwrite them.

use lp_emu_core::Bus;
use lp_emu_esp32c6::machine::{Esp32C6Builder, Esp32C6Machine, Outcome, StopCondition};
use lp_emu_esp32c6::memmap;
use lp_riscv_emu::mach::csr;

const GUARD: u32 = 0x4084_003c;
const GUARD_VALUE: u32 = 0xDEED_BAAD;
const CODE: u32 = memmap::HP_SRAM_BASE + 0x1000;
/// Inside the MMIO window, claimed by nothing on a bare machine.
const HANDLER: u32 = 0x7000_0000;

/// `csrw csr, rs1` = `csrrw x0, csr, rs1`.
fn csrw(csr: u32, rs1: u32) -> u32 {
    (csr << 20) | (rs1 << 15) | (0b001 << 12) | 0x73
}

/// `lui rd, imm20`.
fn lui(rd: u32, imm: u32) -> u32 {
    (imm & 0xffff_f000) | (rd << 7) | 0x37
}

/// `addi rd, rs1, imm12`.
fn addi(rd: u32, rs1: u32, imm: i32) -> u32 {
    ((imm as u32 & 0xfff) << 20) | (rs1 << 15) | (rd << 7) | 0x13
}

/// `sw rs2, imm(rs1)`.
fn sw(rs2: u32, rs1: u32, imm: i32) -> u32 {
    let imm = imm as u32 & 0xfff;
    ((imm >> 5) << 25) | (rs2 << 20) | (rs1 << 15) | (0b010 << 12) | ((imm & 0x1f) << 7) | 0x23
}

/// `li rd, value` as `lui` + `addi`, correct for any 32-bit value.
fn li(rd: u32, value: u32) -> [u32; 2] {
    let lo = (value & 0xfff) as i32;
    let lo = if lo >= 0x800 { lo - 0x1000 } else { lo };
    let hi = value.wrapping_sub(lo as u32);
    [lui(rd, hi), addi(rd, rd, lo)]
}

const EBREAK: u32 = 0x0010_0073;

/// esp-hal's four-CSR arming sequence, into `t0`.
fn arm_guard(program: &mut Vec<u32>) {
    program.push(csrw(csr::TSELECT as u32, 0));
    program.extend(li(5, 0x8)); // tcontrol.mte
    program.push(csrw(csr::TCONTROL as u32, 5));
    program.extend(li(5, 0xC2)); // store | m | match = NAPOT
    program.push(csrw(csr::TDATA1 as u32, 5));
    program.extend(li(5, (GUARD & !3) | 1)); // 4-byte NAPOT
    program.push(csrw(csr::TDATA2 as u32, 5));
}

fn load(m: &mut Esp32C6Machine, program: &[u32]) {
    let bytes: Vec<u8> = program.iter().flat_map(|w| w.to_le_bytes()).collect();
    m.bus.load_image(CODE, &bytes).unwrap();
    m.bus.write_word(GUARD, GUARD_VALUE as i32).unwrap();
    m.harts[0].set_pc(CODE);
}

#[test]
fn a_store_through_the_armed_guard_traps_with_mcause_3_and_mtval_the_guard() {
    let mut m = Esp32C6Builder::bare().build().unwrap();
    let mut program = Vec::new();
    program.extend(li(5, HANDLER)); // mtvec = HANDLER, direct mode
    program.push(csrw(csr::MTVEC as u32, 5));
    arm_guard(&mut program);
    program.extend(li(6, GUARD)); // t1 = guard
    let store_pc = CODE + 4 * program.len() as u32;
    program.push(sw(0, 6, 0)); // sw x0, 0(t1)  <- traps here
    program.push(EBREAK); // never reached
    load(&mut m, &program);

    let outcome = m.run_until(&StopCondition::after_micros(10));
    assert!(
        matches!(
            outcome,
            Outcome::Fault {
                fault: lp_riscv_emu::mach::HartFault::TrapVectorFetch { vector: HANDLER },
                ..
            }
        ),
        "the trap was taken and the handler fetch faulted: {outcome:?}"
    );
    let csr = m.harts[0].csr();
    assert_eq!(csr.mcause, 3, "Breakpoint");
    assert_eq!(csr.mtval, GUARD, "mtval = the faulting data address");
    assert_eq!(csr.mepc, store_pc, "mepc = the store itself, not past it");
    assert!(m.harts[0].triggers().hit(0), "tdata1.hit for slot 0");
    assert_eq!(m.harts[0].triggers().tcontrol(), 0x8);
    assert_eq!(m.harts[0].triggers().tdata2(), (GUARD & !3) | 1);
    assert_eq!(
        m.peek_word(GUARD),
        Some(GUARD_VALUE),
        "the guard word survived: the trap fired before the store"
    );
}

#[test]
fn a_store_next_to_the_guard_does_not_trap() {
    let mut m = Esp32C6Builder::bare().build().unwrap();
    let mut program = Vec::new();
    program.extend(li(5, HANDLER));
    program.push(csrw(csr::MTVEC as u32, 5));
    arm_guard(&mut program);
    program.extend(li(6, GUARD + 4));
    program.push(sw(0, 6, 0)); // the word above the guard
    program.extend(li(6, GUARD - 4));
    program.push(sw(0, 6, 0)); // the word below
    let end_pc = CODE + 4 * program.len() as u32;
    program.push(EBREAK);
    load(&mut m, &program);
    m.bus.write_word(GUARD + 4, 1).unwrap();
    m.bus.write_word(GUARD - 4, 1).unwrap();

    m.run_until(&StopCondition::after_micros(10));
    assert!(
        !m.harts[0].triggers().hit(0),
        "neighbours are not the guard"
    );
    assert_eq!(m.peek_word(GUARD + 4), Some(0));
    assert_eq!(m.peek_word(GUARD - 4), Some(0));
    assert_eq!(m.peek_word(GUARD), Some(GUARD_VALUE));
    // Both stores retired; the run ended at the `ebreak`.
    assert_eq!(m.harts[0].csr().mepc, end_pc);
}

#[test]
fn with_mte_clear_the_guard_is_not_watched() {
    // `tcontrol.mte = 0`: the trigger is configured but does not fire in
    // M-mode. What `clear_watchpoint`'s `csrrw tdata1, 0` achieves by
    // another route.
    let mut m = Esp32C6Builder::bare().build().unwrap();
    let mut program = Vec::new();
    program.extend(li(5, HANDLER));
    program.push(csrw(csr::MTVEC as u32, 5));
    arm_guard(&mut program);
    program.push(csrw(csr::TCONTROL as u32, 0)); // x0: mte off
    program.extend(li(6, GUARD));
    program.push(sw(0, 6, 0));
    program.push(EBREAK);
    load(&mut m, &program);
    m.run_until(&StopCondition::after_micros(10));
    assert!(!m.harts[0].triggers().hit(0));
    assert_eq!(m.peek_word(GUARD), Some(0), "the store went through");
}
