# M4: Allocator Shell - Notes

## Scope of Work

Build the allocator structure: CFG construction, liveness analysis, trace system, and backward walk shell. The allocator produces PInsts with stubbed-out decisions (no real allocation yet).

## Current State

The allocator currently exists as a simple forward-walk implementation in `rv32fa/alloc.rs`:

- Single-pass forward allocation
- Last-use register freeing
- Basic parameter precoloring
- Simple register pool management

This works for straight-line code but lacks:
- CFG construction (even for single block)
- Liveness analysis
- Trace/debug system
- Backward-walk allocation

## M4 Goals

Per the roadmap (`m4-allocator-shell.md`), M4 should create:

1. **CFG module** (`alloc/cfg.rs`): Control flow graph for VInst sequences
2. **Liveness module** (`alloc/liveness.rs`): Liveness analysis per basic block
3. **Trace module** (`alloc/trace.rs`): AllocTrace system for debugging decisions
4. **Walk module** (`alloc/walk.rs`): Backward walk shell with stubbed decisions
5. **Spill module** (`alloc/spill.rs`): Spill slot tracking
6. **Main alloc module** (`alloc/mod.rs`): FastAlloc entry point

## Questions

### Q1: Should we keep the current simple allocator?

**Answer:** Replace completely. The current allocator was a placeholder. Build the new backward-walk allocator with CFG, liveness, and trace system.

### Q2: Do we need CFG for straight-line code?

**Answer:** Build full CFG support. The CFG is critical infrastructure and a major part of this milestone. It must:
- Be well tested
- Have CLI debug support (`--show-cfg` flag)
- Lay groundwork for M5 (control flow support)

Even for straight-line code, we build a single-block CFG. For future control flow, the CFG will handle multiple blocks.

### Q3: Trace detail level?

**Answer:** Maximum debug detail. The trace must show:
- All layers: LPIR index, VInst, resulting PInst(s)
- All decisions: register assignment, spill decisions, reloads, frees, evictions
- Register state: which vreg is in which preg at each step
- Visual sanity check: formatted for human readability

This enables quick debugging and demonstrates the allocator is working correctly.

### Q4: What's the M4 vs M5 boundary?

**Answer:** 
- **M4 (Shell):** Build infrastructure, walk backward, make stubbed decisions
- **M5 (Core):** Implement real allocation (LRU, spill, reload, call clobbers)

M4 is about having the structure in place so M5 can focus on the allocation algorithm.

## Final M4 Scope (Corrected)

M4 builds the allocator **shell** - infrastructure with stubbed decisions:

1. **CFG construction** (`alloc/cfg.rs`) - Build CFG from VInsts, display format
2. **Liveness analysis** (`alloc/liveness.rs`) - Compute live ranges, display format
3. **Trace system** (`alloc/trace.rs`) - AllocTrace structure for recording decisions
4. **Walk shell** (`alloc/walk.rs`) - Backward walk structure with stubbed decisions
5. **CLI integration** (`args.rs`, `handler.rs`) - `--show-cfg`, `--show-liveness` flags
6. **Tests** - Unit tests for CFG construction and liveness

**NOT in M4:** Real allocation decisions (LRU, spill, reload). Those come in M5.

M4 deliverable: Can build CFG, show liveness, walk backward with stubs, record to trace.

## Notes

- The current `alloc.rs` is ~560 lines with tests
- The new structure should be modular (cfg.rs, liveness.rs, trace.rs, etc.)
- Textual debug output is critical - every IR stage must be printable
- The trace should be reversible (allocator walks backward, trace shown forward)
