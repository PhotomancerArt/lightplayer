# M2: Functional Emitter - Design

## Scope of Work

Implement the functional emitter that converts PInst to machine code bytes.

**Status: COMPLETE**

## File Structure

```
lp-shader/lpvm-native/src/isa/rv32fa/
├── emit.rs                  # COMPLETE: Functional emitter
```

## Architecture

```
PInst (physical registers)
   |
   | emit
   v
bytes (machine code)
```

## Main Components

### 1. `emit.rs`

**`PhysEmitter` struct:**
- Accumulates machine code bytes in `Vec<u8>`
- Tracks relocations for Call instructions

**`emit()` method:**
Pattern matches on PInst variants and emits corresponding RISC-V instructions:

| PInst | Encoding |
|-------|----------|
| FrameSetup | addi sp, -size; sw ra; sw fp; addi fp |
| FrameTeardown | lw ra; lw fp; addi sp; ret |
| Add/Sub/Mul/Div/Rem | R-type via encode_* |
| And/Or/Xor | R-type via encode_* |
| Sll/Srl/Sra | R-type via encode_* |
| Neg | sub rd, x0, rs |
| Not | xori rd, rs, -1 |
| Mv | addi rd, rs, 0 |
| Slt/Sltu | R-type via encode_slt* |
| Seqz/Snez/Sltz/Sgtz | Pseudoinstructions |
| Li | iconst32_sequence (lui+addi) |
| Addi | I-type via encode_addi |
| Lw/Sw | I/S-type via encode_lw/sw |
| SlotAddr | addi rd, sp, slot*4 |
| MemcpyWords | lw/sw loop |
| Call | auipc+jalr with relocation |
| Ret | jalr x0, ra, 0 |
| Branches | B-type with placeholder offset |

## Key Design Decisions

1. **Mechanical translation**: No decisions, just pattern matching
2. **Reuse encoding functions**: Uses existing `rv32/inst.rs` encoders
3. **Relocation tracking**: Call instructions record symbol name and offset for later fixup
4. **Placeholder branches**: Branch offsets are 0 (to be resolved when control flow is supported)

## Tests

All tests in `emit::tests` pass:
- `test_emit_add` - Verifies add a0, a1, a2 encoding
- `test_emit_li` - Verifies li a0, 42 encoding (addi)
- `test_emit_ret` - Verifies ret encoding (jalr)

## Validate

```bash
cargo test -p lpvm-native --lib -- rv32fa::emit
```

All tests pass.
