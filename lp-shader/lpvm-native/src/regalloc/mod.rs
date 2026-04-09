//! Register allocation interface.

mod greedy;

use alloc::collections::BTreeSet;
use alloc::vec::Vec;

pub use greedy::GreedyAlloc;

use lpir::{IrFunction, VReg};

use crate::error::NativeError;
use crate::isa::rv32::abi::{CALLER_SAVED, PhysReg};
use crate::types::NativeType;
use crate::vinst::VInst;

/// Per-vreg typing for allocation (parallel to LPIR vreg_types).
#[derive(Debug, Clone)]
pub struct VRegInfo {
    pub types: Vec<NativeType>,
}

impl VRegInfo {
    pub fn count(&self) -> usize {
        self.types.len()
    }
}

impl From<&IrFunction> for VRegInfo {
    fn from(f: &IrFunction) -> Self {
        Self {
            types: f.vreg_types.iter().map(|t| NativeType::from(*t)).collect(),
        }
    }
}

/// Result of register allocation (greedy placement + call clobber set).
#[derive(Debug, Clone)]
pub struct Allocation {
    /// `vreg.0` as index -> physical register if assigned.
    pub vreg_to_phys: Vec<Option<PhysReg>>,
    pub clobbered: BTreeSet<PhysReg>,
    /// VRegs assigned to spill slots (no physical register assigned).
    pub spill_slots: Vec<VReg>,
}

impl Allocation {
    /// Number of spill slots assigned.
    pub fn spill_count(&self) -> u32 {
        self.spill_slots.len() as u32
    }

    /// Check if a vreg is spilled.
    pub fn is_spilled(&self, v: VReg) -> bool {
        self.spill_slots.contains(&v)
    }

    /// Get spill slot index for a spilled vreg.
    /// Returns `Some(slot_index)` if the vreg is spilled, `None` otherwise.
    pub fn spill_slot(&self, v: VReg) -> Option<u32> {
        self.spill_slots
            .iter()
            .position(|&sv| sv == v)
            .map(|p| p as u32)
    }
}

/// All caller-saved registers are clobbered by an outgoing call.
pub fn clobber_set_for_call() -> BTreeSet<PhysReg> {
    CALLER_SAVED.iter().copied().collect()
}

pub trait RegAlloc {
    /// Allocate registers for a function.
    ///
    /// # Arguments
    /// * `func` - The LPIR function
    /// * `vinsts` - The lowered VInsts
    /// * `arg_reg_offset` - Offset into ARG_REGS for parameter assignment.
    ///   0 for direct returns (params start at a0), 1 for sret (params start at a1)
    fn allocate(
        &self,
        func: &IrFunction,
        vinsts: &[VInst],
        arg_reg_offset: usize,
    ) -> Result<Allocation, NativeError>;
}
