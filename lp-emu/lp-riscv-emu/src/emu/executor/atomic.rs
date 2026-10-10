//! Atomic instruction execution (A extension: LR.W, SC.W, AMOSWAP.W, AMOADD.W, AMOXOR.W, AMOAND.W, AMOOR.W)
//!
//! The AMOs are a read and a write with nothing in between, which is atomic on a
//! single hart. `lr.w` / `sc.w` are not that simple: `sc.w` writes only while
//! the reservation `lr.w` took still holds, and the caller ends that
//! reservation when the hart takes a trap (see [`LrReservation`]).

extern crate alloc;

use super::{ExecutionResult, InstClass, LoggingMode, read_reg};
use crate::emu::{error::EmulatorError, logging::InstLog, lr_reservation::LrReservation};
use lp_emu_core::Bus;
use lp_riscv_inst::{Gpr, format::TypeR};

/// Decode and execute atomic instructions (R-type, opcode 0x2f).
#[inline(always)]
pub(super) fn decode_execute_atomic<M: LoggingMode, B: Bus>(
    inst_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    memory: &mut B,
    reservation: &mut LrReservation,
) -> Result<ExecutionResult, EmulatorError> {
    let r = TypeR::from_riscv(inst_word);
    let rd = Gpr::new(r.rd);
    let rs1 = Gpr::new(r.rs1);
    let rs2 = Gpr::new(r.rs2);
    let funct3 = (r.func & 0x7) as u8; // Extract funct3 from func field
    let funct5 = ((inst_word >> 27) & 0x1f) as u8; // bits [31:27]

    // Atomic instructions require funct3 = 0x2 (word width)
    if funct3 != 0x2 {
        return Err(EmulatorError::InvalidInstruction {
            pc,
            instruction: inst_word,
            reason: alloc::format!("Unsupported atomic width: funct3=0x{funct3:x}"),
            regs: *regs,
        });
    }

    match funct5 {
        0x02 => execute_lr_w::<M, B>(rd, rs1, inst_word, pc, regs, memory, reservation),
        0x03 => execute_sc_w::<M, B>(rd, rs1, rs2, inst_word, pc, regs, memory, reservation),
        0x01 => execute_amoswap_w::<M, B>(rd, rs1, rs2, inst_word, pc, regs, memory),
        0x00 => execute_amoadd_w::<M, B>(rd, rs1, rs2, inst_word, pc, regs, memory),
        0x04 => execute_amoxor_w::<M, B>(rd, rs1, rs2, inst_word, pc, regs, memory),
        0x0c => execute_amoand_w::<M, B>(rd, rs1, rs2, inst_word, pc, regs, memory),
        0x08 => execute_amoor_w::<M, B>(rd, rs1, rs2, inst_word, pc, regs, memory),
        _ => Err(EmulatorError::InvalidInstruction {
            pc,
            instruction: inst_word,
            reason: alloc::format!("Unknown atomic instruction: funct5=0x{funct5:x}"),
            regs: *regs,
        }),
    }
}

#[inline(always)]
fn execute_lr_w<M: LoggingMode, B: Bus>(
    rd: Gpr,
    rs1: Gpr,
    inst_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    memory: &mut B,
    reservation: &mut LrReservation,
) -> Result<ExecutionResult, EmulatorError> {
    // LR.W: load the word and reserve it for the SC.W that follows.
    let base = read_reg(regs, rs1);
    let address = base as u32;

    let error_regs = *regs;
    let value = memory
        .read_word(address)
        .map_err(|e| EmulatorError::from_memory_error(e, pc, error_regs))?;

    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    if rd.num() != 0 {
        regs[rd.num() as usize] = value;
    }
    // Only once the load has happened: a faulting LR.W reserves nothing.
    reservation.reserve(address);

    let log = if M::ENABLED {
        Some(InstLog::Load {
            cycle: 0,
            pc,
            instruction: inst_word,
            rd,
            rs1_val: base,
            addr: address,
            mem_val: value,
            rd_old,
            rd_new: value,
        })
    } else {
        None
    };

    Ok(ExecutionResult {
        new_pc: None,
        should_halt: false,
        syscall: false,
        class: InstClass::Atomic,
        inst_size: 4,
        log,
    })
}

