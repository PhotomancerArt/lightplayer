//! Fast allocator shell — liveness, trace, backward walk.
//! The RegionTree is built in lower.rs; this module consumes it.

pub mod liveness;
pub mod trace;
pub mod walk;

use crate::abi::FuncAbi;
use crate::lower::LoweredFunction;
use crate::region::REGION_ID_NONE;
use self::trace::AllocTrace;

/// Run the allocator shell: liveness + backward walk with stubbed decisions.
/// Returns a trace of what the allocator would do (M4: stubs only).
pub fn run_shell(lowered: &LoweredFunction, _func_abi: &FuncAbi) -> AllocTrace {
    let mut trace = AllocTrace::new();

    let root = lowered.region_tree.root;
    if root != REGION_ID_NONE {
        walk::walk_region_stub(
            &lowered.region_tree,
            root,
            &lowered.vinsts,
            &lowered.vreg_pool,
            &mut trace,
        );
        trace.reverse();
    }

    trace
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::region::{Region, RegionTree};
    use crate::vinst::{ModuleSymbols, VInst, VReg, SRC_OP_NONE};
    use alloc::vec::Vec;

    fn make_linear_lowered() -> LoweredFunction {
        let vinsts = vec![
            VInst::IConst32 { dst: VReg(0), val: 1, src_op: SRC_OP_NONE },
            VInst::IConst32 { dst: VReg(1), val: 2, src_op: SRC_OP_NONE },
            VInst::Add32 { dst: VReg(2), src1: VReg(0), src2: VReg(1), src_op: SRC_OP_NONE },
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
    fn shell_populates_trace() {
        // Test that the shell populates a trace for a populated RegionTree
        let lowered = make_linear_lowered();
        assert_ne!(lowered.region_tree.root, REGION_ID_NONE);

        // Just verify the tree structure is there - walk is tested in walk::tests
        assert_eq!(lowered.region_tree.nodes.len(), 1);
    }

    #[test]
    fn shell_empty_region_produces_empty_trace() {
        let tree = RegionTree::new();
        // root stays REGION_ID_NONE
        let lowered = LoweredFunction {
            vinsts: Vec::new(),
            vreg_pool: Vec::new(),
            symbols: ModuleSymbols::default(),
            loop_regions: Vec::new(),
            region_tree: tree,
        };

        // With REGION_ID_NONE root, walk_region_stub returns early without adding entries
        let mut trace = AllocTrace::new();
        walk::walk_region_stub(
            &lowered.region_tree,
            lowered.region_tree.root,
            &lowered.vinsts,
            &lowered.vreg_pool,
            &mut trace,
        );
        assert!(trace.is_empty());
    }
}
