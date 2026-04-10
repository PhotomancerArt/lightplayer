# M4: Allocator Shell - Design

## Scope of Work

Build the allocator infrastructure: CFG construction, liveness analysis, trace system, and backward walk shell with stubbed decisions. Real allocation comes in M5.

## File Structure

```
lp-shader/lpvm-native/src/isa/rv32fa/
├── abi.rs              # EXISTING
├── inst.rs             # EXISTING
├── emit.rs             # EXISTING
├── debug/
│   ├── mod.rs          # EXISTING
│   └── pinst.rs        # EXISTING
├── alloc.rs            # DELETE - moved to alloc/ module
└── alloc/              # NEW: allocator shell modules
    ├── mod.rs          # FastAlloc entry, error types, public API
    ├── cfg.rs          # CFG construction and display format
    ├── liveness.rs     # Liveness analysis and display format
    ├── trace.rs        # AllocTrace system structure
    └── walk.rs         # Backward walk shell with stubbed decisions

lp-cli/src/commands/shader_rv32fa/
├── mod.rs              # EXISTING
├── args.rs             # UPDATE: add --show-cfg, --show-liveness
├── handler.rs          # UPDATE: wire up debug displays
└── pipeline.rs         # EXISTING (no changes)
```

## Conceptual Architecture

```
VInst[] (from lowering)
    ↓
┌─────────────────┐
│   CFG Builder   │  ← Single block for straight-line
│   (cfg.rs)      │    Multiple blocks when control flow added
└─────────────────┘
    ↓
┌─────────────────┐
│ Liveness        │  ← Compute live-in/live-out per block
│ (liveness.rs)   │    Per-instruction liveness for trace
└─────────────────┘
    ↓
┌─────────────────┐
│ Backward Walk   │  ← Walk instructions backward
│   Shell         │    STUB: Log what decisions would be made
│  (walk.rs)      │    STUB: Don't actually allocate yet
└─────────────────┘
    ↓
┌─────────────────┐
│   AllocTrace    │  ← Record stubbed decisions
│   (trace.rs)    │    Format for human-readable display
└─────────────────┘
    ↓
PInst[] (from existing simple allocator - still works)
```

## Main Components

### 1. `alloc/cfg.rs`

Control Flow Graph construction:
- `BlockId(usize)` - Basic block identifier
- `BasicBlock` - id, start/end indices, vinsts, preds, succs
- `CFG` - blocks, entry block
- `build_cfg(vinsts)` - Build CFG from VInst sequence
- `format_cfg(cfg)` - Human-readable CFG display

For M4: Single-block CFG for straight-line code.

### 2. `alloc/liveness.rs`

Liveness analysis:
- `Liveness` - live_in, live_out per block
- `analyze_liveness(cfg, num_vregs)` - Compute liveness
- `format_liveness(liveness)` - Human-readable display

For M4: Build the analysis structure, display format. Not used for allocation yet.

### 3. `alloc/trace.rs`

Trace system for debugging:
- `AllocTrace` - Vec of TraceEntry
- `TraceEntry` - vinst_idx, vinst, decision, register_state
- `TraceDecision` - enum of decision types (StubAssign, StubSpill, etc.)
- `format_trace(trace)` - Human-readable table format

For M4: Record stubbed decisions. Format shows what allocator would do.

### 4. `alloc/walk.rs`

Backward walk shell:
- `WalkState` - Current register assignment tracking (stubbed)
- `walk_block_backward(block, trace)` - Walk and record stubs
- Stub functions that log decisions without actually allocating

For M4: Walks backward, records "would allocate v1 to a0" type decisions.

### 5. `alloc/mod.rs`

Public API:
- `FastAlloc` struct
- `allocate(vinsts, func_abi, func)` - Main entry
- Error types
- Integration with trace, cfg, liveness modules

For M4: Orchestrates the shell components, returns trace with stubbed decisions.

### 6. CLI Integration

`args.rs` additions:
```rust
#[arg(long, action = clap::ArgAction::SetTrue)]
pub show_cfg: bool,

#[arg(long, action = clap::ArgAction::SetTrue)]
pub show_liveness: bool,
```

`handler.rs` updates:
- Call CFG builder and display if `--show-cfg`
- Call liveness analysis and display if `--show-liveness`
- Display trace after allocation

## Key Design Decisions

1. **Stubbed decisions in M4**: The walk records what it would do, doesn't actually do it
2. **Single-block CFG**: Even straight-line code has a CFG block for consistency
3. **Trace is primary output**: M4's value is showing what the allocator would decide
4. **Existing allocator still works**: The simple allocator in `alloc.rs` is replaced by the shell, but still produces valid PInsts

## Success Criteria

1. `cargo test -p lpvm-native --lib -- rv32fa::alloc` passes
2. `./target/debug/lp-cli shader-rv32fa file.glsl --show-cfg` displays CFG
3. `./target/debug/lp-cli shader-rv32fa file.glsl --show-liveness` displays liveness
4. Trace shows stubbed decisions for each VInst
5. Existing filetests still pass (using simple allocator)

## Phases

1. Create alloc/ module structure
2. Implement CFG construction and display
3. Implement liveness analysis and display
4. Implement trace system structure
5. Implement backward walk shell with stubs
6. Wire up CLI --show-cfg and --show-liveness
7. Tests and validation
8. Cleanup