#[inline(always)]
fn execute_sc_w<M: LoggingMode, B: Bus>(
    rd: Gpr,
    rs1: Gpr,
    rs2: Gpr,
    inst_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    memory: &mut B,
    reservation: &mut LrReservation,
) -> Result<ExecutionResult, EmulatorError> {
    // SC.W: store only while LR.W's reservation holds and covers this word.
    // Success or failure, the reservation is gone afterwards.
    let base = read_reg(regs, rs1);
    let value = read_reg(regs, rs2);
    let address = base as u32;

    let error_regs = *regs;
    let old_value = if M::ENABLED {
        memory
            .read_word(address)
            .map_err(|e| EmulatorError::from_memory_error(e, pc, error_regs))?
    } else {
        0
    };
    let stored = reservation.take_for_store(address);
    if stored {
        memory
            .write_word(address, value)
            .map_err(|e| EmulatorError::from_memory_error(e, pc, error_regs))?;
    }

    // 0 for success; 1 is the spec's one defined failure code.
    if rd.num() != 0 {
        regs[rd.num() as usize] = if stored { 0 } else { 1 };
    }

    let log = if M::ENABLED {
        Some(InstLog::Store {
            cycle: 0,
            pc,
            instruction: inst_word,
            rs1_val: base,
            rs2_val: value,
            addr: address,
            mem_old: old_value,
            mem_new: if stored { value } else { old_value },
        })
    } else {
        None
    };

    Ok(ExecutionResult {
        new_pc: None,
        should_halt: false,
        syscall: false,
        class: InstClass::Atomic,
        inst_size: 4,
        log,
    })
}

#[inline(always)]
fn execute_amoswap_w<M: LoggingMode, B: Bus>(
    rd: Gpr,
    rs1: Gpr,
    rs2: Gpr,
    inst_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    memory: &mut B,
) -> Result<ExecutionResult, EmulatorError> {
    // AMOSWAP.W: Atomically swap word
    let base = read_reg(regs, rs1);
    let new_value = read_reg(regs, rs2);
    let address = base as u32;

    let error_regs = *regs;
    let old_value = memory
        .read_word(address)
        .map_err(|e| EmulatorError::from_memory_error(e, pc, error_regs))?;

    memory
        .write_word(address, new_value)
        .map_err(|e| EmulatorError::from_memory_error(e, pc, error_regs))?;

    // Return old value in rd
    if rd.num() != 0 {
        regs[rd.num() as usize] = old_value;
    }

    let log = if M::ENABLED {
        Some(InstLog::Store {
            cycle: 0,
            pc,
            instruction: inst_word,
            rs1_val: base,
            rs2_val: new_value,
            addr: address,
            mem_old: old_value,
            mem_new: new_value,
        })
    } else {
        None
    };

    Ok(ExecutionResult {
        new_pc: None,
        should_halt: false,
        syscall: false,
        class: InstClass::Atomic,
        inst_size: 4,
        log,
    })
}

#[inline(always)]
fn execute_amoadd_w<M: LoggingMode, B: Bus>(
    rd: Gpr,
    rs1: Gpr,
    rs2: Gpr,
    inst_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    memory: &mut B,
) -> Result<ExecutionResult, EmulatorError> {
    // AMOADD.W: Atomically add word
    let base = read_reg(regs, rs1);
    let addend = read_reg(regs, rs2);
    let address = base as u32;

    let error_regs = *regs;
    let old_value = memory
        .read_word(address)
        .map_err(|e| EmulatorError::from_memory_error(e, pc, error_regs))?;

    let new_value = old_value.wrapping_add(addend);
    memory
        .write_word(address, new_value)
        .map_err(|e| EmulatorError::from_memory_error(e, pc, error_regs))?;

    // Return old value in rd
    if rd.num() != 0 {
        regs[rd.num() as usize] = old_value;
    }

    let log = if M::ENABLED {
        Some(InstLog::Store {
            cycle: 0,
            pc,
            instruction: inst_word,
            rs1_val: base,
            rs2_val: addend,
            addr: address,
            mem_old: old_value,
            mem_new: new_value,
        })
    } else {
        None
    };

    Ok(ExecutionResult {
        new_pc: None,
        should_halt: false,
        syscall: false,
        class: InstClass::Atomic,
        inst_size: 4,
        log,
    })
}

