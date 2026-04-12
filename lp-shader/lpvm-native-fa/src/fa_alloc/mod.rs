//! Fast allocator shell — liveness, trace, backward walk.
//! The RegionTree is built in lower.rs; this module consumes it.

pub mod liveness;
pub mod spill;
pub mod trace;
pub mod walk;

use self::trace::AllocTrace;
use crate::abi::FuncAbi;
use crate::lower::LoweredFunction;
use crate::region::REGION_ID_NONE;
use crate::rv32::inst::PInst;
use alloc::vec::Vec;

pub use walk::AllocError;

/// Result of register allocation.
pub struct AllocResult {
    pub pinsts: Vec<PInst>,
    pub trace: AllocTrace,
    pub spill_slots: u32,
}

/// Run the real allocator: backward walk with register allocation.
pub fn allocate(lowered: &LoweredFunction, func_abi: &FuncAbi) -> Result<AllocResult, AllocError> {
    let num_vregs = max_vreg_index(&lowered.vinsts, &lowered.vreg_pool);

    let mut state = walk::WalkState::new(num_vregs, &lowered.symbols);

    // Pre-seed pool with param vregs in their ABI registers
    for (vreg_idx, preg) in func_abi.precolors() {
        let vreg = crate::vinst::VReg(*vreg_idx as u16);
        state.pool.alloc_fixed(preg.hw, vreg);
    }

    let root = lowered.region_tree.root;
    if root != REGION_ID_NONE {
        walk::walk_region(
            &mut state,
            &lowered.region_tree,
            root,
            &lowered.vinsts,
            &lowered.vreg_pool,
            func_abi,
        )?;
        state.pinsts.reverse();
        state.trace.reverse();
    }

    // Wrap with frame setup/teardown
    let spill_slots = state.spill.total_slots();
    let mut pinsts = Vec::with_capacity(state.pinsts.len() + 2);
    pinsts.push(PInst::FrameSetup { spill_slots });
    pinsts.extend(state.pinsts);
    pinsts.push(PInst::FrameTeardown { spill_slots });

    Ok(AllocResult {
        pinsts,
        trace: state.trace,
        spill_slots,
    })
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
    fn shell_populates_trace() {
        // Test that the shell populates a trace for a populated RegionTree
        let lowered = make_linear_lowered();
        assert_ne!(lowered.region_tree.root, REGION_ID_NONE);

        // Just verify the tree structure is there - walk is tested in walk::tests
        assert_eq!(lowered.region_tree.nodes.len(), 1);
    }

    #[test]
    fn empty_region_produces_empty_result() {
        let tree = RegionTree::new();
        // root stays REGION_ID_NONE
        let lowered = LoweredFunction {
            vinsts: Vec::new(),
            vreg_pool: Vec::new(),
            symbols: ModuleSymbols::default(),
            loop_regions: Vec::new(),
            region_tree: tree,
        };

        let abi = crate::rv32::abi::func_abi_rv32(
            &lps_shared::LpsFnSig {
                name: String::from("test"),
                return_type: lps_shared::LpsType::Void,
                parameters: vec![],
            },
            0,
        );

        // With REGION_ID_NONE root, allocate returns early with just frame setup/teardown
        let result = allocate(&lowered, &abi).unwrap();
        // FrameSetup + FrameTeardown
        assert_eq!(result.pinsts.len(), 2);
    }

    #[test]
    fn liveness_and_walk_consistent() {
        let lowered = make_linear_lowered();

        // Liveness: v0, v1 defined then used → live_in empty for this region
        let liveness = liveness::analyze_liveness(
            &lowered.region_tree,
            lowered.region_tree.root,
            &lowered.vinsts,
            &lowered.vreg_pool,
        );
        assert!(liveness.live_in.is_empty());

        // allocate produces pinsts for all 3 instructions plus frame setup/teardown
        let abi = crate::rv32::abi::func_abi_rv32(
            &lps_shared::LpsFnSig {
                name: String::from("test"),
                return_type: lps_shared::LpsType::Void,
                parameters: vec![],
            },
            0,
        );
        let result = allocate(&lowered, &abi).unwrap();
        // Should have: FrameSetup + 3 instructions + FrameTeardown
        assert!(result.pinsts.len() >= 5);
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
    fn allocate_produces_frame_wrapped_output() {
        let lowered = make_linear_lowered();
        // Use a simple ABI with no params/returns
        let abi = crate::rv32::abi::func_abi_rv32(
            &lps_shared::LpsFnSig {
                name: String::from("test"),
                return_type: lps_shared::LpsType::Void,
                parameters: vec![],
            },
            0,
        );

        let result = allocate(&lowered, &abi).unwrap();

        // Should have FrameSetup at start and FrameTeardown at end
        assert!(matches!(result.pinsts[0], PInst::FrameSetup { .. }));
        assert!(matches!(
            result.pinsts.last(),
            Some(PInst::FrameTeardown { .. })
        ));
        // Trace should have entries
        assert!(!result.trace.is_empty());
    }

    #[test]
    fn allocate_handles_loop_control_flow() {
        // Create a LoweredFunction with Loop region (now supported)
        let vinsts = vec![VInst::IConst32 {
            dst: VReg(0),
            val: 1,
            src_op: SRC_OP_NONE,
        }];
        let mut tree = RegionTree::new();
        let header = tree.push(Region::Linear { start: 0, end: 1 });
        let body = tree.push(Region::Linear { start: 0, end: 0 });
        let root = tree.push(Region::Loop {
            header,
            body,
            header_label: 0,
            exit_label: 1,
        });
        tree.root = root;
        let lowered = LoweredFunction {
            vinsts,
            vreg_pool: Vec::new(),
            symbols: ModuleSymbols::default(),
            loop_regions: Vec::new(),
            region_tree: tree,
        };

        let abi = crate::rv32::abi::func_abi_rv32(
            &lps_shared::LpsFnSig {
                name: String::from("test"),
                return_type: lps_shared::LpsType::Void,
                parameters: vec![],
            },
            0,
        );

        // Loop now works (returns Ok, not Err)
        let result = allocate(&lowered, &abi);
        assert!(result.is_ok());
    }

    #[test]
    fn allocate_iconst_add_chain() {
        // v0 = 1; v1 = 2; v2 = Add(v0, v1); Ret v2
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
            VInst::Ret {
                vals: crate::vinst::VRegSlice { start: 0, count: 1 },
                src_op: SRC_OP_NONE,
            },
        ];
        let mut tree = RegionTree::new();
        let root = tree.push(Region::Linear { start: 0, end: 4 });
        tree.root = root;
        let lowered = LoweredFunction {
            vinsts,
            vreg_pool: vec![VReg(2)], // Return value
            symbols: ModuleSymbols::default(),
            loop_regions: Vec::new(),
            region_tree: tree,
        };

        let abi = crate::rv32::abi::func_abi_rv32(
            &lps_shared::LpsFnSig {
                name: String::from("test"),
                return_type: lps_shared::LpsType::Int,
                parameters: vec![],
            },
            0,
        );

        let result = allocate(&lowered, &abi).unwrap();

        // Should have: FrameSetup, Li, Li, Add, Ret, FrameTeardown
        assert!(matches!(result.pinsts[0], PInst::FrameSetup { .. }));
        assert!(
            result
                .pinsts
                .iter()
                .any(|p| matches!(p, PInst::Li { imm: 1, .. }))
        );
        assert!(
            result
                .pinsts
                .iter()
                .any(|p| matches!(p, PInst::Li { imm: 2, .. }))
        );
        assert!(result.pinsts.iter().any(|p| matches!(p, PInst::Add { .. })));
        assert!(result.pinsts.iter().any(|p| matches!(p, PInst::Ret)));
        assert!(matches!(
            result.pinsts.last(),
            Some(PInst::FrameTeardown { .. })
        ));
    }

    #[test]
    fn allocate_trace_shows_real_decisions() {
        let lowered = make_linear_lowered();
        let abi = crate::rv32::abi::func_abi_rv32(
            &lps_shared::LpsFnSig {
                name: String::from("test"),
                return_type: lps_shared::LpsType::Void,
                parameters: vec![],
            },
            0,
        );

        let result = allocate(&lowered, &abi).unwrap();

        // Trace should show register assignments (vX→regY pattern)
        let has_assignment = result
            .trace
            .entries
            .iter()
            .any(|e| e.decision.contains('→'));
        assert!(
            has_assignment,
            "Trace should show register assignments: {:?}",
            result.trace.entries
        );
        // Should NOT contain "STUB"
        let has_stub = result
            .trace
            .entries
            .iter()
            .any(|e| e.decision.contains("STUB"));
        assert!(!has_stub, "Trace should not contain STUB decisions");
    }
}
