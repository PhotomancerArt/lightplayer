# Phase 5: Greedy Allocator Live Value Limit

## Scope

Add maximum live value check to `GreedyAlloc` to prevent incorrect code generation when the allocator exhausts available registers. Returns error if live set exceeds 24 values (our available x8-x31 pool).

## Code Organization

- Update `allocate()` in `regalloc/greedy.rs`
- Add error variant to `error.rs`
- Tests at bottom of greedy.rs

## Implementation Details

Add to `error.rs`:
```rust
#[derive(Debug, Clone)]
pub enum NativeError {
    // ... existing variants ...
    TooManyLiveValues { count: usize, max: usize },
}

impl core::fmt::Display for NativeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            // ... existing arms ...
            NativeError::TooManyLiveValues { count, max } => {
                write!(f, "too many live values: {} (max {})", count, max)
            }
        }
    }
}
```

Update `regalloc/greedy.rs`:
```rust
impl RegAlloc for GreedyAlloc {
    fn allocate(&self, func: &IrFunction) -> Result<Allocation, NativeError> {
        // Available registers: x8-x31 = 24 registers
        const MAX_LIVE: usize = 24;
        const ALLOCA_REGS: [PReg; 24] = [
            PReg(8), PReg(9), PReg(10), PReg(11),
            PReg(12), PReg(13), PReg(14), PReg(15),
            PReg(16), PReg(17), PReg(18), PReg(19),
            PReg(20), PReg(21), PReg(22), PReg(23),
            PReg(24), PReg(25), PReg(26), PReg(27),
            PReg(28), PReg(29), PReg(30), PReg(31),
        ];
        
        let mut per_op = Vec::with_capacity(func.body.len());
        
        for op in &func.body {
            // Compute live vregs at this point
            let live = compute_live_vregs(op, &per_op);
            
            // Check limit
            if live.len() > MAX_LIVE {
                return Err(NativeError::TooManyLiveValues {
                    count: live.len(),
                    max: MAX_LIVE,
                });
            }
            
            // Allocate each vreg to physical register (round-robin)
            let mut map = vec![0u8; func.vreg_types.len()];
            for (i, vreg) in live.iter().enumerate() {
                map[*vreg as usize] = ALLOCA_REGS[i % ALLOCA_REGS.len()].0;
            }
            
            per_op.push(map);
        }
        
        Ok(Allocation { per_op })
    }
}

/// Compute which vregs are live at the current operation.
/// Simple analysis: all vregs defined but not yet used.
fn compute_live_vregs(op: &Op, prior_allocs: &[Vec<u8>]) -> Vec<u16> {
    // TODO: Proper liveness analysis for M3+ (linear scan)
    // For M2: just return all vregs that have been defined
    // This is conservative (over-approximates live set)
    
    let mut defined = alloc::collections::BTreeSet::new();
    let mut used = alloc::collections::BTreeSet::new();
    
    // Scan ops to find defined/used vregs
    // ... implementation ...
    
    // Live = defined - used
    defined.difference(&used).cloned().collect()
}
```

## Simplified M2 Approach

For M2 POC, we can use a simpler limit check since `op-add.glsl` is tiny:
```rust
impl RegAlloc for GreedyAlloc {
    fn allocate(&self, func: &IrFunction) -> Result<Allocation, NativeError> {
        const MAX_LIVE: usize = 24;
        
        // Conservative: total vregs in function must fit in registers
        // (assumes no spilling needed for simple shaders)
        if func.vreg_types.len() > MAX_LIVE {
            return Err(NativeError::TooManyLiveValues {
                count: func.vreg_types.len(),
                max: MAX_LIVE,
            });
        }
        
        // Simple round-robin allocation (one allocation per function for M2)
        let mut map = Vec::with_capacity(func.vreg_types.len());
        for i in 0..func.vreg_types.len() {
            let preg = 8 + (i % 24) as u8; // x8-x31
            map.push(preg);
        }
        
        Ok(Allocation {
            per_op: alloc::vec![map; func.body.len()],
        })
    }
}
```

## Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::NativeType;
    use alloc::vec;
    
    #[test]
    fn test_allocate_within_limit() {
        let alloc = GreedyAlloc;
        let func = IrFunction {
            name: "test".into(),
            vreg_types: vec![NativeType::I32; 10], // 10 vregs OK
            slots: vec![],
            body: vec![Op::Return { val: Some(0) }],
        };
        
        let result = alloc.allocate(&func);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().per_op.len(), 1);
    }
    
    #[test]
    fn test_allocate_exceeds_limit() {
        let alloc = GreedyAlloc;
        let func = IrFunction {
            name: "test".into(),
            vreg_types: vec![NativeType::I32; 30], // 30 vregs exceeds 24
            slots: vec![],
            body: vec![Op::Return { val: Some(0) }],
        };
        
        let result = alloc.allocate(&func);
        assert!(result.is_err());
        match result {
            Err(NativeError::TooManyLiveValues { count, max }) => {
                assert_eq!(count, 30);
                assert_eq!(max, 24);
            }
            _ => panic!("expected TooManyLiveValues error"),
        }
    }
}
```

## Key Points

- Conservative limit: total vregs must fit in 24 physical registers
- Simple per-function allocation (no per-op liveness for M2)
- Error message is clear for debugging

## Validate

```bash
cargo test -p lpvm-native --lib regalloc::greedy::tests
cargo check -p lpvm-native
```
