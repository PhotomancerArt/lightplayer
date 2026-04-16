# M4: Allocator Shell - Summary

## Deliverables

| Component | File | Status |
|-----------|------|--------|
| RegionTree population | `lower.rs` | Phase 1 |
| Region display | `rv32/debug/region.rs` | Phase 2 |
| Liveness (RegSet) | `alloc/liveness.rs` | Phase 3 |
| AllocTrace | `alloc/trace.rs` | Phase 4 |
| Backward walk shell | `alloc/walk.rs` | Phase 5 |
| CLI flags | `shader_rv32fa/args.rs` | Phase 6 |
| Integration tests | `alloc/mod.rs` | Phase 7 |
| Cleanup | — | Phase 8 |

## CLI

- `--show-region` — display region tree structure
- `--show-liveness` — display liveness analysis

## Architecture (post-M3.2)

```
LPIR
 ↓ lower.rs (builds VInst[] + RegionTree)
VInst[] + RegionTree
 ↓ alloc/liveness.rs (RegSet, recursive descent)
 ↓ alloc/walk.rs (backward walk, stubbed decisions → AllocTrace)
 ↓ rv32/alloc.rs (existing simple allocator, still produces PInsts)
PInst[]
 ↓ emit.rs → link.rs
machine code
```

## Key Design Decisions

| Decision | Rationale |
|----------|-----------|
| Arena-based RegionTree (`RegionId = u16`) | Better for `no_std`, no Box allocations |
| RegSet bitset (32 bytes, no heap) | Liveness without heap allocation |
| Build region tree during lowering | Zero-cost — already walking structure recursively |
| Shell runs alongside existing allocator | No disruption; existing tests keep passing |

## M4 → M5 Boundary

M4 produces:
- Populated `RegionTree` with correct structure
- `AllocTrace` with stubbed decisions
- Liveness analysis for Linear/Seq regions

M5 replaces:
- Stubbed decisions with real register assignment
- Conservative IfThenElse/Loop liveness with correct handling
- `rv32/alloc.rs` simple allocator with the new backward walk allocator