#[inline(always)]
fn execute_amoxor_w<M: LoggingMode, B: Bus>(
    rd: Gpr,
    rs1: Gpr,
    rs2: Gpr,
    inst_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    memory: &mut B,
) -> Result<ExecutionResult, EmulatorError> {
    // AMOXOR.W: Atomically XOR word
    let base = read_reg(regs, rs1);
    let xor_val = read_reg(regs, rs2);
    let address = base as u32;

    let error_regs = *regs;
    let old_value = memory
        .read_word(address)
        .map_err(|e| EmulatorError::from_memory_error(e, pc, error_regs))?;

    let new_value = old_value ^ xor_val;
    memory
        .write_word(address, new_value)
        .map_err(|e| EmulatorError::from_memory_error(e, pc, error_regs))?;

    // Return old value in rd
    if rd.num() != 0 {
        regs[rd.num() as usize] = old_value;
    }

    let log = if M::ENABLED {
        Some(InstLog::Store {
            cycle: 0,
            pc,
            instruction: inst_word,
            rs1_val: base,
            rs2_val: xor_val,
            addr: address,
            mem_old: old_value,
            mem_new: new_value,
        })
    } else {
        None
    };

    Ok(ExecutionResult {
        new_pc: None,
        should_halt: false,
        syscall: false,
        class: InstClass::Atomic,
        inst_size: 4,
        log,
    })
}

#[inline(always)]
fn execute_amoand_w<M: LoggingMode, B: Bus>(
    rd: Gpr,
    rs1: Gpr,
    rs2: Gpr,
    inst_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    memory: &mut B,
) -> Result<ExecutionResult, EmulatorError> {
    // AMOAND.W: Atomically AND word
    let base = read_reg(regs, rs1);
    let and_val = read_reg(regs, rs2);
    let address = base as u32;

    let error_regs = *regs;
    let old_value = memory
        .read_word(address)
        .map_err(|e| EmulatorError::from_memory_error(e, pc, error_regs))?;

    let new_value = old_value & and_val;
    memory
        .write_word(address, new_value)
        .map_err(|e| EmulatorError::from_memory_error(e, pc, error_regs))?;

    // Return old value in rd
    if rd.num() != 0 {
        regs[rd.num() as usize] = old_value;
    }

    let log = if M::ENABLED {
        Some(InstLog::Store {
            cycle: 0,
            pc,
            instruction: inst_word,
            rs1_val: base,
            rs2_val: and_val,
            addr: address,
            mem_old: old_value,
            mem_new: new_value,
        })
    } else {
        None
    };

    Ok(ExecutionResult {
        new_pc: None,
        should_halt: false,
        syscall: false,
        class: InstClass::Atomic,
        inst_size: 4,
        log,
    })
}

#[inline(always)]
fn execute_amoor_w<M: LoggingMode, B: Bus>(
    rd: Gpr,
    rs1: Gpr,
    rs2: Gpr,
    inst_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    memory: &mut B,
) -> Result<ExecutionResult, EmulatorError> {
    // AMOOR.W: Atomically OR word
    let base = read_reg(regs, rs1);
    let or_val = read_reg(regs, rs2);
    let address = base as u32;

    let error_regs = *regs;
    let old_value = memory
        .read_word(address)
        .map_err(|e| EmulatorError::from_memory_error(e, pc, error_regs))?;

    let new_value = old_value | or_val;
    memory
        .write_word(address, new_value)
        .map_err(|e| EmulatorError::from_memory_error(e, pc, error_regs))?;

    // Return old value in rd
    if rd.num() != 0 {
        regs[rd.num() as usize] = old_value;
    }

    let log = if M::ENABLED {
        Some(InstLog::Store {
            cycle: 0,
            pc,
            instruction: inst_word,
            rs1_val: base,
            rs2_val: or_val,
            addr: address,
            mem_old: old_value,
            mem_new: new_value,
        })
    } else {
        None
    };

    Ok(ExecutionResult {
        new_pc: None,
        should_halt: false,
        syscall: false,
        class: InstClass::Atomic,
        inst_size: 4,
        log,
    })
}

#[cfg(test)]
mod tests {
    use super::super::{LoggingDisabled, LoggingEnabled};
    use super::*;
    use alloc::vec;
    use lp_emu_core::{DEFAULT_RAM_START, Memory};
    use lp_riscv_inst::Gpr;

