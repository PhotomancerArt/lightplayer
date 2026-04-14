//! Register allocation for fastalloc.
//!
//! This module provides a straight-line register allocator using backward walk
//! with edit-list emission (regalloc2-style approach adapted for LPIR).

use crate::abi::FuncAbi;
use crate::fa_alloc::trace::AllocTrace;
use crate::lower::LoweredFunction;
use alloc::vec::Vec;

pub mod liveness;
pub mod pool;
pub mod render;
pub mod spill;
pub mod trace;
pub mod verify;
pub mod walk;

#[cfg(test)]
pub mod test;

/// Allocation location for a virtual register operand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alloc {
    /// Allocated to a physical register.
    Reg(crate::rv32::gpr::PReg),
    /// Spilled to stack slot.
    Stack(u8),
    /// No allocation (dead, or never used).
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

/// Edit point relative to a VInst (instruction index in block).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditPoint {
    /// Before the instruction executes.
    Before(u16),
    /// After the instruction executes.
    After(u16),
}

/// Manual Ord implementation for correct sorting order.
/// Sorts by instruction index first, then by position (Before < After).
impl PartialOrd for EditPoint {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for EditPoint {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        match (self, other) {
            (EditPoint::Before(a), EditPoint::Before(b))
            | (EditPoint::After(a), EditPoint::After(b)) => a.cmp(b),
            (EditPoint::Before(a), EditPoint::After(b)) => {
                // Same instruction: Before comes before After
                // Different instruction: compare instruction indices
                match a.cmp(b) {
                    core::cmp::Ordering::Equal => core::cmp::Ordering::Less,
                    other => other,
                }
            }
            (EditPoint::After(a), EditPoint::Before(b)) => {
                // Same instruction: After comes after Before
                // Different instruction: compare instruction indices
                match a.cmp(b) {
                    core::cmp::Ordering::Equal => core::cmp::Ordering::Greater,
                    other => other,
                }
            }
        }
    }
}

/// A single allocation edit (insertion) to be applied during emission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edit {
    /// Move value between allocations.
    Move { from: Alloc, to: Alloc },
    /// Load an incoming stack-passed parameter from the caller's frame.
    /// `fp_offset` is the byte offset from FP (positive, in the caller's area).
    LoadIncomingArg { fp_offset: i32, to: Alloc },
}

/// Complete output of the allocator: per-operand allocs + edit list.
#[derive(Clone, Debug)]
pub struct AllocOutput {
    /// Flat table of per-operand allocations.
    /// Indexed by `inst_alloc_offsets[inst] + operand_index`.
    pub allocs: Vec<Alloc>,

    /// Per-instruction operand count offsets into `allocs`.
    pub inst_alloc_offsets: Vec<u16>,

    /// Edits to apply during emission.
    /// Sorted by EditPoint (Before < After at same instruction).
    pub edits: Vec<(EditPoint, Edit)>,

    /// Number of spill slots needed.
    pub num_spill_slots: u32,

    /// Debug trace of allocator decisions.
    pub trace: AllocTrace,
}

impl AllocOutput {
    /// Get the allocation for a specific operand.
    pub fn operand_alloc(&self, inst_idx: u16, operand_idx: u16) -> Alloc {
        let offset = self.inst_alloc_offsets[inst_idx as usize] as usize;
        self.allocs[offset + operand_idx as usize]
    }
}

use alloc::string::String;

/// Allocator errors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AllocError {
    /// Internal error with file:line context and optional message.
    Internal(&'static str, u32, Option<String>),
    TooManyVRegs,
    UnsupportedControlFlow,
    OutOfRegisters,
}

/// Build an [`AllocError::Internal`] capturing the call site.
/// Usage: `emit_err!()` or `emit_err!("slot {} not found", slot_id)`
#[macro_export]
macro_rules! emit_err {
    () => {
        $crate::fa_alloc::AllocError::Internal(file!(), line!(), None)
    };
    ($($arg:tt)*) => {
        $crate::fa_alloc::AllocError::Internal(
            file!(),
            line!(),
            Some(alloc::format!($($arg)*))
        )
    };
}

impl core::fmt::Display for AllocError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            AllocError::Internal(file, line, None) => {
                write!(f, "internal error at {file}:{line}")
            }
            AllocError::Internal(file, line, Some(msg)) => {
                write!(f, "internal error at {file}:{line}: {msg}")
            }
            AllocError::TooManyVRegs => write!(f, "too many virtual registers"),
            AllocError::UnsupportedControlFlow => write!(f, "unsupported control flow"),
            AllocError::OutOfRegisters => write!(f, "out of physical registers"),
        }
    }
}

impl core::error::Error for AllocError {}

/// Result of register allocation.
#[derive(Debug, Clone)]
pub struct AllocResult {
    pub output: AllocOutput,
    pub spill_slots: u32,
    /// Callee-saved GPRs (s2–s11) referenced by allocations or edits; for [`FrameLayout::compute`].
    pub used_callee_saved: crate::abi::PregSet,
}

