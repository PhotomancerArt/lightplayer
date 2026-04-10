# Fastalloc Roadmap — Overview

## Motivation / Rationale

lpvm-native achieves 86% of cranelift's on-device FPS (25 vs 29) with 31%
smaller binary, 38% faster compile, and 39% less peak memory. The remaining
performance gap is dominated by register allocation overhead — specifically, the
combination of a static vreg-to-physical-reg map with conservative caller-saved
register preservation around every builtin call.

On high-pressure code (perf filetests), this produces 1.88x the instruction
count of cranelift. The current allocators (greedy, linear scan) both produce a
function-wide assignment where values are "spilled forever" and the emitter
blindly saves/restores all assigned caller-saved registers at every call site
regardless of liveness.

A backward-walk "fastalloc" allocator eliminates these problems by producing
per-instruction register assignments with explicit move edits. Values can be
evicted to stack when pressure peaks and reloaded into (possibly different)
registers when pressure drops. Call clobber handling becomes part of allocation
rather than a separate emitter concern, and only truly live values are saved.

## Architecture / Design

The change touches the allocator, its output format, and the emitter:

```
lp-shader/lpvm-native/src/
├── regalloc/
│   ├── mod.rs              # UPDATE: new FastAllocation output type
│   ├── greedy.rs           # KEEP: fallback, adapted to produce FastAllocation
│   ├── linear_scan.rs      # KEEP: fallback, adapted to produce FastAllocation
│   └── fastalloc.rs        # NEW: backward-walk allocator (~400 lines)
├── isa/rv32/
│   └── emit.rs             # UPDATE: emit from edit list, remove call-save machinery
└── config.rs               # UPDATE: allocator selection flag
```

### Data flow

```
VInsts (from lowerer, unchanged)
  │
  ▼
FastAllocation {
    operand_allocs: Vec<OperandAlloc>,   ── per-operand phys reg
    edits: Vec<(EditPos, Edit)>,         ── moves to splice
    num_spill_slots: u32,
}
  │
  ▼
Emitter: walk VInsts, splice edits at Before/After positions,
         read per-operand PhysReg for each use/def.
         No more regs_saved_for_call / emit_call_preserves_*.
```

### Key design decisions

- **Edit list output** (regalloc2-style): The allocator produces
  `(Before(idx) | After(idx), Edit::Move { from, to })` edits. The emitter
  interleaves these as `sw`/`lw`/`addi` instructions between VInsts.

- **Backward walk per block**: Each basic block is processed last-instruction-
  first. At each instruction, the allocator knows exactly which vregs are live
  (they'll be used by a later instruction already processed). Physical registers
  are assigned on demand; when registers run out, the LRU vreg is evicted to a
  spill slot.

- **Call clobbers in the allocator**: When the backward walk encounters a call
  instruction, all caller-saved registers are treated as clobbered. Any vreg
  occupying a caller-saved register that is live after the call is evicted to
  its spill slot. This replaces the emitter's `regs_saved_for_call` /
  `emit_call_preserves_before` / `emit_call_preserves_after` machinery.

- **Structured control flow**: GLSL has no arbitrary gotos. At block boundaries
  (if/else merge, loop header), all live vregs are reconciled to their spill
  slots. No CFG or phi nodes needed.

- **Params handled naturally**: Function parameters arrive in caller-saved regs
  (a0-a7). The backward walk evicts them to stack at the first call boundary
  and reloads into whatever register is free afterward. Future optimization:
  prefer callee-saved regs for long-lived params.

## Alternatives Considered

1. **Liveness-based call saves only** — Fix the existing TODO in
   `regs_saved_for_call` to only save registers live after the call. Would
   reduce overhead but doesn't fix spilled-forever or static allocation
   limitations. Band-aid, not a solution.

2. **Improve linear scan** — Fix the broken linear scan, add call-point
   splitting. Linear scan is already partially broken; fixing it to the level
   needed approaches the complexity of fastalloc without the architectural
   advantages (still produces a static map, still needs emitter-side call
   saves).

3. **Port regalloc2** — Use the real regalloc2 crate. Too heavy for no_std +
   alloc embedded target, and our IR (flat VInsts, structured CF) doesn't
   match its CFG-based input format.

## Risks

- **Correctness at block boundaries.** The backward walk must reconcile
  register assignments at merge points (if/else, loop headers). Structured
  control flow simplifies this (spill everything at boundaries), but it still
  needs careful testing.

- **Compile-time cost.** The backward walk + move resolution is slightly more
  work than single-pass greedy. Unlikely to matter on ESP32 workloads (shaders
  are small), but worth measuring.

- **Integration with existing emitter.** The emitter currently has deep
  assumptions about a global vreg→phys map. Refactoring to read per-operand
  allocations and splice edits is the riskiest part of M1.

## Scope Estimate

| Component                          | Size           |
| ---------------------------------- | -------------- |
| New allocation output types        | ~80 lines      |
| Adapt greedy/linear to new output  | ~100 lines     |
| Emitter refactor (edit splicing)   | ~200 lines     |
| Backward walk + core algorithm     | ~300 lines     |
| LRU / eviction                     | ~50 lines      |
| Block splitting + boundary logic   | ~150 lines     |
| Filetests                          | ~100 lines     |
| **Total new/changed**              | **~1,000 lines** |
| Remove call-save emitter code      | ~-200 lines    |