    // Helper to encode atomic instructions manually
    // Format: opcode=0x2f, funct3=0x2, funct5 in bits [31:27]
    // Atomic instructions use R-type format but with funct5 instead of funct7
    fn encode_atomic(rd: Gpr, rs1: Gpr, rs2: Gpr, funct5: u8) -> u32 {
        // Manually construct the instruction word
        // opcode[6:0] = 0x2f
        // rd[11:7] = rd
        // funct3[14:12] = 0x2
        // rs1[19:15] = rs1
        // rs2[24:20] = rs2
        // funct5[31:27] = funct5
        let opcode = 0x2f;
        let funct3 = 0x2;
        (opcode as u32)
            | ((rd.num() as u32) << 7)
            | ((funct3 as u32) << 12)
            | ((rs1.num() as u32) << 15)
            | ((rs2.num() as u32) << 20)
            | ((funct5 as u32) << 27)
    }

    fn encode_lr_w(rd: Gpr, rs1: Gpr) -> u32 {
        // LR.W: funct5=0x02, rs2=0
        encode_atomic(rd, rs1, Gpr::Zero, 0x02)
    }

    fn encode_sc_w(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
        // SC.W: funct5=0x03
        encode_atomic(rd, rs1, rs2, 0x03)
    }

    fn encode_amoswap_w(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
        // AMOSWAP.W: funct5=0x01
        encode_atomic(rd, rs1, rs2, 0x01)
    }

    fn encode_amoadd_w(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
        // AMOADD.W: funct5=0x00
        encode_atomic(rd, rs1, rs2, 0x00)
    }

    fn encode_amoxor_w(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
        // AMOXOR.W: funct5=0x04
        encode_atomic(rd, rs1, rs2, 0x04)
    }

    fn encode_amoand_w(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
        // AMOAND.W: funct5=0x0c
        encode_atomic(rd, rs1, rs2, 0x0c)
    }

    fn encode_amoor_w(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
        // AMOOR.W: funct5=0x08
        encode_atomic(rd, rs1, rs2, 0x08)
    }

    #[test]
    fn test_lr_w() {
        let mut regs = [0i32; 32];
        regs[10] = DEFAULT_RAM_START as i32; // x10 = base address
        let mut memory = Memory::with_default_addresses(vec![], vec![0u8; 1024]);
        memory.write_word(DEFAULT_RAM_START, 0x12345678).unwrap();

        let inst_word = encode_lr_w(Gpr::A0, Gpr::A0);
        let result = decode_execute_atomic::<LoggingEnabled, _>(
            inst_word,
            0x1000,
            &mut regs,
            &mut memory,
            &mut LrReservation::new(),
        )
        .unwrap();

        assert_eq!(regs[10], 0x12345678);
        assert_eq!(result.new_pc, None);
        assert_eq!(result.should_halt, false);
        assert_eq!(result.syscall, false);
    }

    #[test]
    fn sc_w_stores_and_reports_success_on_a_live_reservation() {
        let (mut regs, mut memory, mut reservation) = lr_sc_setup();

        lr(&mut regs, &mut memory, &mut reservation);
        assert_eq!(regs[12], 0xdeadbeefu32 as i32, "lr.w loaded the word");
        assert_eq!(reservation.held(), Some(DEFAULT_RAM_START));

        sc(&mut regs, &mut memory, &mut reservation);
        assert_eq!(regs[13], 0, "sc.w reports success");
        assert_eq!(memory.read_word(DEFAULT_RAM_START).unwrap(), 0x12345678);
        assert_eq!(reservation.held(), None, "a successful sc.w ends it too");
    }

    #[test]
    fn sc_w_without_an_lr_w_fails_and_leaves_memory_alone() {
        let (mut regs, mut memory, mut reservation) = lr_sc_setup();

        sc(&mut regs, &mut memory, &mut reservation);
        assert_eq!(regs[13], 1, "sc.w reports failure");
        assert_eq!(
            memory.read_word(DEFAULT_RAM_START).unwrap(),
            0xdeadbeefu32 as i32
        );
    }

