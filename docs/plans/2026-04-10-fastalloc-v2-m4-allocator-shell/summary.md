# M4: Allocator Shell - Summary

## Scope

Built the allocator infrastructure: **region tree CFG** built during lowering, liveness analysis for structured control flow, trace system, and backward walk shell with stubbed decisions.

## Deliverables

### New Modules

| File | Purpose |
|------|---------|
| `lower.rs` (extended) | Region tree building during lowering |
| `debug/region.rs` | Region tree display format |
| `alloc/liveness.rs` | Recursive liveness for region tree |
| `alloc/trace.rs` | AllocTrace system for recording decisions |
| `alloc/walk.rs` | Backward walk shell with stubbed decisions |
| `alloc/mod.rs` | FastAlloc public API |

### CLI Integration

- `--show-region` flag displays the region tree structure
- `--show-liveness` flag displays liveness analysis

### Key Features

1. **Region Tree** - Structured CFG preserving LPIR control flow (Linear, IfThenElse, Loop)
2. **Liveness** - Recursive descent liveness computation (no fixed-point iteration needed)
3. **Trace** - Records stubbed decisions, reversible to forward order
4. **Walk Shell** - Backward walk structure, logs what it would do
5. **Text Format** - All structures have human-readable display format

## Architecture

```
LPIR → Lower (with regions) → VInst[] + Region tree
                                   ↓
                             Liveness (recursive)
                                   ↓
                             Walk (backward, stubbed)
                                   ↓
                                Trace
                                   ↓
                            (M5: real allocation)
```

## Region Tree Benefits

| Aspect | Region Tree | Flat CFG |
|--------|-------------|----------|
| Build cost | Free (during lowering) | Separate pass |
| VInst copies | 0 (indices only) | 1+ per block |
| Liveness | Recursive descent | Fixed-point iteration |
| Embedded memory | ~4 bytes/region | ~40 bytes/block |
| Structured code | Natural | Requires reconstruction |

## Tests

- Unit tests for region building and coalescing
- Tests for IfThenElse and Loop region detection
- Liveness tests for structured control flow
- Trace and walk tests
- All 17+ rv32fa tests pass

## M4 vs M5 Boundary

**M4 (this work):** Infrastructure shell - region tree, liveness, trace structure, stubbed walk

**M5 (next):** Real allocation - LRU eviction, spill/reload, call clobber handling

M4 provides the structure so M5 can focus purely on the allocation algorithm.
