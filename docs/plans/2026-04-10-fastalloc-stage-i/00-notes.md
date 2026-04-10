# M1: New Allocation Output Format + Emitter Edit Splicing — Notes

## Scope of Work

Define the new `FastAllocation` output type that supports per-instruction
operand assignments and explicit move edits. Adapt the greedy allocator to
produce this format. Refactor the emitter to consume edit lists instead of a
static `vreg_to_phys` map. No new allocation algorithm — this milestone proves
the plumbing end-to-end.

## Current State

### Allocation Output (regalloc/mod.rs)

The current `Allocation` struct:

```rust
pub struct Allocation {
    /// `vreg.0` as index -> physical register if assigned.
    pub vreg_to_phys: Vec<Option<PhysReg>>,
    pub clobbered: PregSet,
    /// VRegs assigned to spill slots (no physical register assigned).
    pub spill_slots: Vec<VReg>,
    /// Rematerialization candidates (IConst32 only vregs).
    pub rematerial_iconst: Vec<Option<i32>>,
    /// Incoming parameters passed on the stack.
    pub incoming_stack_params: Vec<(VReg, i32)>,
}
```

This is a function-wide mapping. The emitter reads from this map for every
operand, and the `spill_slots`/`rematerial_iconst` are checked to decide
whether to load from stack or rematerialize.

### Emitter (emit.rs)

The emitter has deep assumptions about the static map:

- `use_vreg()` - reads `alloc.vreg_to_phys`, checks spill, rematerializes
- `def_vreg()` - reads `alloc.vreg_to_phys`, checks spill
- `store_def_vreg()` - writes to spill slot if needed
- Call handling: `regs_saved_for_call()` builds save list from `vreg_to_phys`,
  `emit_call_preserves_before/after` emit `sw`/`lw` sequences

The emitter walks VInsts in order and for each instruction:
1. Handle call preserves (if call)
2. For each use: `use_vreg()` → load spill or use phys home
3. Emit the instruction
4. For each def: `def_vreg()` → assign temp, then `store_def_vreg()` if spilled
5. Handle call preserves after (if call)

### VInst Operand Model (vinst.rs)

Each VInst has:
- `defs()` - returns iterator of VRegs written
- `uses()` - returns iterator of VRegs read
- `is_call()` - true for Call instructions

For example:
- `Add32 { dst, src1, src2 }` - defs=[dst], uses=[src1, src2]
- `Call { args, rets, .. }` - defs=rets, uses=args
- `IConst32 { dst, .. }` - defs=[dst], uses=[]

### Config Flag (config.rs)

Currently `USE_LINEAR_SCAN_REGALLOC: bool = true` selects between linear scan
and greedy. We'll add `USE_FAST_ALLOC_EMIT: bool` to switch between old emitter
path (static map) and new emitter path (edit list) for validation.

## Questions

### Q1: How do we represent per-instruction operand assignments?

**Context:** The current emitter calls `use_vreg(alloc, v)` and `def_vreg(alloc, v)`
which look up the vreg in the global `vreg_to_phys` map. With fastalloc, we
need per-instruction operand assignments.

Option (a): Flat array indexed by `(inst_idx, operand_idx)` where operand_idx
is sequential (all uses first, then all defs). For each instruction we'd have
the phys reg assignments for each operand.

Option (b): Parallel structure to VInsts - each VInst carries its operand
assignments directly. This changes the VInst enum which is used elsewhere.

Option (c): Keep a separate `operand_allocs: Vec<OperandAlloc>` where
`OperandAlloc { inst_idx: usize, operand: Operand, preg: PhysReg }` and
`Operand` distinguishes use vs def and which one.

**Suggestion:** (a) flat array with indexing scheme. Simple, efficient, doesn't
change VInst. For instruction `i`, we can compute the base offset from a
parallel `inst_operand_offsets` array.

**Answer:** (a) Flat array with indexing scheme. VInst remains unchanged.
Compute `operand_base[i]` for each instruction, then `operand_allocs[base + offset]`
gives the PhysReg for that operand.

### Q2: What exactly goes in the edit list?

**Context:** The edit list needs to represent moves to splice between
instructions. These moves can be:
- Register → Register (`addi` for copy)
- Register → Stack (`sw` for spill)
- Stack → Register (`lw` for reload)
- Immediate → Register (`iconst32_sequence` for rematerialization)

