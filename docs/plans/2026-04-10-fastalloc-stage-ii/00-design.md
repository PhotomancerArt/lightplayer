# M2: Backward-Walk Allocator — Design

## Scope of Work

Implement the core fastalloc algorithm: a backward-walk register allocator that
processes straight-line code (single basic block, no control flow) and produces
`FastAllocation` with per-operand assignments and explicit move edits. This is
the algorithmic core that will replace the adapter-based approach from M1.

## File Structure

```
lp-shader/lpvm-native/src/
├── regalloc/
│   ├── mod.rs              # UPDATE: add USE_FASTALLOC config, FastAllocator export
│   ├── adapter.rs          # (existing from M1)
│   ├── greedy.rs           # (unchanged)
│   ├── linear_scan.rs      # (unchanged)
│   └── fastalloc.rs        # NEW: backward-walk allocator
├── isa/rv32/
│   └── emit.rs             # UPDATE: integrate fastalloc path
└── config.rs               # UPDATE: add USE_FASTALLOC flag
```

## Conceptual Architecture

```
VInsts → FastAllocator → FastAllocation → Emitter
            ↓
    (straight-line only; errors on control flow)
```

## Core Algorithm

### State

```rust
struct FastAllocState {
    /// Current home of each vreg: Some(preg), or None (on stack).
    vreg_home: Vec<Option<PhysReg>>,
    
    /// Inverse: which vreg is in each preg.
    preg_occupant: [Option<VReg>; 32],
    
    /// Set of currently live vregs.
    live: BTreeSet<VReg>,
    
    /// Spill slot assignments (lazy: allocated on first eviction).
    vreg_spill_slot: Vec<Option<u32>>,
    next_spill_slot: u32,
    
    /// LRU ring buffer for eviction (~15 allocatable regs).
    lru: [PhysReg; 15],
    lru_head: usize,
    
    /// Output edits.
    edits: Vec<(EditPos, Edit)>,
}
```

### Backward Walk

Process instructions from last to first. At each instruction `i`:

1. **Defs (late)**: For each def vreg `v`:
   - If `vreg_home[v]` is `Some(p)`: free the register, add `p` to LRU as most-recently-used.
   - If `vreg_spill_slot[v]` is `Some(s)`: vreg was spilled; add `After(i)` edit `Move { from: Reg(p), to: Stack(s) }`.
   - Remove `v` from `live`.

2. **Call clobbers**: If instruction is `Call`:
   - For each live vreg in a caller-saved register: evict to its spill slot (assign slot if first time), add `Before(i)` edit `Move { from: Reg(p), to: Stack(s) }`.

3. **Uses (early)**: For each use vreg `v`:
   - If `vreg_home[v]` is `Some(p)`: already in register; mark `p` as most-recently-used.
   - If `vreg_spill_slot[v]` is `Some(s)`: on stack; pick a free register (or evict LRU), add `Before(i)` edit `Move { from: Stack(s), to: Reg(p) }`, update `vreg_home[v] = Some(p)`.
   - If `v` is `IConst32`: no home; generate `Before(i)` edit `Move { from: Imm(k), to: Reg(p) }` for some free register `p`.
   - Add `v` to `live`.

4. **Fixed constraints**: For call args/rets, ensure values are in ABI registers (`a0-a7`, `a0-a1`). Insert moves as needed before/after the instruction.

### Initialization

Caller sets up initial `vreg_home` for parameters based on ABI classification:
- Register params: `vreg_home[v] = Some(abi_reg)`
- Stack params: `vreg_home[v] = None` (will be loaded in prologue)

### Output

After processing all instructions:
- `operand_homes`: flat array of `OperandHome` for each use/def
- `operand_base`: offset into `operand_homes` for each instruction
- `edits`: all `Move` edits to splice
- `spill_slot_count`: `next_spill_slot`

## Main Components

### FastAllocator (regalloc/fastalloc.rs)

```rust
pub struct FastAllocator;

impl FastAllocator {
    pub fn allocate(
        vinsts: &[VInst],
        num_vregs: usize,
        initial_homes: &[(VReg, Option<PhysReg>)],  // params
    ) -> Result<FastAllocation, NativeError> {
        let mut state = FastAllocState::new(num_vregs, initial_homes);
        
        // Check for control flow
        if has_control_flow(vinsts) {
            return Err(NativeError::ControlFlowNotSupported);
        }
        
        // Backward walk
        for (pos, inst) in vinsts.iter().enumerate().rev() {
            state.process_instruction(pos, inst)?;
        }
        
        // Build operand_homes and operand_base from state
        state.build_allocation(vinsts)
    }
}
```

### Integration (emit.rs)

Update `emit_function_bytes` to check both flags:

```rust
if crate::config::USE_FASTALLOC && crate::config::USE_FAST_ALLOC_EMIT {
    // Fastalloc path
    let fast = FastAllocator::allocate(
        vinsts,
        func.vreg_types.len().max(max_vreg + 1),
        &param_homes,  // from ABI
    )?;
    // ... emit using fast
} else if crate::config::USE_FAST_ALLOC_EMIT {
    // Adapter path (M1)
    let alloc = allocate_for_emit(...)?;
    let fast = AllocationAdapter::adapt(&alloc, vinsts, ...);
    // ... emit using fast
} else {
    // Legacy path
    // ...
}
```

## Key Design Decisions

1. **Caller initializes `vreg_home` for params** — keeps ABI logic outside allocator.

2. **Single pass with lazy spill slot assignment** — `next_spill_slot` at end is total count.

3. **Detect `IConst32` during walk** — no home, generate imm→reg moves at uses.

4. **New `USE_FASTALLOC` config flag** — both `USE_FASTALLOC` and `USE_FAST_ALLOC_EMIT` must be true for fastalloc end-to-end.

5. **Error on control flow** — makes it clear which tests need M3; gradual enablement.

6. **Test for correctness first** — instruction count comparison happens automatically; optimize heuristics later.

## Future Improvements

- **Param-to-callee-saved:** Prefer callee-saved for long-lived params.
- **Better eviction heuristic:** Spill-cost instead of pure LRU.
- **Live range splitting:** Keep spilled values in regs across multiple uses when pressure allows.
