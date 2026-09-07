//! Instruction executor for RISC-V 32-bit instructions.

extern crate alloc;

use crate::emu::{error::EmulatorError, fp_regs::FpRegs, logging::InstLog};
use lp_emu_core::Bus;

pub use lp_emu_core::InstClass;

/// Trait for compile-time logging mode control.
pub trait LoggingMode {
    /// Whether logging is enabled for this mode.
    const ENABLED: bool;
}

/// Logging enabled mode - creates InstLog entries.
pub struct LoggingEnabled;

impl LoggingMode for LoggingEnabled {
    const ENABLED: bool = true;
}

/// Logging disabled mode - zero logging overhead.
pub struct LoggingDisabled;

impl LoggingMode for LoggingDisabled {
    const ENABLED: bool = false;
}

/// Result of executing a single instruction.
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    /// New PC value (None means PC += 4)
    pub new_pc: Option<u32>,
    /// Whether execution should stop (EBREAK)
    pub should_halt: bool,
    /// Whether a syscall was encountered (ECALL)
    pub syscall: bool,
    /// Cost class for the active [`crate::emu::CycleModel`].
    pub class: InstClass,
    /// Instruction width in bytes: `2` for compressed (RVC), `4` for 32-bit.
    pub inst_size: u8,
    /// Log entry for this instruction (None if logging is disabled).
    ///
    /// Boxed: `InstLog` is the largest field the hot path carries (40 bytes),
    /// and `LoggingDisabled` — the interpreter loop's mode — always leaves
    /// this `None`. Boxing turns that dead weight into one pointer's worth
    /// of `ExecutionResult`, which every `Ok(...)` on the hot path returns
    /// whether or not logging is on.
    pub log: Option<alloc::boxed::Box<InstLog>>,
}

/// Helper to read register (x0 always returns 0)
#[inline(always)]
pub(super) fn read_reg(regs: &[i32; 32], reg: lp_riscv_inst::Gpr) -> i32 {
    if reg.num() == 0 {
        0
    } else {
        regs[reg.num() as usize]
    }
}

/// Main dispatch function for decode-execute fusion.
///
/// Decodes the instruction word and executes it in a single step,
/// eliminating the intermediate `Inst` enum allocation.
///
/// `fp` carries the RV32F architectural state (see [`FpRegs`]). Only the
/// floating-point arms and the three F-extension CSRs in [`system`] read or
/// write it; the integer categories never see it.
#[inline(always)]
pub(crate) fn decode_execute<M: LoggingMode, B: Bus>(
    inst_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    _memory: &mut B,
    fp: &mut FpRegs,
) -> Result<ExecutionResult, EmulatorError> {
    // Check if compressed instruction (bits [1:0] != 0b11)
    if (inst_word & 0x3) != 0x3 {
        return compressed::decode_execute_compressed::<M, B>(inst_word, pc, regs, _memory);
    }

    let opcode = (inst_word & 0x7f) as u8;

    match opcode {
        0x33 => {
            // R-type (arithmetic)
            arithmetic::decode_execute_rtype::<M, B>(inst_word, pc, regs, _memory)
        }
        0x13 => {
            // I-type (immediate arithmetic/logical/shift)
            immediate::decode_execute_itype::<M, B>(inst_word, pc, regs, _memory)
        }
        0x03 => {
            // Load instructions
            load_store::decode_execute_load::<M, B>(inst_word, pc, regs, _memory)
        }
        0x23 => {
            // Store instructions
            load_store::decode_execute_store::<M, B>(inst_word, pc, regs, _memory)
        }
        0x63 => {
            // Branch instructions
            branch::decode_execute_branch::<M, B>(inst_word, pc, regs, _memory)
        }
        0x6f => {
            // JAL
            jump::decode_execute_jal::<M, B>(inst_word, pc, regs, _memory)
        }
        0x67 => {
            // JALR
            jump::decode_execute_jalr::<M, B>(inst_word, pc, regs, _memory)
        }
        0x37 => {
            // LUI
            jump::decode_execute_lui::<M, B>(inst_word, pc, regs, _memory)
        }
        0x17 => {
            // AUIPC
            jump::decode_execute_auipc::<M, B>(inst_word, pc, regs, _memory)
        }
        0x73 => {
            // System instructions (ECALL, EBREAK, CSR)
            system::decode_execute_system::<M, B>(inst_word, pc, regs, _memory, fp)
        }
        0x0f => {
            // FENCE/FENCE.I instructions
            system::decode_execute_fence::<M, B>(inst_word, pc, regs, _memory)
        }
        0x2f => {
            // Atomic instructions (A extension)
            atomic::decode_execute_atomic::<M, B>(inst_word, pc, regs, _memory)
        }
        // Floating point (F extension). `lp-riscv-inst` has no F support, so
        // `float` decodes the word itself. Compressed float encodings (Zcf)
        // are deliberately not implemented — see `float`'s module docs.
        float::OPCODE_LOAD_FP
        | float::OPCODE_STORE_FP
        | float::OPCODE_MADD
        | float::OPCODE_MSUB
        | float::OPCODE_NMSUB
        | float::OPCODE_NMADD
        | float::OPCODE_OP_FP => {
            float::decode_execute_float::<M, B>(inst_word, pc, regs, _memory, fp)
        }
        _ => Err(EmulatorError::InvalidInstruction {
            pc,
            instruction: inst_word,
            reason: alloc::format!("Unknown opcode: 0x{opcode:02x}"),
            regs: alloc::boxed::Box::new(*regs),
        }),
    }
}

// Category modules
pub mod arithmetic;
pub mod atomic;
pub mod branch;
pub mod compressed;
pub mod float;
pub mod immediate;
pub mod jump;
pub mod load_store;
pub mod system;

