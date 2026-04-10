# M4: Allocator Shell - Summary

## Scope

Built the allocator infrastructure: CFG construction, liveness analysis, trace system, and backward walk shell with stubbed decisions.

## Deliverables

### New Modules

| File | Purpose |
|------|---------|
| `alloc/cfg.rs` | CFG construction and display format |
| `alloc/liveness.rs` | Liveness analysis and display format |
| `alloc/trace.rs` | AllocTrace system for recording decisions |
| `alloc/walk.rs` | Backward walk shell with stubbed decisions |
| `alloc/mod.rs` | FastAlloc public API |

### CLI Integration

- `--show-cfg` flag displays CFG
- `--show-liveness` flag displays liveness analysis

### Key Features

1. **CFG** - Single-block CFG for straight-line code, ready for multi-block extension
2. **Liveness** - live_in/live_out computation per block
3. **Trace** - Records stubbed decisions, reversible to forward order
4. **Walk Shell** - Backward walk structure, logs what it would do
5. **Text Format** - All structures have human-readable display format

## Architecture

```
VInst[] → CFG → Liveness → Walk (backward, stubbed) → Trace
                                            ↓
                                        (M5: real allocation)
```

## Tests

- Unit tests for each module (cfg, liveness, trace, walk)
- Integration tests verifying components work together
- All 17+ rv32fa tests pass

## M4 vs M5 Boundary

**M4 (this work):** Infrastructure shell - CFG, liveness, trace structure, stubbed walk

**M5 (next):** Real allocation - LRU eviction, spill/reload, call clobber handling

M4 provides the structure so M5 can focus purely on the allocation algorithm.