/// Collect callee-saved pool GPRs (x18–x27) used in `output` for prologue/epilogue.
fn used_callee_saved_from_output(output: &AllocOutput) -> crate::abi::PregSet {
    use crate::abi::PReg as AbiPReg;
    use crate::rv32::gpr;

    let mut set = crate::abi::PregSet::EMPTY;
    let mut insert = |r: crate::rv32::gpr::PReg| {
        if gpr::is_callee_saved_pool_gpr(r) {
            set.insert(AbiPReg::int(r));
        }
    };

    for a in &output.allocs {
        if let Alloc::Reg(r) = a {
            insert(*r);
        }
    }
    for (_, edit) in &output.edits {
        match edit {
            Edit::Move { from, to } => {
                if let Alloc::Reg(r) = from {
                    insert(*r);
                }
                if let Alloc::Reg(r) = to {
                    insert(*r);
                }
            }
            Edit::LoadIncomingArg { to, .. } => {
                if let Alloc::Reg(r) = to {
                    insert(*r);
                }
            }
        }
    }
    set
}

/// Allocate registers for a lowered function (full region tree).
pub fn allocate(lowered: &LoweredFunction, func_abi: &FuncAbi) -> Result<AllocResult, AllocError> {
    use crate::fa_alloc::pool::RegPool;
    use crate::region::{REGION_ID_NONE, Region, RegionId, RegionTree};

    let synthetic_root = lowered.region_tree.root == REGION_ID_NONE && !lowered.vinsts.is_empty();
    let owned_tree;
    let (tree, root): (&RegionTree, RegionId) = if synthetic_root {
        let mut t = RegionTree::new();
        let r = t.push(Region::Linear {
            start: 0,
            end: lowered.vinsts.len() as u16,
        });
        t.root = r;
        owned_tree = t;
        (&owned_tree, r)
    } else {
        (&lowered.region_tree, lowered.region_tree.root)
    };

    let output = walk::allocate_from_tree(
        &lowered.vinsts,
        &lowered.vreg_pool,
        tree,
        root,
        func_abi,
        RegPool::new(),
    )?;
    let spill_slots = output.num_spill_slots;
    let used_callee_saved = used_callee_saved_from_output(&output);

    Ok(AllocResult {
        output,
        spill_slots,
        used_callee_saved,
    })
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
            lpir_slots: Vec::new(),
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
        // Same instruction: Before < After
        let before_5 = EditPoint::Before(5);
        let after_5 = EditPoint::After(5);
        assert!(before_5 < after_5);
        assert!(after_5 > before_5);

        // Different instructions: compare by instruction index
        let before_3 = EditPoint::Before(3);
        let before_7 = EditPoint::Before(7);
        assert!(before_3 < before_7);

        let after_2 = EditPoint::After(2);
        let before_5 = EditPoint::Before(5);
        assert!(after_2 < before_5);
    }

    #[test]
    fn allocator_works_for_linear_regions() {
        let lowered = make_linear_lowered();
        let func_abi = crate::rv32::abi::func_abi_rv32(
            &lps_shared::LpsFnSig {
                name: String::from("test"),
                return_type: lps_shared::LpsType::Void,
                parameters: Vec::new(),
            },
            0,
        );
        let result = allocate(&lowered, &func_abi);
        assert!(result.is_ok(), "allocator should work for Linear regions");
        let alloc_result = result.unwrap();
        // 3 vregs (0, 1, 2) but no spills needed for simple linear
        assert_eq!(alloc_result.spill_slots, 0);
    }

    #[test]
    fn liveness_runs_on_lowered() {
        let lowered = make_linear_lowered();
        let liveness = liveness::analyze_liveness(
            &lowered.region_tree,
            lowered.region_tree.root,
            &lowered.vinsts,
            &lowered.vreg_pool,
        );
        assert!(liveness.live_in.is_empty());
    }

    // Snapshot test helpers for allocator
    fn expect_alloc(input: &str, expected: &str) {
        use crate::debug::vinst;
        use crate::fa_alloc::render::render_alloc_output;
        use crate::fa_alloc::walk::walk_linear;
        use crate::rv32::abi;
        use lps_shared::{LpsFnSig, LpsType};

        let (vinsts, symbols, pool) = vinst::parse(input).unwrap();

        // Create a simple ABI with no params
        let func_abi = abi::func_abi_rv32(
            &LpsFnSig {
                name: String::from("test"),
                return_type: LpsType::Void,
                parameters: Vec::new(),
            },
            0,
        );

        let output = walk_linear(&vinsts, &pool, &func_abi).unwrap();
        let rendered = render_alloc_output(&vinsts, &pool, &output, Some(&symbols));

        // Normalize whitespace for comparison
        let expected_normalized = expected.trim().replace("\r\n", "\n");
        let actual_normalized = rendered.trim().replace("\r\n", "\n");

        assert_eq!(
            actual_normalized, expected_normalized,
            "Allocation output mismatch\nInput:\n{}\nActual:\n{}",
            input, actual_normalized
        );
    }

    #[test]
    fn snapshot_simple_iconst_ret() {
        expect_alloc(
            "i0 = IConst32 10\nRet i0",
            "i0 = IConst32 10
; write: i0 -> t4
; ---------------------------
; read: i0 <- t4
Ret i0
; trace: alloc: v0 -> t29",
        );
    }

    #[test]
    fn snapshot_binary_add() {
        expect_alloc(
            "i0 = IConst32 10\ni1 = IConst32 20\ni2 = Add32 i0, i1\nRet i2",
            "i0 = IConst32 10
; write: i0 -> t5
; ---------------------------
i1 = IConst32 20
; write: i1 -> t6
; ---------------------------
; read: i0 <- t5
; read: i1 <- t6
i2 = Add32 i0, i1
; write: i2 -> t4
; trace: alloc: v0 -> t30
; trace: alloc: v1 -> t31
; ---------------------------
; read: i2 <- t4
Ret i2
; trace: alloc: v2 -> t29",
        );
    }
}
