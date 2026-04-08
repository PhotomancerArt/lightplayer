//! Register allocation interface.

mod greedy;

use alloc::collections::BTreeSet;
use alloc::vec::Vec;

pub use greedy::GreedyAlloc;

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

/// Result of register allocation (M1: greedy placement + call clobber set).
#[derive(Debug, Clone)]
pub struct Allocation {
    /// `vreg.0` as index -> physical register if assigned.
    pub vreg_to_phys: Vec<Option<PhysReg>>,
    pub clobbered: BTreeSet<PhysReg>,
}

/// All caller-saved registers are clobbered by an outgoing call.
pub fn clobber_set_for_call() -> BTreeSet<PhysReg> {
    CALLER_SAVED.iter().copied().collect()
}

pub trait RegAlloc {
    fn allocate(&self, vinsts: &[VInst], vreg_info: &VRegInfo) -> Allocation;
}