Option (a): High-level edits that the emitter lowers:
```rust
enum Edit {
    Move { from: Location, to: Location },
}
enum Location {
    Reg(PhysReg),
    Stack(u32),  // spill slot index
    Imm(i32),    // for rematerialization
}
```

Option (b): Raw machine instructions ready to emit:
```rust
enum Edit {
    Sw { reg: PhysReg, slot: u32 },
    Lw { reg: PhysReg, slot: u32 },
    Addi { dst: PhysReg, src: PhysReg },
    IConst32 { dst: PhysReg, val: i32 },
}
```

**Suggestion:** (a) high-level edits. The emitter already knows how to emit
`sw`/`lw`/`addi`/`iconst32_sequence`. Keeping it high-level is cleaner and
matches the regalloc2 model. The emitter will lower `Move { from, to }` based
on the location types.

**Answer:** (a) High-level edits with `Move { from: Location, to: Location }`.
The emitter lowers these to appropriate instructions based on location type.

### Q3: How do we adapt greedy/linear scan to produce FastAllocation?

**Context:** We need an adapter that takes the existing `Allocation` (static
map) and produces a `FastAllocation` (per-instruction assignments + edits).

The key is generating the edit list. The old `regs_saved_for_call()` logic
generates the save/restore sequences. We can replicate that logic to produce
the edit list:

For each call instruction at position `pos`:
- `Before(pos)`: generate `Move { from: Reg(p), to: Stack(slot) }` for each
  caller-saved reg that needs saving
- `After(pos)`: generate `Move { from: Stack(slot), to: Reg(p) }` for restore

For non-call instructions, the edit list is empty (operand assignments come
from the static map).

**Suggestion:** Create `regalloc/adapter.rs` with `AllocationAdapter` that:
1. Walks VInsts in order
2. For each instruction, looks up operand assignments from `Allocation`
3. For calls, generates the same save/restore edits as the old emitter
4. Produces `FastAllocation` with per-instruction operand assignments

This proves the new format works with identical output.

**Answer:** Create `AllocationAdapter` that converts `Allocation` to `FastAllocation`.
The adapter replicates the old `regs_saved_for_call()` logic to produce edit list
entries for call save/restore sequences.

### Q4: How do we handle the emitter refactor?

**Context:** The current emitter uses `use_vreg()`, `def_vreg()`,
`store_def_vreg()` which all depend on the global `vreg_to_phys` map.

We need a new code path that:
1. Takes `FastAllocation` instead of `Allocation`
2. Reads operand phys regs from `FastAllocation.operand_allocs` instead of
   calling `use_vreg()`/`def_vreg()`
3. Interleaves edit list moves with VInst emission
4. For non-spilled operands, uses the assigned phys reg directly
5. For spilled operands, needs to handle load/store (but this should be rare
   with the adapter since we pre-assign everything)

Actually, with the adapter approach (M1), every vreg has a phys reg assignment
from the original allocator. Spills only happen when the original allocator
spilled. So we can still use spill logic, but it's driven by the operand
assignments rather than the global map.

**Suggestion:** Keep both emitter paths:
- Old path: `emit_function_bytes_old()` uses `Allocation`, `use_vreg()`, etc.
- New path: `emit_function_bytes_fast()` uses `FastAllocation`, reads operand
  assignments, splices edits

This lets us validate bit-identical output between old and new paths.

**Answer:** Keep both paths. New path preprocesses edits into a map, walks VInsts
with index, emits Before/After edits, reads operand pregs from flat array.
Config flag selects between old and new paths.

### Q5: Testing strategy for M1?

**Context:** M1 doesn't change the allocation algorithm, just the output format
and emitter consumption. Output should be bit-identical.

Option (a): Add a config flag to run both emitter paths and compare output
for every function during compilation. Assert identical code bytes.

Option (b): Manual comparison - run filetests with old path, capture output,
run with new path, diff.

Option (c): Only validate that new path produces correct output (not
bit-identical). The new path might have slightly different instruction
ordering for call saves due to how we generate edits.

**Suggestion:** (a) for M1 specifically - we want bit-identical output to prove
the plumbing works. Add a debug flag `FAST_ALLOC_EMIT_VALIDATE` that runs
both paths and asserts identical output. Run this during development and
initial testing.

**Answer:** Filetests are sufficient. The existing allocators aren't battle-tested
either (linear scan is partially broken). M1's goal is working plumbing, not
proving the old allocator correct. If filetests pass with the new path, the
format and emitter integration work.
