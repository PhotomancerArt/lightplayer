# Phase 5: Backward Walk Shell

## Scope

Implement backward walk with stubbed decisions that logs to trace.

## Implementation

### 1. Implement walk shell in `walk.rs`

```rust
use alloc::vec::Vec;
use alloc::format;
use crate::isa::rv32fa::alloc::cfg::BasicBlock;
use crate::isa::rv32fa::alloc::trace::{AllocTrace, TraceEntry};
use crate::vinst::VInst;

/// Walk a block backward, recording stubbed decisions to trace.
pub fn walk_block_stub(block: &BasicBlock, trace: &mut AllocTrace) {
    // Walk instructions in reverse order
    for (offset, vinst) in block.vinsts.iter().enumerate().rev() {
        let global_idx = block.start + offset;
        let entry = stub_process_instruction(global_idx, vinst);
        trace.push(entry);
    }
}

fn stub_process_instruction(vinst_idx: usize, vinst: &VInst) -> TraceEntry {
    let (mnemonic, decision) = match vinst {
        VInst::IConst32 { dst, val, .. } => {
            ("IConst32", format!("STUB: remat v{}={}", dst.0, val))
        }
        VInst::Add32 { dst, src1, src2, .. } => {
            ("Add32", format!("STUB: alloc v{} for def, use v{}, v{}", 
                dst.0, src1.0, src2.0))
        }
        VInst::Mov32 { dst, src, .. } => {
            ("Mov32", format!("STUB: copy v{} to v{}", src.0, dst.0))
        }
        VInst::Ret { vals, .. } => {
            let val_str = vals.iter().map(|v| format!("v{}", v.0)).collect::<Vec<_>>().join(", ");
            ("Ret", format!("STUB: return {}", val_str))
        }
        VInst::Call { .. } => {
            ("Call", "STUB: clobber caller-saved, move args".to_string())
        }
        _ => (vinst.mnemonic(), "STUB: (unhandled)".to_string()),
    };
    
    TraceEntry {
        vinst_idx,
        vinst_mnemonic: mnemonic.to_string(),
        decision,
        register_state: "(stub)".to_string(),
    }
}
```

### 2. Add unit tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::isa::rv32fa::alloc::cfg::build_cfg;
    use crate::vinst::{VInst, VReg};
    
    #[test]
    fn test_walk_block() {
        let vinsts = vec![
            VInst::IConst32 { dst: VReg(0), val: 1, src_op: None },
            VInst::IConst32 { dst: VReg(1), val: 2, src_op: None },
            VInst::Add32 { dst: VReg(2), src1: VReg(0), src2: VReg(1), src_op: None },
            VInst::Ret { vals: vec![VReg(2)], src_op: None },
        ];
        
        let cfg = build_cfg(&vinsts);
        let mut trace = AllocTrace::new();
        
        walk_block_stub(&cfg.blocks[0], &mut trace);
        
        // Should have 4 entries (one per instruction)
        assert_eq!(trace.entries.len(), 4);
        
        // Reverse to get forward order
        trace.reverse();
        assert_eq!(trace.entries[0].vinst_mnemonic, "IConst32");
        assert_eq!(trace.entries[3].vinst_mnemonic, "Ret");
    }
    
    #[test]
    fn test_stub_decisions_logged() {
        let vinsts = vec![
            VInst::Add32 { dst: VReg(2), src1: VReg(0), src2: VReg(1), src_op: None },
        ];
        
        let cfg = build_cfg(&vinsts);
        let mut trace = AllocTrace::new();
        walk_block_stub(&cfg.blocks[0], &mut trace);
        
        assert!(trace.entries[0].decision.contains("STUB"));
        assert!(trace.entries[0].decision.contains("Add32"));
    }
}
```

## Validate

```bash
cargo test -p lpvm-native --lib -- rv32fa::alloc::walk
```
