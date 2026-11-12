# FastAlloc v3 Roadmap

## Motivation

The FastAlloc v2 roadmap (M0-M4) built the foundation: the `lpvm-native`
crate with VInst IR, LPIR lowering, region tree construction, peephole
optimizer, PInst model, RV32 emitter, debug/trace infrastructure, CLI tooling
(`shader-rv32fa`), and a backward-walk shell over structured control flow. A
working straight-line allocator (`rv32::alloc`) validates the pipeline
end-to-end.

The remaining gap is the actual allocator: the `fa_alloc` backward walk
currently makes no real decisions — every entry is stubbed. It needs to assign
physical registers, manage spills/reloads, handle control flow merges, and deal
with call clobbers. Once that works and passes filetests (validated against the
cranelift pipeline in `lpvm-native`), the old cranelift-based crate can be
removed entirely.

## Architecture

The allocator replaces `rv32::alloc::allocate` in the compile pipeline:

```
LPIR
 │  lower.rs
 ▼
VInst[] + RegionTree
 │  peephole.rs
 ▼
VInst[] (optimized)
 │  fa_alloc/           ← THIS IS WHAT WE'RE BUILDING
 │  ├── liveness.rs     region-tree liveness (RegSet bitsets)
 │  ├── walk.rs         backward walk → PInst output
 │  ├── spill.rs        spill slot management
 │  └── trace.rs        allocation decision trace
 ▼
PInst[]
 │  rv32_emit.rs
 ▼
machine code bytes
```

Key design points:

- **Region-tree walk**, not CFG. Structured control flow avoids phi nodes.
- **Backward greedy allocation** with LRU eviction.
- **Output is `Vec<PInst>`** — reuses existing emitter infrastructure.
- **Liveness via `RegSet` bitsets** — heap-free, fixed-size, embedded-suitable.
- **Always-built trace** — cheap to construct, formatted on demand, attached to
  errors.

## Alternatives Considered

**Evolve `rv32::alloc` instead of replacing.** Rejected because the forward-pass
flat-list model cannot handle control flow without becoming a CFG-based
allocator, which is what `fa_alloc` already provides (but structured).

**CFG with basic blocks + phi nodes.** Rejected. The region tree approach avoids
phi insertion/resolution and maps naturally to GLSL's structured control flow.

## Risks

- **Spill code correctness.** Backward allocation makes spill/reload placement
  non-obvious. The trace system is specifically designed to make this debuggable.

- **IfThenElse/Loop merge points.** Liveness must correctly propagate across
  branches. Getting this wrong produces subtle miscompiles. Cranelift filetests
  are the safety net.

- **Call clobbers.** Caller-save/restore around calls interacts with the region
  tree walk. May need special handling for calls inside control flow.

## Milestones

| M   | Name              | Scope                                                                |
|-----|-------------------|----------------------------------------------------------------------|
| M1  | Allocator Core    | Real backward-walk allocation with PInst output, unit tests          |
| M2  | Integration       | Wire fa_alloc into compile, rv32fa filetest target, validate simple  |
| M3  | Control Flow      | IfThenElse, Loop liveness, call clobbers — full functionality        |
| M4  | Cleanup           | Remove lpvm-native crate, rename lpvm-native to lpvm-native      |

## Success Criteria

1. All cranelift filetests pass under the `rv32fa` target
2. `rv32fa` target produces correct results for control flow and calls
3. The `lpvm-native` (cranelift) crate is fully removed
4. `lpvm-native` is renamed to `lpvm-native` and is the sole native backend
5. Trace output is useful for debugging allocation decisions
6. No regressions in firmware build (`fw-esp32`, `fw-emu`)
