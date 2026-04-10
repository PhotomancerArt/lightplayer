# Phase 3: Liveness Analysis

## Scope

Implement liveness analysis for CFG and display format.

## Implementation

### 1. Implement liveness analysis in `liveness.rs`

```rust
use alloc::vec::Vec;
use alloc::collections::BTreeSet;
use crate::isa::rv32fa::alloc::cfg::{CFG, BlockId};
use crate::vinst::{VInst, VReg};

#[derive(Debug)]
pub struct BlockLiveness {
    pub live_in: BTreeSet<VReg>,
    pub live_out: BTreeSet<VReg>,
}

#[derive(Debug)]
pub struct Liveness {
    pub blocks: Vec<BlockLiveness>,
}

/// Analyze liveness per block.
/// M4: Single block - live_out is return values, live_in is all used registers.
pub fn analyze_liveness(cfg: &CFG, num_vregs: usize) -> Liveness {
    let mut blocks = Vec::new();
    
    for block in &cfg.blocks {
        let mut live_in = BTreeSet::new();
        let mut live_out = BTreeSet::new();
        
        // Collect all used vregs
        for vinst in &block.vinsts {
            for u in vinst.uses() {
                live_in.insert(u);
            }
        }
        
        // For Ret, live_out is the return values
        if let Some(last) = block.vinsts.last() {
            if let VInst::Ret { vals, .. } = last {
                for v in vals {
                    live_out.insert(*v);
                }
            }
        }
        
        blocks.push(BlockLiveness { live_in, live_out });
    }
    
    Liveness { blocks }
}

/// Format liveness for debug output.
pub fn format_liveness(liveness: &Liveness) -> String {
    let mut lines = vec!["=== Liveness ===".to_string()];
    
    for (i, block) in liveness.blocks.iter().enumerate() {
        lines.push(format!("Block {}:", i));
        
        let live_in: Vec<_> = block.live_in.iter().map(|v| format!("v{}", v.0)).collect();
        lines.push(format!("  live_in: [{}]", live_in.join(", ")));
        
        let live_out: Vec<_> = block.live_out.iter().map(|v| format!("v{}", v.0)).collect();
        lines.push(format!("  live_out: [{}]", live_out.join(", ")));
        lines.push(String::new());
    }
    
    lines.join("\n")
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
    fn test_liveness_simple() {
        let vinsts = vec![
            VInst::IConst32 { dst: VReg(0), val: 1, src_op: None },
            VInst::IConst32 { dst: VReg(1), val: 2, src_op: None },
            VInst::Add32 { dst: VReg(2), src1: VReg(0), src2: VReg(1), src_op: None },
            VInst::Ret { vals: vec![VReg(2)], src_op: None },
        ];
        
        let cfg = build_cfg(&vinsts);
        let liveness = analyze_liveness(&cfg, 3);
        
        assert_eq!(liveness.blocks.len(), 1);
        assert!(liveness.blocks[0].live_out.contains(&VReg(2)));
    }
    
    #[test]
    fn test_format_liveness() {
        let vinsts = vec![
            VInst::IConst32 { dst: VReg(0), val: 42, src_op: None },
            VInst::Ret { vals: vec![VReg(0)], src_op: None },
        ];
        
        let cfg = build_cfg(&vinsts);
        let liveness = analyze_liveness(&cfg, 1);
        let output = format_liveness(&liveness);
        
        assert!(output.contains("=== Liveness ==="));
        assert!(output.contains("live_out"));
    }
}
```

## Validate

```bash
cargo test -p lpvm-native --lib -- rv32fa::alloc::liveness
```
