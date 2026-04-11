# fastalloc3-m3 Planning Notes

## Scope

Extend `fa_alloc` backward-walk allocator to handle:
- `Call` VInsts (clobber, spill/reload, arg/ret placement)
- `BrIf`, `Br`, `Label` VInsts (branch emission)
- `Select32` VInst
- `IfThenElse` regions (register state save/restore/reconciliation)
- `Loop` regions (register state convergence)
- Wire `FuncAbi` into allocate (param precoloring, clobber sets)
- Real liveness for IfThenElse/Loop

Goal: all existing filetests pass under `rv32fa`.

## Current State

### What works (M1+M2)
- Backward walk for `Linear` and `Seq` regions
- LRU register pool with spill/reload
- Trace system for debugging
- PInst emission for arithmetic, comparison, mov, load/store, ret
- `rv32fa` filetest target wired in
- CLI `shader-rv32fa` pipeline works for straight-line functions

### What doesn't work
- `walk_region` returns `Err(UnsupportedControlFlow)` for IfThenElse/Loop
- `process_inst` returns `Err(UnsupportedCall)` for Call
- `process_inst` returns `Err(UnsupportedSelect)` for Select32
- `process_inst` returns `Err(UnsupportedControlFlow)` for BrIf/Br
- `_func_abi` param in `allocate()` is unused
- `liveness.rs` returns empty sets for IfThenElse/Loop
- `Rv32Emitter` branch encoding is a placeholder (`target << 12`)

### Key code locations
- `fa_alloc/walk.rs` — backward walk + process_inst + emit_vinst
- `fa_alloc/mod.rs` — allocate() entry point
- `fa_alloc/liveness.rs` — region liveness analysis
- `fa_alloc/spill.rs` — spill slot allocator
- `rv32/rv32_emit.rs` — PInst → machine code emission
- `rv32/inst.rs` — PInst enum (already has Beq/Bne/Blt/Bge/J/Call)
- `rv32/gpr.rs` — ALLOC_POOL, ARG_REGS, RET_REGS, SCRATCH
- `abi/func_abi.rs` — FuncAbi with call_clobbers(), precolors(), allocatable()

## Questions

### Q1: Branch label resolution strategy

**Context**: The current `Rv32Emitter` encodes branches with `(*target << 12) as i32`
which is a placeholder. Real branches need a label → byte-offset fixup system.

**Options**:
a. Add `PInst::Label { id: u32 }` to the PInst stream, emit nothing, record byte
   offset during emission, then patch branch instruction bytes after.
b. Two-pass emission: first pass records offsets, second pass patches.

**Suggestion**: (a) — add PInst::Label, record offsets in a HashMap during emit, apply
fixups in a post-pass. This is similar to how lpvm-native's emitter works (JalFixup,
BranchFixup).

**Answer**: (a) — PInst::Label + fixup table in Rv32Emitter.

### Q2: IfThenElse register state reconciliation

**Context**: In the backward walk, after visiting the merge point, we split into two
branches. Each branch may leave the register pool in a different state. At the head
(before the BrIf), we need a single consistent state.

**Strategy**: 
1. Save pool state at the merge point
2. Walk else_body backward → state_else
3. Restore merge-point state
4. Walk then_body backward → state_then
5. Pick one branch's state as canonical (e.g., then)
6. At the end of the other branch (else), emit Mv instructions to reconcile

**Answer**: Save/restore + then-branch canonical. For vregs that differ between branches,
spill in the non-canonical branch and let lazy reload handle it. This is simpler than
trying to emit Mv fixups for arbitrary register shuffles.

Compared to regalloc2's fastalloc which uses a "spillslot invariant" (everything goes
through spillslots at every block boundary), our structured region approach avoids
unnecessary spills when both branches agree. The spillslot invariant is our fallback
for deeply nested cases.

### Q3: Loop register state convergence

**Context**: The loop back-edge carries register state from the end of the body to the
header. The backward walk visits body then header, but the header state must match what
the back-edge provides.

**Answer**: Single-pass with fixup moves at the back-edge. Walk body, then header. If a
vreg is in a different register at the back-edge vs header, emit Mv fixups. No liveness
pre-pass needed — consistent with regalloc2's fastalloc which also does no pre-pass.

### Q4: Call clobber handling in backward walk

**Context**: When the backward walk encounters `Call`, the backward emission order is:
1. Reloads (post-call in execution) — emitted first in backward stream
2. Call PInst
3. Arg moves to ARG_REGS (pre-call in execution)
4. Spills of caller-saved live vregs (pre-call in execution) — emitted last

After reversal: spills → arg moves → call → reloads.

Caller-saved regs in ALLOC_POOL: t0(5), t1(6), t2(7), t4(29), t5(30), t6(31).
Callee-saved regs in ALLOC_POOL (survive calls): s2(18)-s11(27).

**Answer**: Use `func_abi.call_clobbers()` to get the caller-saved set. Iterate occupied
regs, spill those in the clobber set, emit call, resolve args into ARG_REGS.

### Q5: SRET calls

**Context**: Callees returning >2 scalars use sret. Caller passes sret buffer ptr in a0,
shifts args to a1+. After call, loads results from sret buffer.

**Answer**: Handle sret in M3 (this is the last implementation milestone). Need to plumb
sret buffer offset into the allocator. max_callee_sret_bytes should be per-function info
available via FuncAbi or passed alongside. Implement as phase 5 after direct calls work.

### Q6: Param precoloring

**Context**: `FuncAbi::precolors()` returns `[(vreg_idx, PReg)]` pairs — incoming params
are in specific ARG_REGS.

**Answer**: At the end of the backward walk (function entry in execution order), check
that param vregs are in their precolored registers. If not, emit Mv fixups. The simple
approach first; optimization (direct allocation to precolored regs) is future work.

## Notes

- The `VInst::Call` fields: `target: SymbolId`, `args: VRegSlice`, `rets: VRegSlice`,
  `callee_uses_sret: bool`. args/rets are slices into `vreg_pool`.
- `for_each_def` on Call iterates over rets (return values as defs).
- `for_each_use` on Call iterates over args (arguments as uses).
- `BrIf` has `cond: VReg, target: LabelId, invert: bool` — uses cond, no defs.
- `Br` has `target: LabelId` — no uses, no defs.
- `Label` has `LabelId` — no uses, no defs (already skipped in walk).
- PInst already has: `Beq`, `Bne`, `Blt`, `Bge`, `J`, `Call`, `Ret`.
- `Rv32Emitter` already handles all PInst variants mechanically.
