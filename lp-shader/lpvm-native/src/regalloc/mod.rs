//! Register allocation for the native RV32 backend.
//!
//! This module provides a straight-line register allocator using backward walk
//! with edit-list emission (regalloc2-style approach adapted for LPIR).

use crate::abi::{FuncAbi, PReg, PackedPReg, RegClass};
use crate::lower::LoweredFunction;
use alloc::vec::Vec;

pub mod debug_facade;

pub use debug_facade::{
    TraceEntry, TracePush, TraceSink, append_entry_trace_metadata_lines, trace_by_vinst_or_empty,
    trace_sink_new,
};

pub mod classes;
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
///
/// A register allocation carries its [`RegClass`] alongside the hardware
/// encoding: `Reg(int 10)` and `Reg(float 10)` are different registers on an
/// ISA with an FPU, and conflating them is the single failure mode a two-class
/// allocator exists to prevent. The pair is stored packed — see [`PackedPReg`]
/// for why — so construct with [`Alloc::reg`] / [`Alloc::int_reg`] and read
/// back with [`Alloc::preg`] rather than touching the payload directly.
///
/// The hardware encoding's *meaning* still comes from [`FuncAbi::isa()`].
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alloc {
    /// Allocated to a physical register.
    Reg(PackedPReg),
    /// Spilled to stack slot.
    Stack(u8),
    /// No allocation (dead, or never used).
    None,
}

impl Alloc {
    /// Allocation in the physical register `p`.
    pub const fn reg(p: PReg) -> Self {
        Alloc::Reg(PackedPReg::new(p))
    }

    /// Allocation in integer hardware register `hw`.
    pub const fn int_reg(hw: u8) -> Self {
        Alloc::Reg(PackedPReg::int(hw))
    }

    pub fn is_reg(self) -> bool {
        matches!(self, Alloc::Reg(_))
    }

    pub fn is_stack(self) -> bool {
        matches!(self, Alloc::Stack(_))
    }

    /// The physical register — class included — when register-allocated.
    pub fn preg(self) -> Option<PReg> {
        match self {
            Alloc::Reg(r) => Some(r.get()),
            _ => None,
        }
    }

    /// The hardware encoding when register-allocated, **ignoring class**.
    ///
    /// For callers that have already established the class (an emitter inside a
    /// class-specific arm). Anything that could see either class wants
    /// [`Alloc::preg`].
    pub fn reg_hw(self) -> Option<u8> {
        match self {
            Alloc::Reg(r) => Some(r.hw()),
            _ => None,
        }
    }

