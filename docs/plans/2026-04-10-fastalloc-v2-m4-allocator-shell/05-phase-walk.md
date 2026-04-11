# Phase 5: Backward Walk Shell

## Scope

Implement backward walk with stubbed decisions that logs to trace. The walk operates on the region tree.

## Implementation

### 1. Implement walk shell in `walk.rs`

```rust
use alloc::vec::Vec;
use alloc::format;
use alloc::string::ToString;
use crate::lower::Region;
use crate::isa::rv32fa::alloc::trace::{AllocTrace, TraceEntry, TraceDecision};
use crate::vinst::VInst;

/// Walk a region backward, recording stubbed decisions to trace.
/// M4: Only handles Linear regions (straight-line code).
pub fn walk_region_stub(region: &Region, vinsts: &[VInst], trace: &mut AllocTrace) {
    match region {
        Region::Linear { start, end } => {
            // Walk instructions in reverse order
            for i in (*start..*end).rev() {
                let vinst = &vinsts[i as usize];
                let entry = stub_process_instruction(i as usize, vinst);
                trace.push(entry);
            }
        }
        // M4: Only handling Linear regions
        // M5: Add IfThenElse, Loop, Seq handling
        _ => {}
    }
}

fn stub_process_instruction(vinst_idx: usize, vinst: &VInst) -> TraceEntry {
    let (decision, message) = match vinst {
        VInst::IConst32 { dst, val, .. } => {
            let decision = TraceDecision::StubAssign { vreg: dst.0 as u32, preg: 0 };
            let msg = format!("STUB: remat v{}={}", dst.0, val);
            (decision, msg)
        }
        VInst::Add32 { dst, src1, src2, .. } => {
            let decision = TraceDecision::StubAssign { vreg: dst.0 as u32, preg: 0 };
            let msg = format!("STUB: alloc v{} for def, use v{}, v{}", 
                dst.0, src1.0, src2.0);
            (decision, msg)
        }
        VInst::Sub32 { dst, src1, src2, .. } => {
            let decision = TraceDecision::StubAssign { vreg: dst.0 as u32, preg: 0 };
            let msg = format!("STUB: alloc v{} for def, use v{}, v{}", 
                dst.0, src1.0, src2.0);
            (decision, msg)
        }
        VInst::Mul32 { dst, src1, src2, .. } => {
            let decision = TraceDecision::StubAssign { vreg: dst.0 as u32, preg: 0 };
            let msg = format!("STUB: alloc v{} for def, use v{}, v{}", 
                dst.0, src1.0, src2.0);
            (decision, msg)
        }
        VInst::Mov32 { dst, src, .. } => {
            let decision = TraceDecision::StubAssign { vreg: dst.0 as u32, preg: 0 };
            let msg = format!("STUB: copy v{} to v{}", src.0, dst.0);
            (decision, msg)
        }
        VInst::Ret { vals, .. } => {
            let val_str = vals.iter().map(|v| format!("v{}", v.0)).collect::<Vec<_>>().join(", ");
            let decision = TraceDecision::StubFree { preg: 0 };
            let msg = format!("STUB: return {}", val_str);
            (decision, msg)
        }
        VInst::Call { dst, name, args, .. } => {
            let decision = TraceDecision::StubCall { callee: name.clone() };
            let msg = format!("STUB: clobber caller-saved, move args to a0-a7");
            (decision, msg)
        }
        _ => {
            let decision = TraceDecision::StubAssign { vreg: 0, preg: 0 };
            let msg = format!("STUB: unhandled {:?}", vinst.mnemonic());
            (decision, msg)
        }
    };
    
    TraceEntry {
        vinst_idx,
        vinst: vinst.clone(),
        decision,
        message: msg,
    }
}
```

### 2. Add unit tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::lower::Region;
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
    fn test_walk_linear_region() {
        let vinsts = test_vinsts();
        let region = Region::Linear { start: 0, end: 4 };
        let mut trace = AllocTrace::new();
        
        walk_region_stub(&region, &vinsts, &mut trace);
        
        // Should have 4 entries (one per instruction)
        assert_eq!(trace.entries.len(), 4);
        
        // First entry should be Ret (walked backward)
        assert!(matches!(&trace.entries[0].vinst, VInst::Ret { .. }));
        
        // Last entry should be first IConst32
        assert!(matches!(&trace.entries[3].vinst, VInst::IConst32 { dst: VReg(0), .. }));
    }
    
    #[test]
    fn test_stub_decisions_logged() {
        let vinsts = vec![
            VInst::Add32 { dst: VReg(2), src1: VReg(0), src2: VReg(1), src_op: None },
        ];
        let region = Region::Linear { start: 0, end: 1 };
        let mut trace = AllocTrace::new();
        
        walk_region_stub(&region, &vinsts, &mut trace);
        
        assert_eq!(trace.entries.len(), 1);
        assert!(trace.entries[0].message.contains("STUB"));
        assert!(trace.entries[0].message.contains("Add32"));
        assert!(matches!(trace.entries[0].decision, TraceDecision::StubAssign { .. }));
    }
    
    #[test]
    fn test_walk_then_reverse() {
        let vinsts = test_vinsts();
        let region = Region::Linear { start: 0, end: 4 };
        let mut trace = AllocTrace::new();
        
        walk_region_stub(&region, &vinsts, &mut trace);
        
        // Before reverse: entries are in backward order (Ret first)
        assert!(matches!(&trace.entries[0].vinst, VInst::Ret { .. }));
        
        // Reverse to get forward order
        trace.reverse();
        
        // After reverse: entries are in forward order (IConst32 first)
        assert!(matches!(&trace.entries[0].vinst, VInst::IConst32 { dst: VReg(0), .. }));
        assert!(matches!(&trace.entries[3].vinst, VInst::Ret { .. }));
    }
}
```

## Validate

```bash
cargo test -p lpvm-native --lib -- rv32fa::alloc::walk
```
