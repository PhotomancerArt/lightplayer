# Phase 7: Integration Tests

## Scope

Add integration tests that verify the alloc/ module components work together with the region tree.

## Implementation

### 1. Update `alloc/mod.rs` with integration tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::vinst::{VInst, VReg};
    use crate::lower::Region;
    
    fn test_vinsts() -> Vec<VInst> {
        vec![
            VInst::IConst32 { dst: VReg(0), val: 1, src_op: None },
            VInst::IConst32 { dst: VReg(1), val: 2, src_op: None },
            VInst::Add32 { dst: VReg(2), src1: VReg(0), src2: VReg(1), src_op: None },
            VInst::Ret { vals: vec![VReg(2)], src_op: None },
        ]
    }
    
    #[test]
    fn test_alloc_shell_integration() {
        let vinsts = test_vinsts();
        let region = Region::Linear { start: 0, end: 4 };
        
        // Analyze liveness
        let liveness = liveness::analyze_liveness(&region, &vinsts);
        assert!(!liveness.live_in.0.is_empty());
        
        // Walk and build trace
        let mut trace = trace::AllocTrace::new();
        walk::walk_region_stub(&region, &vinsts, &mut trace);
        assert_eq!(trace.entries.len(), 4);
        
        // Reverse and verify
        trace.reverse();
        assert!(matches!(&trace.entries[0].vinst, VInst::IConst32 { dst: VReg(0), .. }));
    }
    
    #[test]
    fn test_region_format_includes_vinsts() {
        use crate::debug::region;
        
        let vinsts = test_vinsts();
        let region = Region::Linear { start: 0, end: 4 };
        let output = region::format_region(&region, &vinsts, 0);
        
        assert!(output.contains("Linear"));
        assert!(output.contains("[0..4]"));
        assert!(output.contains("IConst32"));
        assert!(output.contains("Add32"));
        assert!(output.contains("Ret"));
    }
    
    #[test]
    fn test_liveness_format() {
        let vinsts = test_vinsts();
        let region = Region::Linear { start: 0, end: 4 };
        let liveness = liveness::analyze_liveness(&region, &vinsts);
        let output = liveness::format_liveness(&liveness);
        
        assert!(output.contains("=== Liveness ==="));
        assert!(output.contains("live_in"));
        assert!(output.contains("live_out"));
        assert!(output.contains("v0") || output.contains("v1") || output.contains("v2"));
    }
    
    #[test]
    fn test_trace_format_table() {
        let vinsts = test_vinsts();
        let region = Region::Linear { start: 0, end: 4 };
        let mut trace = trace::AllocTrace::new();
        walk::walk_region_stub(&region, &vinsts, &mut trace);
        
        let output = trace.format();
        assert!(output.contains("=== AllocTrace ==="));
        assert!(output.contains("STUB") || output.contains("v0"));
    }
    
    #[test]
    fn test_empty_region() {
        let vinsts: Vec<VInst> = vec![];
        let region = Region::Linear { start: 0, end: 0 };
        
        let liveness = liveness::analyze_liveness(&region, &vinsts);
        assert!(liveness.live_in.0.is_empty());
        assert!(liveness.live_out.0.is_empty());
        
        let mut trace = trace::AllocTrace::new();
        walk::walk_region_stub(&region, &vinsts, &mut trace);
        assert!(trace.entries.is_empty());
    }
}
```

## Validate

```bash
cargo test -p lpvm-native --lib -- rv32fa::alloc
```

All tests should pass, including new integration tests.
