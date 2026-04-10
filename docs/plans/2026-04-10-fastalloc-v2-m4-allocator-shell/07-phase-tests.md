# Phase 7: Integration Tests

## Scope

Add integration tests that verify the alloc/ module components work together.

## Implementation

### 1. Update `alloc/mod.rs` with integration tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::vinst::{VInst, VReg};
    
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
        
        // Build CFG
        let cfg = cfg::build_cfg(&vinsts);
        assert_eq!(cfg.blocks.len(), 1);
        
        // Analyze liveness
        let num_vregs = 3;
        let liveness = liveness::analyze_liveness(&cfg, num_vregs);
        assert_eq!(liveness.blocks.len(), 1);
        
        // Walk and build trace
        let mut trace = trace::AllocTrace::new();
        walk::walk_block_stub(&cfg.blocks[0], &mut trace);
        assert_eq!(trace.entries.len(), 4);
        
        // Reverse and verify
        trace.reverse();
        assert_eq!(trace.entries[0].vinst_mnemonic, "IConst32");
    }
    
    #[test]
    fn test_cfg_format_includes_vinsts() {
        let vinsts = test_vinsts();
        let cfg = cfg::build_cfg(&vinsts);
        let output = cfg::format_cfg(&cfg);
        
        assert!(output.contains("=== CFG ==="));
        assert!(output.contains("Block 0"));
        assert!(output.contains("IConst32"));
        assert!(output.contains("Add32"));
        assert!(output.contains("Ret"));
    }
    
    #[test]
    fn test_liveness_format() {
        let vinsts = test_vinsts();
        let cfg = cfg::build_cfg(&vinsts);
        let liveness = liveness::analyze_liveness(&cfg, 3);
        let output = liveness::format_liveness(&liveness);
        
        assert!(output.contains("=== Liveness ==="));
        assert!(output.contains("live_in"));
        assert!(output.contains("live_out"));
    }
    
    #[test]
    fn test_trace_format_table() {
        let vinsts = test_vinsts();
        let cfg = cfg::build_cfg(&vinsts);
        let mut trace = trace::AllocTrace::new();
        walk::walk_block_stub(&cfg.blocks[0], &mut trace);
        
        let output = trace.format();
        assert!(output.contains("=== AllocTrace ==="));
        assert!(output.contains("STUB"));
    }
}
```

## Validate

```bash
cargo test -p lpvm-native --lib -- rv32fa::alloc
```

All tests should pass, including new integration tests.
