# FastAlloc3 M1 — Allocator Core — Notes

## Scope

Replace the stubbed `fa_alloc` backward walk with real register allocation that
produces `Vec<PInst>` for straight-line code. Unit-tested independently of the
compile pipeline.

## Current State

### `fa_alloc/` (the stub shell)

- **`walk.rs`**: `walk_region_stub(tree, region_id, vinsts, pool, trace)` — walks
  all region types (Linear, Seq, IfThenElse, Loop) but only pushes
  `TraceEntry`s with "STUB" decisions. Returns `()`.
- **`mod.rs`**: `run_shell(lowered, func_abi) -> AllocTrace` — calls
  `walk_region_stub` then `trace.reverse()`. Returns trace only, no PInsts.
- **`liveness.rs`**: `analyze_liveness(tree, id, vinsts, pool) -> Liveness` —
  real backward liveness for Linear/Seq, empty stubs for IfThenElse/Loop.
- **`trace.rs`**: `AllocTrace` / `TraceEntry` with `stub_entry()` helper.
  Format support exists.

### `rv32::alloc` (working straight-line allocator, to be replaced)

- `allocate(vinsts, func_abi, func, vreg_pool) -> Result<Vec<PInst>, AllocError>`
- Forward pass, flat `&[VInst]`
- Free-list from `gpr::ALLOC_POOL`, last-use tracking
- Params precolored via `func_abi.precolor_of(i)`
- Wraps output with FrameSetup/FrameTeardown (spill_slots: 0 — no spill support)
- Rejects: Br, BrIf, Select32, Call, SRET, stack params

### Key types

- `PInst` (rv32/inst.rs): Li, Add, Sub, Lw, Sw, Mv, etc. Uses `PReg = u8`.
- `VReg(u16)`: virtual register. `VInst::for_each_def/for_each_use` take `pool: &[VReg]`.
- `FuncAbi`: `precolor_of(vreg_idx) -> Option<PReg>`, `allocatable() -> PregSet`,
  `is_sret()`, etc. Note: abi `PReg` is `{ hw: u8, class: RegClass }`, not `u8`.
- `ALLOC_POOL`: `[5, 6, 7, 29, 30, 31, 18..27]` (t0-t2, t4-t6, s2-s11).
- `ARG_REGS`: `[10..17]` (a0-a7). `RET_REGS`: `[10, 11]` (a0-a1).

### Lowering output

`lower_ops` returns `LoweredFunction` with `vinsts`, `vreg_pool`, `region_tree`,
`symbols`, `loop_regions`. The region tree root for a straight-line function may
be a single `Linear` or a `Seq` of `Linear` regions.

## Questions

### Q1: Should M1 handle Seq regions?

**Context:** `lower_ops` can wrap multiple Linear regions in a Seq even for
straight-line code (e.g. if there's prologue + body + epilogue as separate
linear blocks). If M1 only handles Linear, we may not be able to allocate even
simple functions.

**Suggested answer:** Yes, handle Seq. It's trivial — just walk children in
order, threading register state. The walk shell already does this.

**Answer:** Yes. Handle Seq — just walk children threading register state.

### Q2: What should the `fa_alloc` public API return?

**Context:** Currently `run_shell` returns `AllocTrace`. The compile pipeline
needs `Vec<PInst>`. We also need spill slot count for FrameSetup/FrameTeardown.

**Suggested answer:** Return a struct:
```rust
pub struct AllocResult {
    pub pinsts: Vec<PInst>,
    pub trace: AllocTrace,
    pub spill_slots: u32,
}
```
FrameSetup/FrameTeardown are included in `pinsts` (allocated by fa_alloc).

**Answer:** Return `AllocResult { pinsts, trace, spill_slots }`. fa_alloc emits
FrameSetup/FrameTeardown itself.

### Q3: Precoloring strategy in backward walk?

**Context:** In a backward walk, we encounter return values first (must be in
RET_REGS) and function params last (must be in ARG_REGS). The forward allocator
precolors params at the start. In backward, we need to handle constraints at
both ends.

**Suggested answer:** At Ret, constrain return vregs to RET_REGS. At the end of
the backward walk (start of function), if any param vreg is in the wrong
register, emit Mv to fix up. Precoloring info comes from `func_abi.precolor_of`.

**Answer:** Inline constraint handling during backward walk (regalloc2 approach):
- At Ret: assign use-vregs to RET_REGS, evict conflicts, emit moves if needed.
- At function start (after walk): if param vregs aren't in ARG_REGS, emit Mv fixups.
No separate precoloring pass. Constraints derived from instruction type + FuncAbi.
