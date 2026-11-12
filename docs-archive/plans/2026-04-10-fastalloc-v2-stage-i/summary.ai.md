# FastAlloc v2 Stage I - Summary

## Goal
Replace the problematic first fastalloc attempt with a clean, testable, functional pipeline.

## Key Decisions

1. **Functional Pipeline**: `VInsts -> Allocator -> PhysInsts -> Emitter -> bytes`
2. **PhysInst IR**: Physical registers, explicit frame ops, no indirection
3. **Textual Formats**: 
   - VInst: `i0 = Add32 i1, i2` (custom)
   - PhysInst: `add a0, a1, a2` (standard RISC-V asm)
4. **Straight-Line Only**: Error on branches/jumps for Stage I
5. **Debug-First**: Extensive trace support, CLI with `--show-*` flags

## Architecture

```
LPIR (lpir)
  ↓
Lowerer (rv32/lower.rs) - unchanged
  ↓
VInsts
  ↓
Peephole optimizer (rv32/peephole.rs) - unchanged
  ↓
FastAllocator (rv32fa/alloc.rs) - NEW
  - Backward walk
  - LRU eviction
  - Produces PhysInsts
  ↓
PhysInsts (rv32fa/inst.rs) - NEW
  - Physical registers (u8)
  - FrameSetup/FrameTeardown
  ↓
FastEmitter (rv32fa/emit.rs) - NEW
  - Mechanical PhysInst -> bytes
  - No operand_base/edits
  ↓
Machine code in RAM
```

## Phases

| Phase | Task | Output |
|-------|------|--------|
| 1 | ABI and directory structure | `rv32fa/mod.rs`, `rv32fa/abi.rs` |
| 2 | PhysInst enum | `rv32fa/inst.rs` |
| 3 | PhysInst text format | `rv32fa/debug/physinst.rs` |
| 4 | PhysInst-to-bytes emitter | `rv32fa/emit.rs` |
| 5 | Simple allocator | `rv32fa/alloc.rs` |
| 6 | CLI integration | `shader-rv32fa` command |
| 7 | Unit tests | Parser, allocator, emitter tests |
| 8 | Filetest validation | debug1.glsl, native-rv32-iadd.glsl |
| 9 | Cleanup | Remove old code, rename rv32fa -> rv32 |

## Success Criteria

- `debug1.glsl` compiles and produces correct output
- Unit tests pass for all new components
- Firmware builds without errors
- CLI `--show-*` flags produce readable debug output
