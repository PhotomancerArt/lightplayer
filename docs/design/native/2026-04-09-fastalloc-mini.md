# Mini-Fastalloc for lpvm-native

Design notes for a backward-walk register allocator inspired by regalloc2's
fastalloc, adapted to our VInst IR. Not planned for immediate implementation —
written down so we can return to it later if the 1.18× gap on real shaders
becomes a problem.

## Why consider it

Our linear scan assigns each vreg one home (register or spill slot) for its
entire lifetime. Once spilled, a value stays spilled — every use generates
`lw tmp, [fp+off]` even when registers are plentiful at that point.

Fastalloc's key property: a vreg's home can **change between instructions**.
A value can be evicted to stack when pressure peaks, then reloaded into a
(possibly different) register when pressure drops. This "live range splitting
for free" is the main source of its advantage.

Current numbers (2026-04-09, linear scan + remat):

- Perf tests: **1.88×** (artificial pressure, many long-lived values)
- Rainbow shader: **1.18×** (real workload, shorter live ranges)

The 1.18× is acceptable. The 1.88× tells us where the ceiling is.

## Algorithm sketch

### Core idea

Walk each basic block **in reverse** (last instruction first). At each
instruction, the allocator knows which vregs are **currently live** (they'll be
used by some later instruction we already processed). It assigns physical
registers to operands on demand, evicting the least-recently-used vreg to a
spill slot when registers run out, and inserting explicit move instructions
(reg→stack, stack→reg) into the output stream.

### Per-instruction steps (reverse order)

For instruction `I` with operands `[def d, use a, use b]`:

1. **Process defs** (late): `d` is being defined here, so it dies looking
   backward. Free its register. Record the allocation for `d`'s operand slot.
   If `d` had a spill slot (it was live across some earlier region), insert
   `mov reg→stack` **after** `I` so later uses see it in the slot.

2. **Process uses** (early): `a` and `b` need to be in registers (or stack,
   for `Any` constraints). If the vreg already has a register, great. If it's
   on the stack (was evicted earlier in the backward walk), pick a free reg,
   insert `mov stack→reg` **before** `I`, and update the vreg's home.

3. **Handle clobbers**: For `Call` instructions, every caller-saved register
   is clobbered. Before processing the call's uses, evict any vreg sitting in
   a caller-saved reg to its spill slot. This replaces the current
   `emit_call_preserves_before/after` mechanism.

4. **Handle fixed regs**: Call arguments must be in `a0–a7`, returns in
   `a0–a1`. These are fixed-register constraints — the allocator must move
   values into/out of those regs and mark them unavailable.

### Block boundaries

GLSL has **structured control flow** (if/else, for/while — no arbitrary
gotos). The lowerer already emits `Label`, `Br`, `BrIf` with label IDs, and
tracks `LoopRegion` boundaries. This means:

- **No CFG needed.** We don't need a proper basic-block graph with
  predecessor/successor edges, phi nodes, or parallel moves at block
  boundaries.

- **Structured regions** give us a simpler contract: at every merge point
  (after an if/else, at a loop header), all live vregs must be in their
  **spill slots** (the canonical "safe" location). The allocator ensures this
  by spilling everything live at region boundaries.

Concretely, the block structure we'd need:

```
struct Block {
    start: usize,  // index into vinsts[]
    end: usize,    // exclusive
    kind: BlockKind,
}

enum BlockKind {
    Straight,
    LoopHeader,
    LoopExit,
    IfThen,
    IfElse,
    Merge,
}
```

