# M2: Backward-Walk Allocator — Notes

## Scope of Work

Implement the core fastalloc algorithm: a backward-walk register allocator that
processes straight-line code (single basic block, no control flow) and produces
`FastAllocation` with per-operand assignments and explicit move edits. This is
the algorithmic core that will replace the adapter-based approach from M1.

## Current State

### M1 completed

- `FastAllocation` types defined (`operand_homes`, `edits`, etc.)
- `AllocationAdapter` converts `Allocation` → `FastAllocation`
- New emitter path consumes `FastAllocation` and splices edits
- All filetests pass with both old and new paths
- `USE_FAST_ALLOC_EMIT` flag controls which path is used

### Current allocation flow

```
VInsts → Greedy/Linear allocator → Allocation → AllocationAdapter → FastAllocation → Emitter
```

### Target allocation flow (M2)

```
VInsts → FastAllocator → FastAllocation → Emitter
```

The adapter is bypassed; the allocator produces `FastAllocation` directly.

## Key Algorithm Concepts

### Backward walk

Process instructions from last to first. At each instruction:

1. **Defs (late)**: The vreg being defined dies here (going backward). Free its
   register. If the value was live across some region (spilled), insert
   `Move reg→stack` **after** the instruction.

2. **Uses (early)**: These vregs need to be in registers. If already in a reg,
   great. If on stack (was evicted earlier in the backward walk), pick a free
   reg, insert `Move stack→reg` **before** the instruction.

3. **Call clobbers**: For `Call` instructions, all caller-saved registers are
   clobbered. Evict any live vregs sitting in caller-saved regs to their spill
   slots before processing the call's uses.

4. **Fixed constraints**: Call args must be in `a0-a7`, returns in `a0-a1`.
   Move values into/out of these regs as needed.

### Data structures (from design doc)

```rust
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
    lru: [PhysReg; 15],  // ~15 allocatable regs
    lru_head: usize,
    
    /// Output edits to splice into emission.
    edits: Vec<(EditPos, Edit)>,
}
```

### Live set management

Walking backward:
- At instruction `i`, the `live` set contains all vregs that will be used by
  instructions `0..i-1` (already processed in backward walk).
- When we see a **def** for vreg `v`, we remove `v` from `live` (it dies here).
- When we see a **use** for vreg `v`, we add `v` to `live` (it becomes live).

## Questions

### Q1: How do we handle the precoloring of parameters?

**Context:** Function parameters arrive in ABI registers (`a0-a7`). The
allocator needs to know which vregs are parameters and where they start.

**Option (a):** Pass param info explicitly to the allocator (list of `(vreg,
initial_preg)` pairs).

**Option (b):** Treat params as already having `vreg_home` entries set before
the backward walk starts. The allocator just reads the existing home.

**Suggestion:** (b) is cleaner. The caller sets up initial `vreg_home` for
params based on ABI classification. The backward walk starts with params
already marked as "in their initial registers."

**Answer:** (b) Caller initializes `vreg_home` for params before the backward walk.

### Q2: When do we allocate spill slots?

### Q2: When do we allocate spill slots?

**Context:** Spill slots are assigned lazily on first eviction. But we need to
know the total `spill_slot_count` for frame layout before emission.

**Option (a):** Two-pass approach: first pass assigns spill slots and records
which vregs got which slots; second pass produces the allocation.

**Option (b):** Single pass with lazy assignment, then count at the end.
The `next_spill_slot` counter at the end is the `spill_slot_count`.

**Suggestion:** (b) is simpler. We just need to ensure spill slots are assigned
deterministically (first eviction gets slot 0, etc.).

**Answer:** (b) Single pass with lazy assignment; `next_spill_slot` at end is the count.

### Q3: How do we handle rematerialization of IConst32?

**Context:** `IConst32` values can be rematerialized at each use instead of
being spilled. In the backward walk, this means:

- The def doesn't need a register or spill slot
- Each use generates a `Move Imm→Reg` edit instead of `Move Stack→Reg`

**Option (a):** Detect `IConst32` defs during the walk and treat them specially
(no home, just generate imm→reg moves at uses).

**Option (b):** Pre-scan all `IConst32` defs before the walk and mark those
vregs as "rematerializable."

**Suggestion:** (a) is simpler and fits the backward walk structure naturally.
When we encounter `IConst32 { dst, val }`, we don't assign it a home; at uses,
we generate the immediate move.

**Answer:** (a) Detect `IConst32` during walk; no home, generate imm→reg moves at uses.

### Q4: How do we integrate with the existing config flag?

**Context:** We have `USE_LINEAR_SCAN_REGALLOC` and `USE_FAST_ALLOC_EMIT`. We
need a new flag `USE_FASTALLOC` to select the fastalloc allocator.

**Option (a):** Replace `USE_LINEAR_SCAN_REGALLOC` with `USE_FASTALLOC`. If
`USE_FASTALLOC` is true, use fastalloc; else use greedy.

**Option (b):** Keep both flags. `USE_FASTALLOC` selects the allocator;
`USE_FAST_ALLOC_EMIT` must also be true for the emitter to consume it
(meaningful combination: fastalloc + new emitter; others are transitional).

**Suggestion:** (b) for now. It lets us test fastalloc with the old emitter
(theoretically, though we'd need an adapter path) and gives flexibility. Once
fastalloc is proven, we can simplify.

**Answer:** (b) Keep both flags; `USE_FASTALLOC` selects allocator, `USE_FAST_ALLOC_EMIT`
selects emitter path. Both must be true for the fastalloc end-to-end path.

### Q5: What's the testing strategy?

**Context:** We want to validate that fastalloc produces correct code and
better (or at least not worse) allocation than greedy/linear.

**Option (a):** Compare instruction counts: fastalloc should match or beat
existing allocators on straight-line code.

**Option (b):** Just verify correctness: filetests should pass; instruction
counts may differ and that's OK for now.

**Suggestion:** (b) for initial bringup, then (a) for tuning. The key is
correctness first — we can optimize the eviction heuristics later.

**Answer:** (b) Verify correctness first (filetests pass). Instruction count
comparison to craneliff happens automatically in filetests; we'll use that for
tuning once correctness is solid.

### Q6: How do we handle functions with control flow?

### Q6: How do we handle functions with control flow?

**Context:** M2 is straight-line only. But we need to handle (or reject)
functions with branches.

**Option (a):** Detect `Label`/`Br`/`BrIf` and fall back to greedy/linear with
adapter.

**Option (b)** Detect and error — M2 only handles straight-line, M3 will add
control flow.

**Suggestion:** (a) for smoother integration. We can gradually enable fastalloc
as we expand its capabilities.

**Answer:** (b) Error on control flow for now. This makes it clear which
tests/features need M3 work based on which tests fail. Once M3 adds block
handling, those tests will start passing.

## Future Improvements

- **Param-to-callee-saved:** Once fastalloc works, measure if long-lived params
  in caller-saved regs are a bottleneck. If so, add logic to prefer callee-saved
  for long-lived values.

- **Better eviction heuristic:** LRU is simple but not optimal. Consider
  spill-cost heuristics (spill the value with lowest cost: fewest uses,
  rematerializable, etc.).

- **Live range splitting:** Currently a spilled value is reloaded before each
  use. Could keep it in a reg across multiple uses if pressure allows.
