# Phase 3: Liveness Analysis

## Scope

Implement recursive liveness analysis for the region tree and display format.

## Implementation

### 1. Implement recursive liveness in `liveness.rs`

```rust
use alloc::vec::Vec;
use alloc::collections::BTreeSet;
use alloc::format;
use crate::lower::Region;
use crate::vinst::{VInst, VReg};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveSet(pub BTreeSet<VReg>);

impl LiveSet {
    pub fn new() -> Self {
        Self(BTreeSet::new())
    }
    
    pub fn union(&self, other: &LiveSet) -> LiveSet {
        LiveSet(self.0.union(&other.0).cloned().collect())
    }
    
    pub fn remove(&mut self, vreg: VReg) {
        self.0.remove(&vreg);
    }
    
    pub fn insert(&mut self, vreg: VReg) {
        self.0.insert(vreg);
    }
    
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Liveness result for a region.
#[derive(Debug)]
pub struct Liveness {
    pub live_in: LiveSet,
    pub live_out: LiveSet,
}

/// Compute liveness for a single instruction.
/// Returns (uses, defs) for the instruction.
fn instruction_liveness(vinst: &VInst) -> (Vec<VReg>, Vec<VReg>) {
    match vinst {
        VInst::IConst32 { dst, .. } => (vec![], vec![*dst]),
        VInst::Add32 { dst, src1, src2, .. } => {
            (vec![*src1, *src2], vec![*dst])
        }
        VInst::Sub32 { dst, src1, src2, .. } => {
            (vec![*src1, *src2], vec![*dst])
        }
        VInst::Mul32 { dst, src1, src2, .. } => {
            (vec![*src1, *src2], vec![*dst])
        }
        VInst::Mov32 { dst, src, .. } => (vec![*src], vec![*dst]),
        VInst::Ret { vals, .. } => (vals.clone(), vec![]),
        VInst::Call { dst, args, .. } => {
            let mut uses = args.clone();
            if let Some(d) = dst {
                (uses, vec![*d])
            } else {
                (uses, vec![])
            }
        }
        _ => (vec![], vec![]),
    }
}

/// Analyze liveness recursively on region tree.
/// M4: Single Linear region (straight-line code).
pub fn analyze_liveness(region: &Region, vinsts: &[VInst]) -> Liveness {
    match region {
        Region::Linear { start, end } => {
            let mut live = LiveSet::new();
            
            // Walk instructions backward
            for i in (*start..*end).rev() {
                let vinst = &vinsts[i as usize];
                let (uses, defs) = instruction_liveness(vinst);
                
                // Remove defs (they're dead before this point)
                for d in defs {
                    live.remove(d);
                }
                // Add uses (they're live before this point)
                for u in uses {
                    live.insert(u);
                }
            }
            
            Liveness {
                live_in: live.clone(),
                live_out: live,
            }
        }
        
        // M4: Only handling Linear regions
        // M5: Add IfThenElse, Loop, Seq handling
        _ => Liveness {
            live_in: LiveSet::new(),
            live_out: LiveSet::new(),
        }
    }
}

/// Format liveness for debug output.
pub fn format_liveness(liveness: &Liveness) -> String {
    let mut lines = vec!["=== Liveness ===".to_string()];
    
    let live_in: Vec<_> = liveness.live_in.0.iter().map(|v| format!("v{}", v.0)).collect();
    lines.push(format!("  live_in: [{}]", live_in.join(", ")));
    
    let live_out: Vec<_> = liveness.live_out.0.iter().map(|v| format!("v{}", v.0)).collect();
    lines.push(format!("  live_out: [{}]", live_out.join(", ")));
    
    lines.join("\n")
}
```

### 2. Add unit tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::lower::Region;
    use crate::vinst::{VInst, VReg};
    
    #[test]
    fn test_liveness_simple() {
        let vinsts = vec![
            VInst::IConst32 { dst: VReg(0), val: 1, src_op: None },
            VInst::IConst32 { dst: VReg(1), val: 2, src_op: None },
            VInst::Add32 { dst: VReg(2), src1: VReg(0), src2: VReg(1), src_op: None },
            VInst::Ret { vals: vec![VReg(2)], src_op: None },
        ];
        
        let region = Region::Linear { start: 0, end: 4 };
        let liveness = analyze_liveness(&region, &vinsts);
        
        // live_in should contain all registers that are used
        assert!(liveness.live_in.0.contains(&VReg(0)));
        assert!(liveness.live_in.0.contains(&VReg(1)));
        assert!(liveness.live_in.0.contains(&VReg(2)));
    }
    
    #[test]
    fn test_liveness_defs_killed() {
        let vinsts = vec![
            VInst::IConst32 { dst: VReg(0), val: 1, src_op: None },
            VInst::IConst32 { dst: VReg(0), val: 2, src_op: None },  // Redefines v0
            VInst::Ret { vals: vec![VReg(0)], src_op: None },
        ];
        
        let region = Region::Linear { start: 0, end: 3 };
        let liveness = analyze_liveness(&region, &vinsts);
        
        // First IConst32's def is killed by second, so only v0 should be live
        assert_eq!(liveness.live_in.0.len(), 1);
        assert!(liveness.live_in.0.contains(&VReg(0)));
    }
    
    #[test]
    fn test_format_liveness() {
        let vinsts = vec![
            VInst::IConst32 { dst: VReg(0), val: 42, src_op: None },
            VInst::Ret { vals: vec![VReg(0)], src_op: None },
        ];
        
        let region = Region::Linear { start: 0, end: 2 };
        let liveness = analyze_liveness(&region, &vinsts);
        let output = format_liveness(&liveness);
        
        assert!(output.contains("=== Liveness ==="));
        assert!(output.contains("live_out"));
        assert!(output.contains("v0"));
    }
}
```

## Validate

```bash
cargo test -p lpvm-native --lib -- rv32fa::alloc::liveness
```
