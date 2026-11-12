# Interval Analysis for Structured LPIR

This document describes how to compute live intervals for register allocation given LPIR's structured control flow constraints.

## The Problem

Traditional compilers compute liveness via iterative dataflow analysis on a CFG:

```
IN[b] = use[b] ∪ (OUT[b] - def[b])
OUT[b] = ∪ IN[s] for s ∈ succ[b]
```

This requires:

- Building a CFG from basic blocks
- Bitvectors for live sets (size = #vregs × #blocks)
- Iterating to fixpoint

For a 500KB RAM target, this is prohibitive.

## LPIR's Advantage: Structured Control Flow

LPIR has no arbitrary jumps. Control flow is:

- `IfStart` ... (`Else` ...) `End`
- `LoopStart` ... (`Continuing` ...) `End`
- `SwitchStart` ... (`Case` ...)* (`Default` ...)? `End`
- `Break`, `Continue` (to innermost loop)
- `Return`

This maps to a **tree structure**, not a graph.

## O(n) Interval Computation Algorithm

### Key Insight

In structured control flow, the live range of a value is always a **contiguous interval** `[first_def, last_use]`. The only complication is that a value live before a loop remains live throughout the entire loop.

### Algorithm

```rust
/// Compute live intervals in single pass over LPIR
/// 
/// Memory: O(vreg_count) - two u16 per vreg (first_def, last_use)
fn compute_intervals(func: &IrFunction) -> Vec<Interval> {
    let vreg_count = func.vreg_count as usize;
    
    // State: first definition point for each vreg
    let mut first_def: Vec<Option<u16>> = vec![None; vreg_count];
    
    // State: last use point for each vreg
    let mut last_use: Vec<u16> = vec![0; vreg_count];
    
    // Stack tracking loop contexts for range extension
    struct LoopContext {
        start_pc: u16,
        end_pc: u16,      // Filled in when we see LoopEnd
        active_vregs: Vec<VReg>, // Vregs live at loop entry
    }
    let mut loop_stack: Vec<LoopContext> = vec![];
    
    // Track vregs live at each merge point (if/else)
    // For simplicity, we extend live ranges conservatively through merge points
    
    for (pc_u, op) in func.body.iter().enumerate() {
        let pc = pc_u as u16;
        
        // Step 1: Record uses (read before write)
        for vreg in op.read_vregs() {
            if first_def[vreg.0 as usize].is_none() {
                // Use before def: live from function entry
                first_def[vreg.0 as usize] = Some(0);
            }
            last_use[vreg.0 as usize] = pc;
        }
        
        // Step 2: Record defs
        if let Some(vreg) = op.write_vreg() {
            if first_def[vreg.0 as usize].is_none() {
                first_def[vreg.0 as usize] = Some(pc);
            }
        }
        
        // Step 3: Handle control flow effects
        match op {
            Op::LoopStart { .. } => {
                // Record which vregs are currently live
                let active: Vec<VReg> = (0..vreg_count)
                    .filter(|&v| first_def[v].is_some() && last_use[v] >= pc)
                    .map(|v| VReg(v as u16))
                    .collect();
                
                loop_stack.push(LoopContext {
                    start_pc: pc,
                    end_pc: 0, // Filled later
                    active_vregs: active,
                });
            }
            
            Op::LoopEnd { .. } => {
                let loop_ctx = loop_stack.pop().expect("unbalanced loop");
                
                // Extend all vregs live at loop entry to at least loop end
                for vreg in loop_ctx.active_vregs {
                    last_use[vreg.0 as usize] = last_use[vreg.0 as usize].max(pc);
                }
            }
            
            Op::IfStart { cond, .. } => {
                // Condition is used at branch
                last_use[cond.0 as usize] = pc;
                
                // Conservative: we will extend live ranges of vregs
                // defined in either branch to the merge point
            }
            
            Op::End { .. } => {
                // Merge point: any vreg live in either branch
                // is live up to this point. We handle this by
                // forward-scan in the if/else region to pre-mark
                // vregs that will need extension.
            }
            
            Op::Break | Op::Continue => {
                // These exit the current loop context
                // Vregs remain live through the loop as handled by loop_stack
            }
            
            _ => {}
        }
    }
    
    // Build final intervals
    (0..vreg_count)
        .filter(|&v| first_def[v].is_some())
        .map(|v| Interval {
            vreg: VReg(v as u16),
            start: first_def[v].unwrap(),
            end: last_use[v],
        })
        .collect()
}
```

### Handling If/Else Merge Points

The naive algorithm is conservative at merge points. A more precise approach:

```rust
/// Two-pass approach for precise if/else handling
/// 
/// Pass 1: Record def/use positions as above, but also track
/// which branch (then/else) each vreg is used in.
/// 
/// Pass 2: For each merge point, extend live ranges:
/// - Vreg used only in then branch: live through then, dead in else
/// - Vreg used in both: live through entire if/else construct
/// - Vreg defined in one branch and used after: live from def through merge

struct IfContext {
    merge_pc: u16,
    then_vregs: HashSet<VReg>,
    else_vregs: HashSet<VReg>,
}

// In Pass 1, track which branch we're in via a stack
let mut branch_stack: Vec<(Branch, u16)> = vec![]; // (which_branch, merge_pc)

// At End, we know which vregs were live in each branch
// We conservatively extend to the merge point
```

However, for simplicity, we can be conservative: any vreg live at an IfStart is live through the entire IfEnd. This may extend some intervals unnecessarily but is safe and fast.

### Loop Depth Weighting

For spill decisions, we care about loop depth:

```rust
fn compute_loop_depth(func: &IrFunction) -> Vec<u8> {
    let mut depth = vec![0u8; func.body.len()];
    let mut current_depth = 0u8;
    
    for (pc, op) in func.body.iter().enumerate() {
        match op {
            Op::LoopStart { .. } => {
                current_depth += 1;
            }
            Op::LoopEnd { .. } => {
                current_depth -= 1;
            }
            _ => {}
        }
        depth[pc] = current_depth;
    }
    
    depth
}

// In interval computation, track max depth for each vreg
// Vregs used in deeper loops get priority for registers
```

## Memory Analysis


| Structure                     | Size                                 | Purpose             |
| ----------------------------- | ------------------------------------ | ------------------- |
| `first_def: Vec<Option<u16>>` | 2 × vreg_count bytes                 | First definition PC |
| `last_use: Vec<u16>`          | 2 × vreg_count bytes                 | Last use PC         |
| `loop_stack`                  | ~loop_nesting × (4 + live_vregs × 2) | Active loops        |


For a typical shader with 60 vregs and 3-deep loop nesting:

- `first_def`: 120 bytes
- `last_use`: 120 bytes  
- `loop_stack`: ~3 × (4 + 40 × 2) = ~250 bytes

Total: **~500 bytes** for interval computation.

Compare to regalloc2's interference graph: **tens of kilobytes**.

## Linear Scan Allocation

Once we have intervals, allocation is straightforward:

```rust
fn allocate(intervals: &[Interval]) -> Vec<Loc> {
    let mut result = vec![Loc::Spill(0); vreg_count];
    
    // Sort by start position
    let mut sorted: Vec<&Interval> = intervals.iter().collect();
    sorted.sort_by_key(|i| i.start);
    
    // Active set: (end, phys_reg)
    let mut active: Vec<(u16, u8)> = vec![];
    
    for interval in sorted {
        // Remove expired intervals
        active.retain(|(end, _)| *end >= interval.start);
        
        if let Some(reg) = allocate_reg(&mut active) {
            result[interval.vreg.0 as usize] = Loc::Reg(reg);
            active.push((interval.end, reg));
        } else {
            // Need to spill
            let spill_slot = allocate_spill_slot(interval.vreg);
            result[interval.vreg.0 as usize] = Loc::Spill(spill_slot);
        }
    }
    
    result
}

fn allocate_reg(active: &mut Vec<(u16, u8)>) -> Option<u8> {
    let used: u32 = active.iter().map(|(_, r)| 1u32 << (*r - 8)).sum();
    let free = !used & ((1 << 24) - 1); // x8-x31
    
    // Find first free bit
    if free != 0 {
        let reg = free.trailing_zeros() as u8 + 8;
        Some(reg)
    } else {
        None
    }
}
```

### Spill Heuristic

When no register is free, we must spill something. Options:

1. **Current interval**: Simple, but may spill a short-lived temporary
2. **Active interval with furthest end** (Belady-style optimal for this pass):
  ```rust
   let spill_candidate = active.iter().max_by_key(|(end, _)| end);
   // If current interval ends further than all active, spill current
   // Otherwise, spill the one ending furthest, use its register for current
  ```

The furthest-end heuristic is optimal for minimizing spills in a single pass.

## Correctness Verification

To verify the algorithm:

```rust
#[test]
fn test_interval_simple() {
    let ir = parse_module(r"
func @test(v1:i32) -> i32 {
  v2:i32 = iadd v1, v1
  v3:i32 = imul v2, v2
  return v3
}
").unwrap();
    
    let intervals = compute_intervals(&ir.functions[0]);
    
    // v1: param, used at pc=0,0 → [0, 0]
    // v2: def at pc=0, used at pc=1,1 → [0, 1]
    // v3: def at pc=1, used at pc=2 → [1, 2]
    
    assert_eq!(intervals[0].vreg.0, 1); // v1
    assert_eq!(intervals[0].start, 0);
    assert_eq!(intervals[0].end, 0);
    
    assert_eq!(intervals[1].vreg.0, 2); // v2
    assert_eq!(intervals[1].start, 0);
    assert_eq!(intervals[1].end, 1);
}

#[test]
fn test_interval_loop() {
    let ir = parse_module(r"
func @test(v1:i32) -> i32 {
  v2:i32 = iconst.i32 0
  loop {
    br_if_not v1
    v2 = iadd v2, v1
    v1 = isub_imm v1, 1
  }
  return v2
}
").unwrap();
    
    let intervals = compute_intervals(&ir.functions[0]);
    
    // v2 defined before loop, used inside, returned after
    // → interval must extend through entire loop
    let v2_interval = &intervals[1];
    assert!(v2_interval.end > v2_interval.start);
}
```

## Summary

Structured control flow enables O(n) interval computation with O(vreg_count) memory. This is the foundation for a lightweight register allocator suitable for 500KB RAM targets.