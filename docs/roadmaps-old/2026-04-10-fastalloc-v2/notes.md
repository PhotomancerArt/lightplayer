# FastAlloc v2 - Development Notes

## Quick Reference

### Running Tests

```bash
# Unit tests for new code
cargo test -p lpvm-native --lib -- rv32fa

# Peephole tests
cargo test -p lpvm-native --lib -- peephole

# Filetests with trace
debug=1 cargo test -p lps-filetests --test filetest -- native-rv32-iadd 2>&1 | head -50

# CLI debug
cargo run -p lp-cli -- shader-rv32fa test.glsl --show-vinst --show-physinst --trace
```

### Environment Variables

| Variable | Effect |
|----------|--------|
| `REG_ALLOC_ALGORITHM=fast` | Use fast allocator |
| `DEBUG=1` | Enable trace output in filetests |
| `RUST_LOG=debug` | Full debug logging |

### File Locations

| Component | Path |
|-----------|------|
| Peephole optimizer | `src/peephole.rs` |
| VInst parser/formatter | `src/debug/vinst.rs` |
| PhysInst definitions | `src/isa/rv32fa/inst.rs` |
| PhysInst parser/formatter | `src/isa/rv32fa/debug/physinst.rs` |
| Allocator | `src/isa/rv32fa/alloc/` |
| Emitter | `src/isa/rv32fa/emit.rs` |
| CLI command | `lp-cli/src/commands/shader_rv32fa/` |

## Architecture Decisions

### Why Textual IR?

Test-oriented architecture. Every IR stage (LPIR, VInst, PhysInst) has:
- Textual representation for debugging
- Parser for writing expect tests
- Formatter for comparing output

This lets us write:
```rust
expect_fastalloc("v0 = Add32 v1, v2", "a0 = Add32 a1, a2")
```

### Why New ISA Directory?

Clean separation. The `rv32/` pipeline stays untouched while we build `rv32fa/`. When proven, we delete `rv32/` and rename.

### Why CFG for Straight-Line?

Consistency. Even straight-line code is a single-block CFG. This makes the transition to control flow (M3) incremental.

### Why Always-Built Trace?

Debugging. The trace is always constructed (cheap) and formatted on demand. On error, you get the full context of what the allocator was doing.

### Memory Efficiency

Embedded targets have limited RAM. Key choices:

| Structure | Choice | Bytes/vreg | Rationale |
|-----------|--------|------------|-----------|
| Spill slots | `i8` with -1 sentinel | 1 | Most shaders have < 128 spills |
| Register pool | Fixed `[Option<VReg>; 32]` | 128 | Exactly 32 registers |
| LRU queue | Fixed `Vec<u8, 16>` | 16 | Only allocatable regs |
| Live set | `BTreeSet<VReg>` | varies | Sparse, typically small |

Tradeoff: Code clarity vs memory. We chose clarity first, optimize later when profiling shows it's needed.

## Known Limitations (M2)

- **No control flow**: Br, BrIf rejected with error
- **Single block**: CFG only produces one block
- **No phi nodes**: Not needed without control flow
- **No live range splitting**: Spill once, reload at each use

## Future Milestones (Not in This Roadmap)

- **M3**: Control flow (if/else, loops, block boundaries)
- **M4**: Optimizations (callee-saved preference, better eviction)
- **M5**: Float support (when VInst adds float variants)
