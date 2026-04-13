//! Fast allocator shell — liveness, trace, backward walk.
//! The RegionTree is built in lower.rs; this module consumes it.

pub mod liveness;
pub mod spill;
pub mod trace;
pub mod pool;

use self::trace::AllocTrace;
use crate::abi::FuncAbi;
use crate::lower::LoweredFunction;
use alloc::vec::Vec;

pub use pool::RegPool;

/// Where an operand lives: physical register, spill slot, or unassigned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alloc {
    /// Assigned to this physical register.
    Reg(crate::rv32::gpr::PReg),
    /// Spilled to this slot (0-based, FP-relative).
    Stack(u8),
    /// Unassigned (shouldn't happen after successful allocation).
    None,
}

impl Alloc {
    pub fn is_reg(self) -> bool {
        matches!(self, Alloc::Reg(_))
    }
    pub fn is_stack(self) -> bool {
        matches!(self, Alloc::Stack(_))
    }
    pub fn reg(self) -> Option<crate::rv32::gpr::PReg> {
        match self {
            Alloc::Reg(r) => Some(r),
            _ => None,
        }
    }
    pub fn stack_slot(self) -> Option<u8> {
        match self {
            Alloc::Stack(s) => Some(s),
            _ => None,
        }
    }
}

/// Position relative to a VInst where an edit is inserted.
/// Sorts by instruction index first, then by position (Before < After).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditPoint {
    Before(u16),  // VInst index
    After(u16),
}

impl EditPoint {
    pub fn inst(self) -> u16 {
        match self {
            EditPoint::Before(i) | EditPoint::After(i) => i,
        }
    }
}

impl PartialOrd for EditPoint {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for EditPoint {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        // Compare by instruction index first
        let self_idx = self.inst();
        let other_idx = other.inst();
        match self_idx.cmp(&other_idx) {
            core::cmp::Ordering::Equal => {
                // At same index, Before comes before After
                match (self, other) {
                    (EditPoint::Before(_), EditPoint::Before(_)) => core::cmp::Ordering::Equal,
                    (EditPoint::Before(_), EditPoint::After(_)) => core::cmp::Ordering::Less,
                    (EditPoint::After(_), EditPoint::Before(_)) => core::cmp::Ordering::Greater,
                    (EditPoint::After(_), EditPoint::After(_)) => core::cmp::Ordering::Equal,
                }
            }
            ord => ord,
        }
    }
}

/// An edit: move value from one allocation to another.
/// Covers spill (reg → stack), reload (stack → reg), and reg-reg moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edit {
    Move { from: Alloc, to: Alloc },
}

/// Allocator output: per-operand assignments and edits to insert.
/// Following regalloc2's `Output` structure.
pub struct AllocOutput {
    /// Flat array of allocations: allocs[(inst_idx, operand_idx)].
    /// Use `inst_alloc_offsets` to find the start for each instruction.
    pub allocs: Vec<Alloc>,

    /// Offset into `allocs` for each instruction's operands.
    /// `inst_alloc_offsets[i]` is the index where instruction i's allocations start.
    pub inst_alloc_offsets: Vec<u16>,

    /// Edits to insert between instructions, sorted by EditPoint.
    pub edits: Vec<(EditPoint, Edit)>,

    /// Total spill slots needed for this function.
    pub num_spill_slots: u32,

    /// Allocator trace for debugging.
    pub trace: AllocTrace,
}

impl AllocOutput {
    /// Get the allocation for a specific operand of an instruction.
    pub fn operand_alloc(&self, inst: u16, operand_idx: u16) -> Alloc {
        let offset = self.inst_alloc_offsets[inst as usize];
        self.allocs[offset as usize + operand_idx as usize]
    }

    /// Set the allocation for a specific operand.
    pub fn set_operand_alloc(&mut self, inst: u16, operand_idx: u16, alloc: Alloc) {
        let offset = self.inst_alloc_offsets[inst as usize];
        self.allocs[offset as usize + operand_idx as usize] = alloc;
    }
}

/// Allocation error types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AllocError {
    NotImplemented,
    TooManyVRegs,
    UnsupportedControlFlow,
    OutOfRegisters,
}

impl core::fmt::Display for AllocError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            AllocError::NotImplemented => write!(f, "allocator not yet implemented (M1)"),
            AllocError::TooManyVRegs => write!(f, "too many virtual registers"),
            AllocError::UnsupportedControlFlow => write!(f, "unsupported control flow"),
            AllocError::OutOfRegisters => write!(f, "out of registers"),
        }
    }
}

