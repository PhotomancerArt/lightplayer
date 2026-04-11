# M4: Allocator Shell - Design

## Scope of Work

Build the allocator infrastructure: **region tree CFG** built during lowering, liveness analysis for structured control flow, trace system, and backward walk shell with stubbed decisions. Real allocation comes in M5.

## File Structure

```
lp-shader/lpvm-native/src/isa/rv32fa/
├── abi.rs              # EXISTING
├── inst.rs             # EXISTING
├── emit.rs             # EXISTING
├── debug/
│   ├── mod.rs          # EXISTING
│   └── pinst.rs        # EXISTING
│   └── region.rs       # NEW: region tree display
├── lower.rs            # EXTEND: add Region tree building
├── alloc.rs            # DELETE - moved to alloc/ module
└── alloc/              # NEW: allocator shell modules
    ├── mod.rs          # FastAlloc entry, error types, public API
    ├── liveness.rs     # Recursive liveness for region tree
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
│ Lowerer (lower.rs)      │  ← Build VInst[] + Region tree simultaneously
│                         │    Preserves IfThenElse, Loop structure
└─────────────────────────┘
    ↓
VInst[] + Region tree
    ↓
┌─────────────────────────┐
│ Liveness                │  ← Recursive descent on region tree
│ (liveness.rs)           │    No fixed-point iteration needed
└─────────────────────────┘
    ↓
┌─────────────────────────┐
│ Backward Walk           │  ← Walk instructions backward
│   Shell                 │    STUB: Log what decisions would be made
│  (walk.rs)              │    STUB: Don't actually allocate yet
└─────────────────────────┘
    ↓
┌─────────────────────────┐
│   AllocTrace            │  ← Record stubbed decisions
│   (trace.rs)            │    Format for human-readable display
└─────────────────────────┘
    ↓
PInst[] (from existing simple allocator - still works)
```

## Main Components

### 1. `lower.rs` (Extended)

**Region tree building during lowering:**

The lowerer already processes structured control flow recursively. We extend it to build a region tree alongside the flat VInst slice:

- `Region` enum - Linear, IfThenElse, Loop, Seq
- `LowerCtx` builds regions as it processes IfStart/LoopStart
- `coalesce_linears()` - merge consecutive linear regions for compactness

Benefits:
- **Zero cost**: built during existing lowering pass
- **Zero copies**: indices into VInst slice, not copied instructions
- **Structured**: matches original LPIR control flow exactly

### 2. `debug/region.rs` (New)

Region tree display:
- `format_region(region, vinsts, indent)` - Human-readable tree format
- Shows Linear ranges, IfThenElse branches, Loop structure
- Indented display for hierarchy

### 3. `alloc/liveness.rs`

**Recursive liveness analysis:**

Unlike flat CFG which needs fixed-point iteration, the region tree enables recursive descent:

```
liveness(Linear) = union of instruction liveness
liveness(IfThenElse) = liveness(head) ∪ liveness(then) ∪ liveness(else)
liveness(Loop) = fixed-point on header+body (small, local)
```

- `analyze_liveness_region(region, vinsts)` - Recursive descent
- `format_liveness(liveness)` - Human-readable display

### 4. `alloc/trace.rs`

Trace system for debugging:
- `AllocTrace` - Vec of TraceEntry
- `TraceEntry` - vinst_idx, vinst, decision, register_state
- `TraceDecision` - enum of decision types (StubAssign, StubSpill, etc.)
- `format_trace(trace)` - Human-readable table format

For M4: Record stubbed decisions. Format shows what allocator would do.

### 5. `alloc/walk.rs`

Backward walk shell:
- `WalkState` - Current register assignment tracking (stubbed)
- `walk_region_backward(region, trace)` - Walk and record stubs
- Stub functions that log decisions without actually allocating

For M4: Walks backward through VInsts, records "would allocate v1 to a0" type decisions.

### 6. `alloc/mod.rs`

Public API:
- `FastAlloc` struct
- `allocate(vinsts, region, func_abi, func)` - Main entry
- Error types
- Integration with trace, liveness modules

For M4: Orchestrates the shell components, returns trace with stubbed decisions.

### 7. CLI Integration

`args.rs` additions:
```rust
#[arg(long, action = clap::ArgAction::SetTrue)]
pub show_region: bool,      // Show region tree structure

#[arg(long, action = clap::ArgAction::SetTrue)]
pub show_liveness: bool,    // Show liveness analysis
```

`handler.rs` updates:
- Display region tree if `--show-region`
- Display liveness analysis if `--show-liveness`
- Display trace after allocation

## Key Design Decisions

1. **Region tree over flat CFG**: Matches LPIR structure, enables recursive liveness, no VInst copies
2. **Build during lowering**: Free - we already walk the structure recursively
3. **Stubbed decisions in M4**: The walk records what it would do, doesn't actually do it
4. **Trace is primary output**: M4's value is showing what the allocator would decide
5. **Existing allocator still works**: The simple allocator is replaced by the shell, but still produces valid PInsts

## Memory Efficiency

| Aspect | Region Tree | Flat CFG (alternative) |
|--------|-------------|------------------------|
| Build | During lowering (free) | Separate pass required |
| VInst storage | Single Vec, indexed | Copied into blocks |
| Per-region overhead | 8-24 bytes (indices) | 40+ bytes (Vec fields) |
| Liveness algorithm | Recursive descent | Fixed-point iteration |
| Control flow | Natural structure | Requires reconstruction |

## Success Criteria

1. `cargo test -p lpvm-native --lib -- rv32fa::alloc` passes
2. `./target/debug/lp-cli shader-rv32fa file.glsl --show-region` displays region tree
3. `./target/debug/lp-cli shader-rv32fa file.glsl --show-liveness` displays liveness
4. Trace shows stubbed decisions for each VInst
5. Existing filetests still pass (using simple allocator)
6. Memory overhead: < 10% of VInst slice size for region tree

## Phases

1. Extend lowerer to build Region tree
2. Implement region tree display format
3. Implement recursive liveness analysis
4. Implement trace system structure
5. Implement backward walk shell with stubs
6. Wire up CLI --show-region and --show-liveness
7. Tests and validation
8. Cleanup