#[cfg(test)]
mod tests {
    //! `decode_execute` driven through a non-`Memory` `Bus`, standing in for
    //! the brief's `tests/bus_trait.rs`: `decode_execute` is `pub(crate)`,
    //! so an external integration test can't reach it — this lives here
    //! instead, exercising the exact trait boundary P2/P3 will implement.

    use super::*;
    use crate::emu::fp_regs::FpRegs;
    use alloc::{vec, vec::Vec};
    use lp_emu_core::{MemoryAccessKind, MemoryError};
    use lp_riscv_inst::{Gpr, encode};

    /// A `Bus` test double, not backed by [`lp_emu_core::Memory`]: one fixed
    /// address always faults (`InvalidAccess`), another always fires a
    /// watchpoint (`Watchpoint`), everything else is a flat byte array.
    /// `take_sideband` reports `true` exactly once, right after a store —
    /// proving the side-band plumbing the privileged stepper (P2) will read
    /// after Store-class instructions actually reaches the executor.
    struct TestBus {
        ram: Vec<u8>,
        fault_addr: u32,
        watch_addr: u32,
        sideband: bool,
    }

    impl TestBus {
        fn new() -> Self {
            Self {
                ram: vec![0u8; 4096],
                fault_addr: 0x100,
                watch_addr: 0x200,
                sideband: false,
            }
        }
    }

    impl Bus for TestBus {
        fn fetch_instruction(&mut self, _address: u32) -> Result<u32, MemoryError> {
            unimplemented!("decode_execute takes the instruction word directly")
        }

        fn read_word(&mut self, address: u32) -> Result<i32, MemoryError> {
            if address == self.fault_addr {
                return Err(MemoryError::InvalidAccess {
                    address,
                    size: 4,
                    kind: MemoryAccessKind::Read,
                });
            }
            if address == self.watch_addr {
                return Err(MemoryError::Watchpoint {
                    address,
                    kind: MemoryAccessKind::Read,
                    slot: 0,
                });
            }
            let o = address as usize;
            Ok(i32::from_le_bytes([
                self.ram[o],
                self.ram[o + 1],
                self.ram[o + 2],
                self.ram[o + 3],
            ]))
        }

        fn read_halfword(&mut self, address: u32) -> Result<i16, MemoryError> {
            let o = address as usize;
            Ok(i16::from_le_bytes([self.ram[o], self.ram[o + 1]]))
        }

        fn read_byte(&mut self, address: u32) -> Result<i8, MemoryError> {
            Ok(self.ram[address as usize] as i8)
        }

        fn read_u8(&mut self, address: u32) -> Result<u8, MemoryError> {
            Ok(self.ram[address as usize])
        }

        fn write_word(&mut self, address: u32, value: i32) -> Result<(), MemoryError> {
            if address == self.fault_addr {
                return Err(MemoryError::InvalidAccess {
                    address,
                    size: 4,
                    kind: MemoryAccessKind::Write,
                });
            }
            if address == self.watch_addr {
                return Err(MemoryError::Watchpoint {
                    address,
                    kind: MemoryAccessKind::Write,
                    slot: 0,
                });
            }
            let o = address as usize;
            self.ram[o..o + 4].copy_from_slice(&value.to_le_bytes());
            self.sideband = true;
            Ok(())
        }

        fn write_halfword(&mut self, address: u32, value: i16) -> Result<(), MemoryError> {
            let o = address as usize;
            self.ram[o..o + 2].copy_from_slice(&value.to_le_bytes());
            Ok(())
        }

        fn write_byte(&mut self, address: u32, value: i8) -> Result<(), MemoryError> {
            self.ram[address as usize] = value as u8;
            Ok(())
        }

        fn take_sideband(&mut self) -> bool {
            core::mem::take(&mut self.sideband)
        }
    }

    fn run(bus: &mut TestBus, inst_word: u32) -> Result<ExecutionResult, EmulatorError> {
        let mut regs = [0i32; 32];
        regs[10] = 0; // x10 = base address 0, so lw/sw imm is the address
        let mut fp = FpRegs::new();
        decode_execute::<LoggingDisabled, _>(inst_word, 0x1000, &mut regs, bus, &mut fp)
    }

    #[test]
    fn a_bus_fault_propagates_out_of_decode_execute() {
        let mut bus = TestBus::new();
        let inst_word = encode::lw(Gpr::A1, Gpr::A0, bus.fault_addr as i32);
        let err = run(&mut bus, inst_word).unwrap_err();
        assert!(matches!(
            err,
            EmulatorError::InvalidMemoryAccess { address, .. } if address == bus.fault_addr
        ));
    }

    #[test]
    fn a_watchpoint_hit_propagates_as_its_own_error_kind() {
        let mut bus = TestBus::new();
        let inst_word = encode::sw(Gpr::A0, Gpr::A1, bus.watch_addr as i32);
        let err = run(&mut bus, inst_word).unwrap_err();
        assert!(matches!(
            err,
            EmulatorError::Watchpoint { address, .. } if address == bus.watch_addr
        ));
    }

    #[test]
    fn take_sideband_is_observed_after_a_store_but_not_a_load() {
        let mut bus = TestBus::new();
        assert!(!bus.take_sideband(), "no store yet");

        let sw = encode::sw(Gpr::A0, Gpr::A1, 0x300);
        run(&mut bus, sw).unwrap();
        assert!(
            bus.take_sideband(),
            "a store must raise the bus's side-band flag"
        );
        assert!(!bus.take_sideband(), "take_sideband consumes the flag");

        let lw = encode::lw(Gpr::A1, Gpr::A0, 0x300);
        run(&mut bus, lw).unwrap();
        assert!(!bus.take_sideband(), "a load must not raise the side-band");
    }
}
