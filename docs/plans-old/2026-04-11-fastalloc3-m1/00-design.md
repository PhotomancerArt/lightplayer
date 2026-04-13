# FastAlloc3 M1 — Allocator Core — Design

## Scope

Replace the stubbed `fa_alloc` backward walk with real register allocation that
produces `Vec<PInst>` for straight-line code (Linear and Seq regions).
Unit-tested independently of the compile pipeline. IfThenElse/Loop/Call produce
errors.

## File Structure

```
lp-shader/lpvm-native-fa/src/
├── fa_alloc/
│   ├── mod.rs              # UPDATE: allocate() entry → AllocResult
│   ├── walk.rs             # UPDATE: real backward walk with PInst output
│   ├── liveness.rs         # EXISTING: confirmed working for Linear/Seq
│   ├── trace.rs            # UPDATE: real decisions instead of stubs
│   └── spill.rs            # NEW: spill slot management
├── rv32/
│   ├── alloc.rs            # EXISTING: untouched (deleted in M4)
│   ├── inst.rs             # EXISTING: PInst enum (consumed by fa_alloc)
│   ├── gpr.rs              # EXISTING: PReg, ALLOC_POOL, register names
│   └── ...
└── ...
```

## Conceptual Architecture

```
                    fa_alloc::allocate(lowered, func_abi)
                              │
                    ┌─────────▼──────────┐
                    │   WalkState        │
                    │  ┌───────────────┐ │
                    │  │ RegPool       │ │  preg↔vreg map + LRU free list
                    │  │ SpillAlloc    │ │  vreg → spill slot assignment
                    │  │ AllocTrace    │ │  per-instruction decisions
                    │  │ pinsts: Vec   │ │  PInst output (built backward, reversed)
                    │  └───────────────┘ │
                    └─────────┬──────────┘
                              │
            walk_region(tree, root)
                              │
              ┌───────────────┼───────────────┐
              ▼               ▼               ▼
         Linear           Seq            (error in M1)
         walk backward    walk children   IfThenElse/Loop
         per instruction  in reverse
              │
              ▼
     process_inst(vinst)
     ┌────────────────────────────────┐
     │ 1. Defs: free regs (backward  │
     │    = value dies here)          │
     │ 2. Uses: alloc/reload regs    │
     │    (backward = value born)     │
     │ 3. Emit PInst with resolved   │
     │    physical registers          │
     │ 4. Record trace entry          │
     └────────────────────────────────┘
```

## Main Components

### `AllocResult` (returned by `allocate`)

```rust
pub struct AllocResult {
    pub pinsts: Vec<PInst>,
    pub trace: AllocTrace,
    pub spill_slots: u32,
}
```

### `WalkState` (mutable state during backward walk)

- **`RegPool`**: fixed array `[Option<VReg>; 32]` mapping PReg→VReg. LRU
  eviction via ordered free list from `ALLOC_POOL`. Methods: `alloc(vreg)`,
  `free(preg)`, `touch(preg)`, `home(vreg) -> Option<PReg>`,
  `evict_for(preg)`.
- **`SpillAlloc`**: assigns frame-pointer-relative spill slots on demand.
  `get_or_assign(vreg) -> slot_idx`, `has_slot(vreg) -> Option<slot_idx>`,
  `total_slots() -> u32`.
- **`pinsts: Vec<PInst>`**: built in backward order, reversed at the end.
- **`trace: AllocTrace`**: records real decisions per instruction.

### `process_inst` (per-VInst logic)

For each VInst walked backward:

1. **Defs** (via `for_each_def`): the vreg is defined here. In backward walk,
   this means the value "dies" — free its PReg. If the vreg is precolored
   (param) and not in its ARG_REG, emit `Mv` fixup.
2. **Uses** (via `for_each_use`): the vreg is used here. In backward walk, this
   means the value is "born" — ensure it has a PReg. If not assigned, allocate
   from free list or evict LRU (spilling the evicted vreg).
3. **Emit PInst**: translate VInst to PInst using resolved PRegs.
4. **Record trace**: log the decision (which vregs→PRegs, any spills/reloads).

### Special cases

- **`Ret`**: constrain use-vregs to `RET_REGS`. Evict anything else in those
  regs first.
- **`IConst32`**: record value for potential rematerialization (Li instead of
  reload from spill slot).
- **`Label`**: skip (no allocation needed).
- **`Call`/`BrIf`/`Br`/`Select32`**: error in M1.

### Precoloring (regalloc2 approach)

No separate precoloring pass. Constraints handled inline:
- At `Ret`: assign uses to `RET_REGS`.
- At function start (after walk completes): if param vregs aren't in their
  `ARG_REGS`, emit `Mv` fixups at the front of the PInst stream.
- Constraint info from `FuncAbi::precolor_of(vreg_idx)`.

### Frame wrapping

`allocate` wraps the PInst output:
- `FrameSetup { spill_slots }` at the start
- `FrameTeardown { spill_slots }` at the end
- `spill_slots` from `SpillAlloc::total_slots()`

### Forward compatibility (M3)

Data structures designed so M3 can add:
- `RegPool::clobber()` to spill all caller-saved occupants around calls
- `callee_saved_used` tracking for prologue/epilogue save/restore
- Param register preference hints (allocate param vregs to ARG_REGs when free)