impl core::error::Error for AllocError {}

/// Result of register allocation (placeholder for M1).
pub struct AllocResult {
    pub trace: AllocTrace,
    pub spill_slots: u32,
}

/// Stub allocator: returns NotImplemented error.
/// TODO(M2): Implement real backward walk allocator.
pub fn allocate(_lowered: &LoweredFunction, _func_abi: &FuncAbi) -> Result<AllocResult, AllocError> {
    Err(AllocError::NotImplemented)
}

fn max_vreg_index(vinsts: &[crate::vinst::VInst], pool: &[crate::vinst::VReg]) -> usize {
    let mut m = 0usize;
    for inst in vinsts {
        inst.for_each_use(pool, |u| m = m.max(u.0 as usize + 1));
        inst.for_each_def(pool, |d| m = m.max(d.0 as usize + 1));
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::region::{Region, RegionTree};
    use crate::vinst::{ModuleSymbols, SRC_OP_NONE, VInst, VReg};
    use alloc::string::String;
    use alloc::vec::Vec;

    fn make_linear_lowered() -> LoweredFunction {
        let vinsts = vec![
            VInst::IConst32 {
                dst: VReg(0),
                val: 1,
                src_op: SRC_OP_NONE,
            },
            VInst::IConst32 {
                dst: VReg(1),
                val: 2,
                src_op: SRC_OP_NONE,
            },
            VInst::Add32 {
                dst: VReg(2),
                src1: VReg(0),
                src2: VReg(1),
                src_op: SRC_OP_NONE,
            },
        ];
        let mut tree = RegionTree::new();
        let root = tree.push(Region::Linear { start: 0, end: 3 });
        tree.root = root;

        LoweredFunction {
            vinsts,
            vreg_pool: Vec::new(),
            symbols: ModuleSymbols::default(),
            loop_regions: Vec::new(),
            region_tree: tree,
        }
    }

    #[test]
    fn alloc_types_exist() {
        // Verify the new Alloc types compile and work
        let alloc_reg = Alloc::Reg(5);
        let alloc_stack = Alloc::Stack(0);
        let alloc_none = Alloc::None;

        assert!(alloc_reg.is_reg());
        assert!(!alloc_reg.is_stack());
        assert_eq!(alloc_reg.reg(), Some(5));

        assert!(!alloc_stack.is_reg());
        assert!(alloc_stack.is_stack());
        assert_eq!(alloc_stack.stack_slot(), Some(0));

        assert!(!alloc_none.is_reg());
        assert!(!alloc_none.is_stack());
    }

    #[test]
    fn edit_point_ordering() {
        let p1 = EditPoint::Before(1);
        let p2 = EditPoint::After(1);
        let p3 = EditPoint::Before(2);

        assert!(p1 < p2);
        assert!(p2 < p3);
    }

    #[test]
    fn stub_allocator_returns_not_implemented() {
        // M1: allocator is stubbed and returns NotImplemented
        let lowered = make_linear_lowered();
        let abi = crate::rv32::abi::func_abi_rv32(
            &lps_shared::LpsFnSig {
                name: String::from("test"),
                return_type: lps_shared::LpsType::Void,
                parameters: vec![],
            },
            0,
        );

        let result = allocate(&lowered, &abi);
        assert!(matches!(result, Err(AllocError::NotImplemented)));
    }

    #[test]
    fn region_format_includes_vinsts() {
        let lowered = make_linear_lowered();
        let output = crate::rv32::debug::region::format_region_tree(
            &lowered.region_tree,
            lowered.region_tree.root,
            &lowered.vinsts,
            &lowered.vreg_pool,
            &lowered.symbols,
            0,
        );

        assert!(output.contains("Linear [0..3)"));
        assert!(output.contains("IConst32"));
        assert!(output.contains("Add32"));
    }

    #[test]
    fn liveness_analysis_works() {
        let lowered = make_linear_lowered();

        // Liveness: v0, v1 defined then used → live_in empty for this region
        let liveness = liveness::analyze_liveness(
            &lowered.region_tree,
            lowered.region_tree.root,
            &lowered.vinsts,
            &lowered.vreg_pool,
        );
        assert!(liveness.live_in.is_empty());
    }
}