    #[test]
    fn a_second_sc_w_after_one_lr_w_fails() {
        let (mut regs, mut memory, mut reservation) = lr_sc_setup();

        lr(&mut regs, &mut memory, &mut reservation);
        sc(&mut regs, &mut memory, &mut reservation);
        assert_eq!(regs[13], 0);

        regs[11] = 0x0bad_f00d;
        sc(&mut regs, &mut memory, &mut reservation);
        assert_eq!(regs[13], 1, "the first sc.w used the reservation up");
        assert_eq!(memory.read_word(DEFAULT_RAM_START).unwrap(), 0x12345678);
    }

    #[test]
    fn an_sc_w_to_another_word_fails_and_ends_the_reservation() {
        let (mut regs, mut memory, mut reservation) = lr_sc_setup();

        lr(&mut regs, &mut memory, &mut reservation);
        regs[10] = (DEFAULT_RAM_START + 4) as i32;
        sc(&mut regs, &mut memory, &mut reservation);
        assert_eq!(regs[13], 1, "the word is outside the reservation set");
        assert_eq!(memory.read_word(DEFAULT_RAM_START + 4).unwrap(), 0);

        regs[10] = DEFAULT_RAM_START as i32;
        sc(&mut regs, &mut memory, &mut reservation);
        assert_eq!(regs[13], 1, "the failed sc.w still ended the reservation");
    }

    #[test]
    fn sc_w_after_a_cleared_reservation_fails() {
        // What the hart does when it takes a trap between the two.
        let (mut regs, mut memory, mut reservation) = lr_sc_setup();

        lr(&mut regs, &mut memory, &mut reservation);
        reservation.clear();
        sc(&mut regs, &mut memory, &mut reservation);
        assert_eq!(regs[13], 1);
        assert_eq!(
            memory.read_word(DEFAULT_RAM_START).unwrap(),
            0xdeadbeefu32 as i32
        );
    }

    #[test]
    fn test_amoswap_w() {
        let mut regs = [0i32; 32];
        regs[10] = DEFAULT_RAM_START as i32; // x10 = base address
        regs[11] = 0xdeadbeefu32 as i32; // x11 = new value
        let mut memory = Memory::with_default_addresses(vec![], vec![0u8; 1024]);
        memory.write_word(DEFAULT_RAM_START, 0x12345678).unwrap();

        let inst_word = encode_amoswap_w(Gpr::A0, Gpr::A0, Gpr::A1);
        let result = decode_execute_atomic::<LoggingEnabled, _>(
            inst_word,
            0x1000,
            &mut regs,
            &mut memory,
            &mut LrReservation::new(),
        )
        .unwrap();

        assert_eq!(regs[10], 0x12345678); // Returns old value
        assert_eq!(
            memory.read_word(DEFAULT_RAM_START).unwrap(),
            0xdeadbeefu32 as i32
        );
        assert_eq!(result.new_pc, None);
    }

    #[test]
    fn test_amoadd_w() {
        let mut regs = [0i32; 32];
        regs[10] = DEFAULT_RAM_START as i32; // x10 = base address
        regs[11] = 5; // x11 = addend
        let mut memory = Memory::with_default_addresses(vec![], vec![0u8; 1024]);
        memory.write_word(DEFAULT_RAM_START, 10).unwrap();

        let inst_word = encode_amoadd_w(Gpr::A0, Gpr::A0, Gpr::A1);
        let result = decode_execute_atomic::<LoggingEnabled, _>(
            inst_word,
            0x1000,
            &mut regs,
            &mut memory,
            &mut LrReservation::new(),
        )
        .unwrap();

        assert_eq!(regs[10], 10); // Returns old value
        assert_eq!(memory.read_word(DEFAULT_RAM_START).unwrap(), 15);
        assert_eq!(result.new_pc, None);
    }

    #[test]
    fn test_amoxor_w() {
        let mut regs = [0i32; 32];
        regs[10] = DEFAULT_RAM_START as i32;
        regs[11] = 0xffff0000u32 as i32; // XOR mask
        let mut memory = Memory::with_default_addresses(vec![], vec![0u8; 1024]);
        memory.write_word(DEFAULT_RAM_START, 0x12345678).unwrap();

        let inst_word = encode_amoxor_w(Gpr::A0, Gpr::A0, Gpr::A1);
        let _result = decode_execute_atomic::<LoggingEnabled, _>(
            inst_word,
            0x1000,
            &mut regs,
            &mut memory,
            &mut LrReservation::new(),
        )
        .unwrap();

        assert_eq!(regs[10], 0x12345678); // Returns old value
        assert_eq!(
            memory.read_word(DEFAULT_RAM_START).unwrap(),
            (0x12345678u32 ^ 0xffff0000u32) as i32
        );
    }

