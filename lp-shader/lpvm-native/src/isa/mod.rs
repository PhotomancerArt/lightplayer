pub mod rv32;

use object::elf;
use object::Architecture;

/// The target ISA + sub-architecture for a compiled module.
///
/// Variant names describe the **target hardware**, not the codegen output.
/// `Rv32imac` is the ESP32-C6 target (`riscv32imac-unknown-none-elf`); the
/// emitter currently produces only base RV32IM instructions. The A and C
/// extensions appear in the target name because the firmware runtime uses
/// them, not because we emit them.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum IsaTarget {
    Rv32imac,
}

impl IsaTarget {
    /// Pool-init order for the register allocator's LRU.
    pub fn allocatable_pool_order(self) -> &'static [u8] {
        match self {
            IsaTarget::Rv32imac => crate::isa::rv32::gpr::ALLOC_POOL,
        }
    }

    /// True if `p` is in the allocatable register pool.
    pub fn is_in_allocatable_pool(self, p: u8) -> bool {
        match self {
            IsaTarget::Rv32imac => crate::isa::rv32::gpr::pool_contains(p),
        }
    }

    /// Human-readable name for `p` (debug rendering only).
    pub fn reg_name(self, p: u8) -> &'static str {
        match self {
            IsaTarget::Rv32imac => crate::isa::rv32::gpr::reg_name(p),
        }
    }

    /// True if a return value with `scalar_count` scalars uses the
    /// sret-via-buffer convention rather than direct registers.
    pub fn sret_uses_buffer_for(self, scalar_count: u32) -> bool {
        match self {
            IsaTarget::Rv32imac => {
                (scalar_count as usize) > crate::isa::rv32::abi::SRET_SCALAR_THRESHOLD
            }
        }
    }

    /// Minimum stack frame alignment in bytes.
    pub fn stack_alignment(self) -> u32 {
        match self {
            IsaTarget::Rv32imac => crate::isa::rv32::abi::STACK_ALIGNMENT,
        }
    }

    /// `object` crate Architecture for ELF emission.
    pub fn elf_architecture(self) -> Architecture {
        match self {
            IsaTarget::Rv32imac => Architecture::Riscv32,
        }
    }

    /// e_flags value for ELF header.
    pub fn elf_e_flags(self) -> u32 {
        match self {
            IsaTarget::Rv32imac => elf::EF_RISCV_FLOAT_ABI_SOFT,
        }
    }

    /// Caller-saved GPR indices within the allocatable pool (clobbered across calls).
    pub fn caller_saved_pool_hw(self) -> &'static [u8] {
        match self {
            IsaTarget::Rv32imac => crate::isa::rv32::gpr::CALLER_SAVED_POOL,
        }
    }

    /// Hardware index for the `idx`-th scalar return register for direct (non-sret) returns.
    pub fn direct_ret_reg_hw(self, idx: usize) -> Option<u8> {
        match self {
            IsaTarget::Rv32imac => crate::isa::rv32::gpr::RET_REGS.get(idx).copied(),
        }
    }

    /// Count of direct return registers in the hardware ABI (e.g. 2 for RV32 a0–a1).
    pub fn direct_ret_reg_count(self) -> usize {
        match self {
            IsaTarget::Rv32imac => crate::isa::rv32::gpr::RET_REGS.len(),
        }
    }

    /// Hardware index for the `idx`-th incoming call argument register.
    pub fn call_arg_reg_hw(self, idx: usize) -> Option<u8> {
        match self {
            IsaTarget::Rv32imac => crate::isa::rv32::gpr::ARG_REGS.get(idx).copied(),
        }
    }

    /// Number of argument registers in the hardware calling convention.
    pub fn call_arg_reg_count(self) -> usize {
        match self {
            IsaTarget::Rv32imac => crate::isa::rv32::gpr::ARG_REGS.len(),
        }
    }
}
