# M4: Allocator Shell - Notes

## Scope of Work

Build the allocator structure: **region tree CFG** built during lowering, recursive liveness analysis for structured control flow, trace system, and backward walk shell. The allocator produces PInsts with stubbed-out decisions (no real allocation yet).

## Current State

The allocator currently exists as a simple forward-walk implementation in `rv32fa/alloc.rs`:

- Single-pass forward allocation
- Last-use register freeing
- Basic parameter precoloring
- Simple register pool management

This works for straight-line code but lacks:
- Proper CFG representation (even for single block)
- Liveness analysis
- Trace/debug system
- Backward-walk allocation

## Design Evolution: Flat CFG → Region Tree

**Original thought:** Build a flat CFG with BasicBlocks containing VInst vectors, preds/succs as Vec<BlockId>. This is the standard approach but has issues for our use case:

- Requires VInst copies into blocks
- Fixed-point iteration for liveness
- ~40 bytes per block overhead
- Loses structured control flow information

**New direction:** Build a **region tree** during lowering that preserves LPIR's structured control flow:

- Zero VInst copies (indices only)
- Recursive descent liveness (no fixed-point needed)
- ~8-24 bytes per region
- Natural representation for IfThenElse, Loop, Seq

## M4 Goals

Per the roadmap (`m4-allocator-shell.md`), M4 should create:

1. **Region tree in lowerer** (`lower.rs`): Build Region enum alongside VInst slice
2. **Region display** (`debug/region.rs`): Human-readable region tree format
3. **Liveness module** (`alloc/liveness.rs`): Recursive liveness on region tree
4. **Trace module** (`alloc/trace.rs`): AllocTrace system for debugging decisions
5. **Walk module** (`alloc/walk.rs`): Backward walk shell with stubbed decisions
6. **Main alloc module** (`alloc/mod.rs`): FastAlloc entry point

## Questions

### Q1: Should we keep the current simple allocator?

**Answer:** Replace completely. The current allocator was a placeholder. Build the new backward-walk allocator with region tree, liveness, and trace system.

### Q2: Do we need CFG for straight-line code?

**Answer:** Yes, but use region tree instead of flat CFG. The region representation is critical infrastructure:
- Built during lowering (free)
- Well tested
- Has CLI debug support (`--show-region` flag)
- Lays groundwork for M5 (control flow support)

Even for straight-line code, we get a Linear region. For control flow, IfThenElse and Loop regions.

### Q3: Trace detail level?

**Answer:** Maximum debug detail. The trace must show:
- All layers: LPIR index, VInst, resulting PInst(s)
- All decisions: register assignment, spill decisions, reloads, frees, evictions
- Register state: which vreg is in which preg at each step
- Visual sanity check: formatted for human readability

This enables quick debugging and demonstrates the allocator is working correctly.

### Q4: What's the M4 vs M5 boundary?

**Answer:** 
- **M4 (Shell):** Build infrastructure (region tree, liveness, trace), walk backward, make stubbed decisions
- **M5 (Core):** Implement real allocation (LRU, spill, reload, call clobbers)

M4 is about having the structure in place so M5 can focus on the allocation algorithm.

### Q5: Why region tree over flat CFG?

**Answer:** 

| Aspect | Region Tree | Flat CFG |
|--------|-------------|----------|
| Build | During lowering (free) | Separate pass required |
| VInst copies | 0 (indices only) | Copied into blocks |
| Per-node overhead | 8-24 bytes (indices) | 40+ bytes (Vec fields) |
| Liveness | Recursive descent | Fixed-point iteration |
| Structured code | Natural | Requires reconstruction |
| Embedded memory | Better | Worse |

Since we lower from structured LPIR, preserving that structure is natural and efficient.

### Q6: How does recursive liveness work?

**Answer:**

```rust
fn liveness(region: &Region, vinsts: &[VInst]) -> LiveSet {
    match region {
        Region::Linear { start, end } => {
            // Walk instructions backward
            let mut live = LiveSet::new();
            for i in (*start..*end).rev() {
                update_liveness(&mut live, &vinsts[i as usize]);
            }
            live
        }
        Region::IfThenElse { head, then_body, else_body } => {
            let then_live = liveness(then_body, vinsts);
            let else_live = liveness(else_body, vinsts);
            let merge_live = then_live.union(&else_live);
            let head_live = liveness(head, vinsts);
            merge_live.union(&head_live)
        }
        Region::Loop { header, body } => {
            // Fixed-point on header+body (small, local)
            let mut live = liveness(header, vinsts);
            loop {
                let body_live = liveness(body, vinsts);
                let new_live = live.union(&body_live);
                if new_live == live { break; }
                live = new_live;
            }
            live
        }
        Region::Seq(regions) => {
            // Walk backward through sequence
            let mut live = LiveSet::new();
            for r in regions.iter().rev() {
                let r_live = liveness(r, vinsts);
                live = live.union(&r_live);
            }
            live
        }
    }
}
```

Much simpler than fixed-point on arbitrary CFG!

## Final M4 Scope (Corrected)

M4 builds the allocator **shell** - infrastructure with stubbed decisions:

1. **Region tree building** (`lower.rs`) - Build Region enum during lowering
2. **Region display** (`debug/region.rs`) - Human-readable tree format
3. **Liveness analysis** (`alloc/liveness.rs`) - Recursive liveness on region tree
4. **Trace system** (`alloc/trace.rs`) - AllocTrace structure for recording decisions
5. **Walk shell** (`alloc/walk.rs`) - Backward walk structure with stubbed decisions
6. **CLI integration** (`args.rs`, `handler.rs`) - `--show-region`, `--show-liveness` flags
7. **Tests** - Unit tests for region building and liveness

**NOT in M4:** Real allocation decisions (LRU, spill, reload). Those come in M5.

M4 deliverable: Can build region tree, show liveness, walk backward with stubs, record to trace.

## Notes

- The current `alloc.rs` is ~560 lines with tests
- The new structure should be modular (region display, liveness.rs, trace.rs, etc.)
- Textual debug output is critical - every IR stage must be printable
- The trace should be reversible (allocator walks backward, trace shown forward)
- Region tree coalescing: merge consecutive Linear regions to keep tree compact