    #[test]
    fn test_amoand_w() {
        let mut regs = [0i32; 32];
        regs[10] = DEFAULT_RAM_START as i32;
        regs[11] = 0x0000ffff; // AND mask
        let mut memory = Memory::with_default_addresses(vec![], vec![0u8; 1024]);
        memory.write_word(DEFAULT_RAM_START, 0x12345678).unwrap();

        let inst_word = encode_amoand_w(Gpr::A0, Gpr::A0, Gpr::A1);
        let _result = decode_execute_atomic::<LoggingEnabled, _>(
            inst_word,
            0x1000,
            &mut regs,
            &mut memory,
            &mut LrReservation::new(),
        )
        .unwrap();

        assert_eq!(regs[10], 0x12345678); // Returns old value
        assert_eq!(
            memory.read_word(DEFAULT_RAM_START).unwrap(),
            0x12345678 & 0x0000ffff
        );
    }

    #[test]
    fn test_amoor_w() {
        let mut regs = [0i32; 32];
        regs[10] = DEFAULT_RAM_START as i32;
        regs[11] = 0x0000ffff; // OR mask
        let mut memory = Memory::with_default_addresses(vec![], vec![0u8; 1024]);
        memory.write_word(DEFAULT_RAM_START, 0x12340000).unwrap();

        let inst_word = encode_amoor_w(Gpr::A0, Gpr::A0, Gpr::A1);
        let _result = decode_execute_atomic::<LoggingEnabled, _>(
            inst_word,
            0x1000,
            &mut regs,
            &mut memory,
            &mut LrReservation::new(),
        )
        .unwrap();

        assert_eq!(regs[10], 0x12340000); // Returns old value
        assert_eq!(
            memory.read_word(DEFAULT_RAM_START).unwrap(),
            0x12340000 | 0x0000ffff
        );
    }

    #[test]
    fn test_fast_path() {
        let mut regs = [0i32; 32];
        regs[10] = DEFAULT_RAM_START as i32;
        let mut memory = Memory::with_default_addresses(vec![], vec![0u8; 1024]);
        memory.write_word(DEFAULT_RAM_START, 0x12345678).unwrap();

        let inst_word = encode_lr_w(Gpr::A0, Gpr::A0);
        let result = decode_execute_atomic::<LoggingDisabled, _>(
            inst_word,
            0x1000,
            &mut regs,
            &mut memory,
            &mut LrReservation::new(),
        )
        .unwrap();

        assert_eq!(regs[10], 0x12345678);
        assert!(result.log.is_none()); // Fast path has no logging
    }

    /// x10 = the word's address, x11 = the value sc.w stores; the word holds
    /// 0xdeadbeef.
    fn lr_sc_setup() -> ([i32; 32], Memory, LrReservation) {
        let mut regs = [0i32; 32];
        regs[10] = DEFAULT_RAM_START as i32;
        regs[11] = 0x12345678;
        let mut memory = Memory::with_default_addresses(vec![], vec![0u8; 1024]);
        memory
            .write_word(DEFAULT_RAM_START, 0xdeadbeefu32 as i32)
            .unwrap();
        (regs, memory, LrReservation::new())
    }

    /// `lr.w x12, (x10)`
    fn lr(regs: &mut [i32; 32], memory: &mut Memory, reservation: &mut LrReservation) {
        let word = encode_lr_w(Gpr::A2, Gpr::A0);
        decode_execute_atomic::<LoggingEnabled, _>(word, 0x1000, regs, memory, reservation)
            .unwrap();
    }

    /// `sc.w x13, x11, (x10)`
    fn sc(regs: &mut [i32; 32], memory: &mut Memory, reservation: &mut LrReservation) {
        let word = encode_sc_w(Gpr::A3, Gpr::A0, Gpr::A1);
        decode_execute_atomic::<LoggingEnabled, _>(word, 0x1000, regs, memory, reservation)
            .unwrap();
    }
}
