# Phase 2: CFG Construction and Display

## Scope

Implement CFG building for VInst sequences and text format for debug display.

## Implementation

### 1. Implement `build_cfg()` in `cfg.rs`

```rust
/// Build CFG from VInsts.
/// M4: Single block containing all VInsts (straight-line only).
pub fn build_cfg(vinsts: &[VInst]) -> CFG {
    let block = BasicBlock {
        id: BlockId(0),
        start: 0,
        end: vinsts.len(),
        vinsts: vinsts.to_vec(),
        preds: Vec::new(),
        succs: Vec::new(),
    };

    CFG {
        blocks: vec![block],
        entry: BlockId(0),
    }
}

impl CFG {
    /// Check if CFG has control flow (more than one block or branches).
    pub fn has_control_flow(&self) -> bool {
        self.blocks.len() > 1 ||
        self.blocks[0].vinsts.iter().any(|v| {
            matches!(v, VInst::Br { .. } | VInst::BrIf { .. })
        })
    }

    pub fn block(&self, id: BlockId) -> &BasicBlock {
        &self.blocks[id.0]
    }
}
```

### 2. Implement `format_cfg()` in `cfg.rs`

```rust
use crate::debug::vinst;

/// Format CFG for debug output.
pub fn format_cfg(cfg: &CFG) -> String {
    let mut lines = vec!["=== CFG ===".to_string()];
    
    for block in &cfg.blocks {
        lines.push(format!(
            "Block {}: [VInst {}..{}]", 
            block.id.0, block.start, block.end
        ));
        
        for (i, vinst) in block.vinsts.iter().enumerate() {
            let global_idx = block.start + i;
            let vinst_text = vinst::format_vinst(vinst);
            lines.push(format!("  {}: {}", global_idx, vinst_text));
        }
        
        if !block.preds.is_empty() {
            let preds: Vec<_> = block.preds.iter().map(|b| b.0.to_string()).collect();
            lines.push(format!("  preds: [{}]", preds.join(", ")));
        }
        if !block.succs.is_empty() {
            let succs: Vec<_> = block.succs.iter().map(|b| b.0.to_string()).collect();
            lines.push(format!("  succs: [{}]", succs.join(", ")));
        }
        lines.push(String::new());
    }
    
    lines.join("\n")
}
```

### 3. Add unit tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::vinst::{VInst, VReg};
    
    #[test]
    fn test_cfg_single_block() {
        let vinsts = vec![
            VInst::IConst32 { dst: VReg(0), val: 42, src_op: None },
            VInst::Ret { vals: vec![VReg(0)], src_op: None },
        ];
        
        let cfg = build_cfg(&vinsts);
        assert_eq!(cfg.blocks.len(), 1);
        assert_eq!(cfg.blocks[0].vinsts.len(), 2);
        assert!(!cfg.has_control_flow());
    }
    
    #[test]
    fn test_cfg_detects_branch() {
        let vinsts = vec![
            VInst::Br { target: 1, src_op: None },
            VInst::Label(1, None),
            VInst::Ret { vals: vec![VReg(0)], src_op: None },
        ];
        
        let cfg = build_cfg(&vinsts);
        assert!(cfg.has_control_flow());
    }
    
    #[test]
    fn test_format_cfg() {
        let vinsts = vec![
            VInst::IConst32 { dst: VReg(0), val: 42, src_op: None },
            VInst::Ret { vals: vec![VReg(0)], src_op: None },
        ];
        
        let cfg = build_cfg(&vinsts);
        let output = format_cfg(&cfg);
        assert!(output.contains("=== CFG ==="));
        assert!(output.contains("Block 0"));
        assert!(output.contains("IConst32"));
    }
}
```

## Validate

```bash
cargo test -p lpvm-native --lib -- rv32fa::alloc::cfg
```
