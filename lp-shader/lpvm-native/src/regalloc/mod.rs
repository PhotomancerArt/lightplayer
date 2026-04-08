//! Register allocation interface.

mod greedy;

use alloc::collections::BTreeSet;
use alloc::vec::Vec;

pub use greedy::GreedyAlloc;

use lpir::IrFunction;

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
}

/// All caller-saved registers are clobbered by an outgoing call.
pub fn clobber_set_for_call() -> BTreeSet<PhysReg> {
    CALLER_SAVED.iter().copied().collect()
}

pub trait RegAlloc {
    fn allocate(&self, func: &IrFunction, vinsts: &[VInst]) -> Result<Allocation, NativeError>;
}
