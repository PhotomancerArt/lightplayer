# M2: Functional Emitter - Notes

## Scope of Work

Implement the functional emitter that converts PInst to machine code bytes. This is purely mechanical - no decisions, just pattern matching and encoding.

## Current State

**M2 IS ALREADY IMPLEMENTED.**

The emitter exists at `lp-shader/lpvm-native/src/isa/rv32fa/emit.rs` and is fully functional:

- All PInst variants are implemented
- Uses encoding functions from `rv32/inst.rs`
- Handles FrameSetup/FrameTeardown (prologue/epilogue)
- Handles all arithmetic, logical, shift, comparison operations
- Handles LoadImm (with lui+addi sequence for large values)
- Handles memory ops (lw, sw)
- Handles Call (with relocation tracking)
- Handles Ret
- Handles branches (Beq, Bne, Blt, Bge, J) with placeholder offsets

## Tests

All 3 emitter tests pass:
- `test_emit_add` - Verifies add encoding
- `test_emit_li` - Verifies load immediate (small values)
- `test_emit_ret` - Verifies return encoding

## Integration

The `emit_function_fastalloc_bytes` function in `mod.rs` integrates the full pipeline:
```
LPIR -> lower_ops -> VInsts -> peephole -> allocate -> PInsts -> emit -> bytes
```

## Questions

None - M2 is complete.

## Notes

- The branch instructions (Beq, Bne, Blt, Bge, J) emit placeholder offsets (0)
- Full branch target resolution will be implemented in a later milestone when control flow is supported
- The current allocator is forward-walk with last-use freeing, not backward-walk as originally planned
