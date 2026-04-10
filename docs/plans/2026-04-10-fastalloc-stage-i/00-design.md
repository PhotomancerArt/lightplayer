# M1: New Allocation Output Format + Emitter Edit Splicing — Design

## Scope of Work

Define the new `FastAllocation` output type that supports per-instruction
operand assignments and explicit move edits. Adapt the greedy allocator to
produce this format via an adapter. Refactor the emitter to consume edit lists
instead of a static `vreg_to_phys` map.

This milestone proves the plumbing end-to-end. The allocation algorithm itself
does not change — we adapt the existing `Allocation` output to the new format.

## File Structure

```
lp-shader/lpvm-native/src/
├── regalloc/
│   ├── mod.rs              # UPDATE: FastAllocation, EditPos, Edit, Location types
│   ├── adapter.rs          # NEW: AllocationAdapter (Allocation -> FastAllocation)
│   ├── greedy.rs           # (unchanged - produces Allocation)
│   └── linear_scan.rs      # (unchanged - produces Allocation)
├── isa/rv32/
│   └── emit.rs             # UPDATE: emit_function_bytes_fast() new path
└── config.rs               # UPDATE: USE_FAST_ALLOC_EMIT flag
```

## Conceptual Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        Compilation Flow                         │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  LPIR ──► Lowerer ──► VInsts ──► Allocator ──► Allocation       │
│                                    (greedy/linear)              │
│                                      │                          │
│                                      ▼                          │
│                           ┌─────────────────┐                   │
│                           │ AllocationAdapter│                  │
│                           │  (M1: new)       │                  │
│                           └─────────────────┘                   │
│                                      │                          │
│                                      ▼                          │
│                           ┌─────────────────┐                   │
│                           │  FastAllocation  │                  │
│                           │  - operand_allocs│                  │
│                           │  - operand_base  │                  │
│                           │  - edits         │                  │
│                           └─────────────────┘                   │
│                                      │                          │
│                                      ▼                          │
│                           ┌─────────────────┐                   │
│                           │  Emitter (new)  │                   │
│                           │  - Preprocess   │                   │
│                           │    edits map    │                   │
│                           │  - Walk VInsts  │                   │
│                           │  - Emit edits   │                   │
│                           │    before/after │                   │
│                           └─────────────────┘                   │
│                                      │                          │
│                                      ▼                          │
│                              Machine Code                       │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

## Main Components and How They Interact

### FastAllocation (regalloc/mod.rs)

The new allocation output type that replaces the static `Allocation` map:

```rust
pub struct FastAllocation {
    /// Flat array of PhysReg assignments for all operands.
    /// Indexed by operand_base[inst_idx] + operand_offset.
    pub operand_allocs: Vec<PhysReg>,
    
    /// Base offset into operand_allocs for each instruction.
    /// operand_base[i] is the first operand for instruction i.
    pub operand_base: Vec<usize>,
    
    /// Move edits to splice between instructions.
    pub edits: Vec<(EditPos, Edit)>,
    
    /// Number of spill slots needed (for frame layout).
    pub num_spill_slots: u32,
    
    /// Incoming stack parameters (same as Allocation).
    pub incoming_stack_params: Vec<(VReg, i32)>,
}

pub enum EditPos {
    Before(usize),  // instruction index
    After(usize),
}

pub enum Edit {
    Move { from: Location, to: Location },
}

pub enum Location {
    Reg(PhysReg),
    Stack(u32),  // spill slot index
    Imm(i32),    // for rematerialization
}
```

### AllocationAdapter (regalloc/adapter.rs)

Converts the existing `Allocation` (static vreg→phys map) to `FastAllocation`:

1. **Build operand base offsets**: For each VInst, count uses + defs to build
   `operand_base` array.

2. **Fill operand assignments**: For each operand of each instruction, look up
   the vreg in `Allocation.vreg_to_phys` and store the PhysReg in
   `operand_allocs`.

3. **Generate call edits**: For each `VInst::Call` at position `pos`:
   - Compute which caller-saved regs need saving (same logic as
     `regs_saved_for_call()`)
   - Add `Before(pos)` edits: `Move { from: Reg(p), to: Stack(slot) }`
   - Add `After(pos)` edits: `Move { from: Stack(slot), to: Reg(p) }`

4. **Copy spill slot count and incoming params** from `Allocation`.

### Emitter New Path (isa/rv32/emit.rs)

The new emission function `emit_function_bytes_fast()`:

1. **Preprocess edits**: Build `before_edits: BTreeMap<usize, Vec<Edit>>` and
   `after_edits` for O(1) lookup during emission.

2. **Walk VInsts with index `i`**:
   - Emit `Before(i)` edits if any: lower `Move { from, to }` to appropriate
     instructions (`sw`, `lw`, `addi`, `iconst32_sequence`)
   - For each use operand `j`: read `preg = operand_allocs[operand_base[i] + j]`
   - Emit the instruction using those pregs
   - For each def operand `j`: read preg from operand_allocs
   - Emit `After(i)` edits if any

3. **Edit lowering**:
   - `Move { Reg(p1), Reg(p2) }` → `encode_addi(p2, p1, 0)`
   - `Move { Reg(p), Stack(s) }` → `encode_sw(p, S0, spill_offset(s))`
   - `Move { Stack(s), Reg(p) }` → `encode_lw(p, S0, spill_offset(s))`
   - `Move { Imm(k), Reg(p) }` → `iconst32_sequence(p, k)`

### Config Flag (config.rs)

Add `USE_FAST_ALLOC_EMIT: bool` to select between old and new emitter paths:
- `false` (default initially): use old path with `Allocation`
- `true`: use new path with `FastAllocation` (via adapter)

This allows incremental testing and validation.

## Key Design Decisions

1. **Flat operand array**: Simple, cache-friendly indexing. No changes to VInst.

2. **High-level edits**: The emitter lowers `Move` to machine instructions.
   Keeps allocation output ISA-agnostic.

3. **Adapter pattern**: Existing allocators unchanged. The adapter proves the
   new format works without introducing a new algorithm in M1.

4. **Dual path**: Keep old emitter path during M1 for comparison. Remove in
   later milestone once fastalloc is proven.

5. **Filetest validation**: Existing filetests are sufficient. M1's goal is
   working plumbing, not proving allocator correctness.
