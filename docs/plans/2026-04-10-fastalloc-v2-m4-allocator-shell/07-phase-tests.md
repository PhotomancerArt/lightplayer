# Phase 7: Integration Tests

## Scope

Add integration tests verifying the alloc/ shell components work together with real lowered output.

## Implementation

### 1. Add integration tests in `alloc/mod.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::region::Region;
    use crate::vinst::{VInst, VReg, SRC_OP_NONE};

    fn make_linear_lowered() -> LoweredFunction {
        let vinsts = vec![
            VInst::IConst32 { dst: VReg(0), val: 1, src_op: SRC_OP_NONE },
            VInst::IConst32 { dst: VReg(1), val: 2, src_op: SRC_OP_NONE },
            VInst::Add32 { dst: VReg(2), src1: VReg(0), src2: VReg(1), src_op: SRC_OP_NONE },
        ];
        let mut tree = crate::region::RegionTree::new();
        let root = tree.push(Region::Linear { start: 0, end: 3 });
        tree.root = root;

        LoweredFunction {
            vinsts,
            vreg_pool: Vec::new(),
            symbols: crate::vinst::ModuleSymbols::default(),
            loop_regions: Vec::new(),
            region_tree: tree,
        }
    }

    #[test]
    fn shell_produces_trace() {
        let lowered = make_linear_lowered();
        let func_abi = /* minimal FuncAbi */;
        let trace = run_shell(&lowered, &func_abi);

        assert_eq!(trace.entries.len(), 3);
        // Forward order after reverse
        assert_eq!(trace.entries[0].vinst_idx, 0);
        assert_eq!(trace.entries[2].vinst_idx, 2);
    }

    #[test]
    fn shell_empty_region() {
        let mut tree = crate::region::RegionTree::new();
        // root stays REGION_ID_NONE
        let lowered = LoweredFunction {
            vinsts: Vec::new(),
            vreg_pool: Vec::new(),
            symbols: crate::vinst::ModuleSymbols::default(),
            loop_regions: Vec::new(),
            region_tree: tree,
        };
        let func_abi = /* minimal FuncAbi */;
        let trace = run_shell(&lowered, &func_abi);
        assert!(trace.is_empty());
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

        // Walk produces trace for all 3 instructions
        let mut trace = trace::AllocTrace::new();
        walk::walk_region_stub(
            &lowered.region_tree,
            lowered.region_tree.root,
            &lowered.vinsts,
            &lowered.vreg_pool,
            &mut trace,
        );
        assert_eq!(trace.entries.len(), 3);
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

        assert!(output.contains("Linear"));
        assert!(output.contains("IConst32"));
        assert!(output.contains("Add32"));
    }
}
```

### 2. End-to-end test with real GLSL lowering

```rust
#[test]
fn shell_with_real_lowered_function() {
    // Use test infrastructure to compile a simple GLSL function
    // and verify the shell produces a non-empty trace
    let (ir, sig) = /* compile simple GLSL */;
    let abi = ModuleAbi::from_ir_and_sig(&ir, &sig);
    let func = &ir.functions[0];
    let lowered = lower_ops(func, &ir, &abi, FloatMode::Q32)?;

    assert_ne!(lowered.region_tree.root, crate::region::REGION_ID_NONE);

    let func_abi = crate::rv32::abi::func_abi_rv32(func, &abi.slot_kinds);
    let trace = run_shell(&lowered, &func_abi);
    assert!(!trace.is_empty());
}
```

## Validate

```bash
cargo test -p lpvm-native-fa --lib -- alloc
```