    /// Register class of this allocation, when register-allocated.
    pub fn reg_class(self) -> Option<RegClass> {
        match self {
            Alloc::Reg(r) => Some(r.class()),
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

    /// Debug trace of allocator decisions (empty / ZST when `debug` feature is off).
    pub trace: TraceSink,
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
        $crate::regalloc::AllocError::Internal(file!(), line!(), None)
    };
    ($($arg:tt)*) => {
        $crate::regalloc::AllocError::Internal(
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

/// `AllocOutput::allocs` holds one `Alloc` per operand of every instruction in
/// the function — the allocator's largest allocation, built on the device in
/// the JIT's heap. Two bytes has been the pin since the ISA-decoupling
/// refactor and the register class did not cost it: the class travels in the
/// spare high bit of [`PackedPReg`] rather than in a byte of its own.
const _: () = assert!(core::mem::size_of::<Alloc>() == 2);

/// Result of register allocation.
#[derive(Debug, Clone)]
pub struct AllocResult {
    pub output: AllocOutput,
    pub spill_slots: u32,
    /// Callee-saved GPRs (s2–s11) referenced by allocations or edits; for [`FrameLayout::compute`].
    pub used_callee_saved: crate::abi::PregSet,
}

/// Collect callee-saved pool GPRs used in `output` for prologue/epilogue.
fn used_callee_saved_from_output(output: &AllocOutput, func_abi: &FuncAbi) -> crate::abi::PregSet {
    let mut set = crate::abi::PregSet::EMPTY;
    // `PregSet` has a lane per (class, hw) pair, so a float register never
    // aliases the integer one with the same encoding here.
    let mut insert = |p: PReg| {
        if func_abi.allocatable().contains(p) && !func_abi.is_caller_saved_pool(p) {
            set.insert(p);
        }
    };

    for a in &output.allocs {
        if let Some(p) = a.preg() {
            insert(p);
        }
    }
    for (_, edit) in &output.edits {
        match edit {
            Edit::Move { from, to } => {
                if let Some(p) = from.preg() {
                    insert(p);
                }
                if let Some(p) = to.preg() {
                    insert(p);
                }
            }
            Edit::LoadIncomingArg { to, .. } => {
                if let Some(p) = to.preg() {
                    insert(p);
                }
            }
        }
    }
    set
}

/// Allocate registers for a lowered function (full region tree).
pub fn allocate(lowered: &LoweredFunction, func_abi: &FuncAbi) -> Result<AllocResult, AllocError> {
    use crate::regalloc::pool::RegPool;
    use crate::region::{REGION_ID_NONE, Region, RegionId, RegionTree};

    log::debug!(
        "[native-fa] allocate: starting for {} vinsts, region_tree.root={}",
        lowered.vinsts.len(),
        lowered.region_tree.root
    );

    let synthetic_root = lowered.region_tree.root == REGION_ID_NONE && !lowered.vinsts.is_empty();
    let owned_tree;
    let (tree, root): (&RegionTree, RegionId) = if synthetic_root {
        log::debug!(
            "[native-fa] allocate: creating synthetic linear region for {} vinsts",
            lowered.vinsts.len()
        );
        let mut t = RegionTree::new();
        let r = t.push(Region::Linear {
            start: 0,
            end: lowered.vinsts.len() as u16,
        });
        t.root = r;
        owned_tree = t;
        (&owned_tree, r)
    } else {
        log::debug!(
            "[native-fa] allocate: using existing region tree with root={}, {} nodes",
            lowered.region_tree.root,
            lowered.region_tree.nodes.len()
        );
        (&lowered.region_tree, lowered.region_tree.root)
    };

    log::debug!("[native-fa] allocate: calling allocate_from_tree...");
    let output = walk::allocate_from_tree(
        &lowered.vinsts,
        &lowered.vreg_pool,
        tree,
        root,
        func_abi,
        RegPool::for_abi(func_abi),
    )?;
    let spill_slots = output.num_spill_slots;
    let used_callee_saved = used_callee_saved_from_output(&output, func_abi);

    log::debug!(
        "[native-fa] allocate: complete, {} spill slots, {} edits",
        spill_slots,
        output.edits.len()
    );
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
    use crate::vinst::{AluOp, ModuleSymbols, SRC_OP_NONE, VInst, VReg};
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
            VInst::AluRRR {
                op: AluOp::Add,
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
        let alloc_reg = Alloc::int_reg(5);
        let alloc_stack = Alloc::Stack(0);
        let alloc_none = Alloc::None;

        assert!(alloc_reg.is_reg());
        assert!(!alloc_reg.is_stack());
        assert_eq!(alloc_reg.preg(), Some(PReg::int(5)));
        assert_eq!(alloc_reg.reg_hw(), Some(5));
        assert_eq!(alloc_reg.reg_class(), Some(RegClass::Int));

        assert!(!alloc_stack.is_reg());
        assert!(alloc_stack.is_stack());
        assert_eq!(alloc_stack.stack_slot(), Some(0));

        assert!(!alloc_none.is_reg());
        assert!(!alloc_none.is_stack());
    }

    /// The point of a class-aware `Alloc`: the same hardware encoding in the
    /// two classes must never compare equal. Every "is this operand where the
    /// ABI says it should be?" check in `verify.rs` is an `Alloc` equality.
    #[test]
    fn alloc_distinguishes_the_register_classes() {
        assert_ne!(Alloc::int_reg(10), Alloc::reg(PReg::float(10)));
        assert_eq!(
            Alloc::reg(PReg::float(10)).reg_class(),
            Some(RegClass::Float)
        );
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
        let func_abi = crate::regalloc::test::abi_fixtures::void_func_abi();
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
        use crate::regalloc::render::render_alloc_output;
        use crate::regalloc::test::abi_fixtures;
        use crate::regalloc::walk::walk_linear;

        let (vinsts, symbols, pool) = vinst::parse(input).unwrap();

        let func_abi = abi_fixtures::void_func_abi();

        let output = walk_linear(&vinsts, &pool, &func_abi).unwrap();
        let rendered = render_alloc_output(&vinsts, &pool, &output, Some(&symbols), func_abi.isa());

        // Normalize whitespace for comparison
        let expected_normalized = expected.trim().replace("\r\n", "\n");
        let actual_normalized = rendered.trim().replace("\r\n", "\n");

        assert_eq!(
            actual_normalized, expected_normalized,
            "Allocation output mismatch\nInput:\n{input}\nActual:\n{actual_normalized}",
        );
    }

    #[test]
    fn snapshot_simple_iconst_ret() {
        #[cfg(feature = "debug")]
        let expected = "i0 = IConst32 10\n; write: i0 -> Reg(t4)\n; ---------------------------\n; read: i0 <- Reg(t4)\nRet i0\n; trace: alloc: v0 -> t29";
        #[cfg(not(feature = "debug"))]
        let expected = "i0 = IConst32 10\n; write: i0 -> Reg(t4)\n; ---------------------------\n; read: i0 <- Reg(t4)\nRet i0";
        expect_alloc("i0 = IConst32 10\nRet i0", expected);
    }

    #[test]
    fn snapshot_binary_add() {
        #[cfg(feature = "debug")]
        let expected = "i0 = IConst32 10\n; write: i0 -> Reg(t4)\n; ---------------------------\ni1 = IConst32 20\n; write: i1 -> Reg(t5)\n; ---------------------------\n; read: i0 <- Reg(t4)\n; read: i1 <- Reg(t5)\ni2 = Add i0, i1\n; write: i2 -> Reg(t4)\n; trace: alloc: v0 -> t29\n; trace: alloc: v1 -> t30\n; ---------------------------\n; read: i2 <- Reg(t4)\nRet i2\n; trace: alloc: v2 -> t29";
        #[cfg(not(feature = "debug"))]
        let expected = "i0 = IConst32 10\n; write: i0 -> Reg(t4)\n; ---------------------------\ni1 = IConst32 20\n; write: i1 -> Reg(t5)\n; ---------------------------\n; read: i0 <- Reg(t4)\n; read: i1 <- Reg(t5)\ni2 = Add i0, i1\n; write: i2 -> Reg(t4)\n; ---------------------------\n; read: i2 <- Reg(t4)\nRet i2";
        expect_alloc(
            "i0 = IConst32 10\ni1 = IConst32 20\ni2 = Add i0, i1\nRet i2",
            expected,
        );
    }
}
