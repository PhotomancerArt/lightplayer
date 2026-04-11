# M4: Allocator Shell - Design

## Scope of Work

Build the allocator infrastructure: **populate the region tree** during lowering, recursive liveness analysis, trace system, and backward walk shell with stubbed decisions. Real allocation comes in M5.

## Prior Work (M3.2)

The crate restructure (M3.2) already delivered:

- `src/region.rs` — arena-based `RegionTree` with `Region` enum (Linear, IfThenElse, Loop, Seq)
- `src/regset.rs` — fixed-size `RegSet` bitset over VRegs (no heap)
- `LoweredFunction.region_tree` field (currently `RegionTree::default()`, not populated)
- `compile.rs` / `emit.rs` / `link.rs` — clean compilation pipeline
- Crate is `lpvm-native-fa`, ISA code is at `src/rv32/` (no `isa/` wrapper)

## File Structure

```
lp-shader/lpvm-native-fa/src/
├── region.rs           # EXISTING: extend with helper methods
├── regset.rs           # EXISTING: used by liveness
├── rv32/
│   ├── alloc.rs        # EXISTING: simple straight-line allocator (kept working)
│   └── debug/
│       ├── mod.rs      # EXISTING
│       └── region.rs   # NEW: region tree display
├── lower.rs            # EXTEND: populate RegionTree during lowering
└── alloc/              # NEW: allocator shell modules
    ├── mod.rs          # FastAlloc entry, public API
    ├── liveness.rs     # Recursive liveness for region tree (uses RegSet)
    ├── trace.rs        # AllocTrace system structure
    └── walk.rs         # Backward walk shell with stubbed decisions

lp-cli/src/commands/shader_rv32fa/
├── mod.rs              # EXISTING
├── args.rs             # UPDATE: add --show-region, --show-liveness
├── handler.rs          # UPDATE: wire up debug displays
└── pipeline.rs         # EXISTING (no changes)
```

## Conceptual Architecture

```
LPIR (structured control flow)
    ↓
┌─────────────────────────┐
│ Lowerer (lower.rs)      │  ← Build VInst[] + populate RegionTree simultaneously
│                         │    Preserves IfThenElse, Loop structure
└─────────────────────────┘
    ↓
VInst[] + RegionTree
    ↓
┌─────────────────────────┐
│ Liveness                │  ← Recursive descent on RegionTree
│ (alloc/liveness.rs)     │    Uses RegSet (no heap), no fixed-point needed
└─────────────────────────┘
    ↓
┌─────────────────────────┐
│ Backward Walk           │  ← Walk instructions backward
│   Shell                 │    STUB: Log what decisions would be made
│  (alloc/walk.rs)        │    STUB: Don't actually allocate yet
└─────────────────────────┘
    ↓
┌─────────────────────────┐
│   AllocTrace            │  ← Record stubbed decisions
│   (alloc/trace.rs)      │    Format for human-readable display
└─────────────────────────┘
    ↓
PInst[] (from existing rv32/alloc.rs - still works)
```

## Main Components

### 1. `lower.rs` (Extended)

**Populate RegionTree during lowering:**

The lowerer already processes structured control flow recursively via `lower_range`. We extend it to build region nodes in the existing `RegionTree` arena:

- `LowerCtx` gains a `region_tree: RegionTree` field
- `lower_range` returns a `RegionId` alongside emitting VInsts
- `coalesce_linears()` merges consecutive linear regions in Seq children

Benefits:
- **Zero cost**: built during existing lowering pass
- **Zero copies**: indices into VInst slice, not copied instructions
- **Arena-based**: `RegionId = u16`, children stored in `seq_children` vec

### 2. `rv32/debug/region.rs` (New)

Region tree display:
- `format_region_tree(tree, root, vinsts, indent)` — Human-readable tree format
- Shows Linear ranges, IfThenElse branches, Loop structure
- Indented display for hierarchy

### 3. `alloc/liveness.rs`

**Recursive liveness analysis using `RegSet`:**

Unlike flat CFG which needs fixed-point iteration, the region tree enables recursive descent:

```
liveness(Linear) = backward walk of instruction defs/uses
liveness(IfThenElse) = liveness(head) ∪ liveness(then) ∪ liveness(else)
liveness(Loop) = fixed-point on header+body (small, local)
```

- Uses `RegSet` (fixed-size bitset, no heap) not `BTreeSet`
- `analyze_liveness(tree, region_id, vinsts, pool)` — Recursive descent
- `format_liveness(liveness)` — Human-readable display

### 4. `alloc/trace.rs`

Trace system for debugging:
- `AllocTrace` — Vec of TraceEntry
- `TraceEntry` — vinst_idx, vinst_mnemonic, decision, register_state (all strings)
- `format()` — Human-readable table format

For M4: Record stubbed decisions. Format shows what allocator would do.

### 5. `alloc/walk.rs`

Backward walk shell:
- `walk_region_stub(tree, region_id, vinsts, pool, trace)` — Walk and record stubs
- Stub functions that log decisions without actually allocating
- Uses `VInst::mnemonic()` and `VInst::format_alloc_trace_detail()` for display

For M4: Walks backward through VInsts, records "would allocate v1 to a0" type decisions.

### 6. `alloc/mod.rs`

Public API:
- `run_shell(lowered, func_abi) -> AllocTrace` — Main entry
- Orchestrates liveness + walk, returns trace with stubbed decisions

For M4: The existing `rv32/alloc.rs` still produces PInsts. The shell runs alongside it.

### 7. CLI Integration

`args.rs` additions:
```rust
#[arg(long, action = clap::ArgAction::SetTrue)]
pub show_region: bool,

#[arg(long, action = clap::ArgAction::SetTrue)]
pub show_liveness: bool,
```

## Key Design Decisions

1. **Arena-based RegionTree over Box<Region>**: Uses `RegionId = u16` indices, `seq_children` vec — better for `no_std`/embedded
2. **RegSet over BTreeSet**: Fixed-size `[u64; 4]` bitset, zero heap allocation for liveness
3. **Build during lowering**: Free — we already walk the structure recursively
4. **Stubbed decisions in M4**: The walk records what it would do, doesn't actually do it
5. **Trace is primary output**: M4's value is showing what the allocator would decide
6. **Existing allocator still works**: `rv32/alloc.rs` continues to produce valid PInsts

## Memory Efficiency

| Aspect | Region Tree (arena) | Flat CFG (alternative) |
|--------|---------------------|------------------------|
| Build | During lowering (free) | Separate pass required |
| VInst storage | Single Vec, indexed | Copied into blocks |
| Per-region overhead | 8 bytes (Region enum) | 40+ bytes (Vec fields) |
| Liveness | RegSet bitset (32 bytes) | BTreeSet (heap) |
| Control flow | Natural structure | Requires reconstruction |

## Success Criteria

1. `cargo test -p lpvm-native-fa --lib` passes
2. `--show-region` displays region tree
3. `--show-liveness` displays liveness
4. Trace shows stubbed decisions for each VInst
5. Existing filetests still pass (using existing allocator)
6. Memory overhead: < 10% of VInst slice size for region tree

## Phases

1. Create `alloc/` module structure + populate RegionTree in lowerer
2. Region tree display (`rv32/debug/region.rs`)
3. Recursive liveness analysis (`alloc/liveness.rs`)
4. Trace system (`alloc/trace.rs`)
5. Backward walk shell (`alloc/walk.rs`)
6. CLI `--show-region` and `--show-liveness`
7. Integration tests
8. Cleanup