This is derivable from the existing `Label` / `Br` / `BrIf` / `LoopRegion`
info without building a full CFG. The lowerer's `LoopRegion { header_idx,
backedge_idx }` already marks loop boundaries. If/else structure is implicit
in the label topology (BrIf to else_label, Br to end_label, Label else_label,
..., Label end_label).

### Building blocks from VInsts

A block boundary occurs at:

- Every `Label` instruction (starts a new block)
- Every `Br` / `BrIf` instruction (ends the current block)

So: scan VInsts linearly, split at labels and branches. Each block is a
contiguous slice `vinsts[start..end]`. The "structured" guarantee means
blocks nest cleanly — no irreducible control flow.

### Liveness at block boundaries

At the **start of each block** (after processing it backward), any vreg still
marked live must be in its spill slot. Insert `mov slot→reg` edits at the
block entry to reconcile.

At the **end of each block** (before processing it backward), seed the live
set from the successor block's expectations. For structured control flow:

- **Straight / fall-through**: successor's live-at-entry = our live-at-exit.
- **Loop header**: live-at-entry includes loop-carried values (already
  handled by `extend_for_loops` in current linear scan — same info reusable).
- **If/else merge**: live set is the union of both branches' live-at-exit.

Because we have structured control flow, we can process blocks in **reverse
post-order** (which is just reverse of emission order for structured code)
and propagate liveness backward.

### Where moves go

The allocator doesn't modify VInsts in place. Instead it produces a list of
**edits**: `(position, Edit)` where position is `Before(inst_idx)` or
`After(inst_idx)`, and Edit is `Move { from, to }`. The emitter interleaves
these moves with the original VInsts.

This is the same architecture as regalloc2's `Output.edits`. For us, edits
would be additional `VInst::Mov32` or `lw`/`sw` pairs spliced in during
emission.

### Data structures

```
struct MiniAllocState {
    /// Current home of each vreg: Some(preg), or None (on stack).
    vreg_home: Vec<Option<PhysReg>>,

    /// Inverse: which vreg is in each preg (VReg::MAX = empty).
    preg_occupant: [VReg; 32],

    /// Set of currently live vregs (built during backward walk).
    live: BTreeSet<VReg>,

    /// Spill slot assignments (lazy: allocated on first eviction).
    vreg_spill_slot: Vec<Option<u32>>,
    next_spill_slot: u32,

    /// LRU for eviction: least-recently-used preg is best victim.
    /// Could be a simple circular buffer over the ~15 allocatable regs.
    lru: [PhysReg; 15],
    lru_head: usize,

    /// Output edits to splice into emission.
    edits: Vec<(EditPos, VReg, PhysReg, i32)>,
}
```

### Interaction with existing code

**What stays the same:**

- `lower.rs` (LPIR → VInst lowering) — unchanged
- `vinst.rs` (VInst enum) — unchanged
- `abi.rs` (ABI classification, frame layout) — unchanged
- Emission (`emit.rs`) — mostly unchanged, but `use_vreg` / `def_vreg` would
  read from per-operand allocations instead of a global vreg→phys map

**What changes:**

- `regalloc/` — new `mini_fastalloc.rs` alongside existing `linear_scan.rs`
  and `greedy.rs`
- `Allocation` struct — instead of one `vreg_to_phys` map, we'd produce
  per-instruction operand allocations (or equivalently, the edit list)
- `emit.rs` — the emitter splices in move edits at the right positions
- Call handling — clobber saves/restores move from emission-time heuristics
  into the allocator itself (cleaner separation)

### Estimated effort

| Component                            | Size           | Notes                                                    |
| ------------------------------------ | -------------- | -------------------------------------------------------- |
| Block splitting                      | ~50 lines      | Scan for Label/Br/BrIf                                   |
| Backward walk + alloc                | ~300 lines     | Core algorithm                                           |
| LRU / eviction                       | ~50 lines      | Simpler than regalloc2 (one class)                       |
| Edit generation                      | ~50 lines      | Before/After markers                                     |
| Edit splicing in emitter             | ~80 lines      | Modify `emit_function_bytes`                             |
| Liveness seeding at block boundaries | ~100 lines     | Reuse LoopRegion info                                    |
| Tests                                | ~150 lines     | Port existing regalloc tests                             |
| **Total**                            | **~800 lines** | Plus removing ~200 lines of call-save logic from emit.rs |

### What we'd gain

- Spilled values can be reloaded when pressure drops (no more "spilled
  forever")
- Call clobber handling moves into the allocator (cleaner, and the allocator
  can make smarter choices — e.g. only save regs that are actually used after
  the call)
- Better worst-case perf on high-pressure code (mat4 chains, many live
  values across calls)

### What we'd lose / risk

- More complex allocator (linear scan is ~700 lines and well-tested)
- Move insertion must be correct at block boundaries (structured CF helps,
  but still needs careful testing)
- Compile-time cost: backward walk + move resolution may be slightly slower
  than single-pass linear scan (unlikely to matter on ESP32 workloads)

### Decision

Not doing this now. The 1.18× on rainbow is fine, and there are bigger
priorities (stack params, rainbow path stage III, more builtins). If we
later see real shaders hitting >1.3× on production workloads, this design
is ready to implement.
