# Stack Parameter Support - Analysis

## Scope of Work

Implement RISC-V ILP32 stack parameter support for `lpvm-native`. Currently, the backend rejects functions with more than 8 parameter slots (vmctx + 7 user params). The RV32 ABI allows additional parameters to be passed on the stack.

## Current State

### ABI Classification (Already Works)

`lp-shader/lpvm-native/src/isa/rv32/abi.rs`:

- `classify_params()` already creates `ArgLoc::Stack { offset, size }` for parameters beyond a0-a7
- `push_scalar_words()` handles the register-to-stack transition at `ARG_REGS.len()` (8)
- Stack offsets are positive (from caller's stack frame, accessed via `s0`/`fp`)

### Current Limitations

**Regalloc (greedy.rs:78-93)**:
```rust
for i in 0..slots {
    match param_locs[i] {
        ArgLoc::Reg(p) => { vreg_to_phys[i] = Some(...); }
        ArgLoc::Stack { .. } => {
            return Err(NativeError::TooManyArgs(slots));  // <-- REJECTS STACK PARAMS
        }
    }
}
```

**Emit (emit.rs:395-396, 442-443)**:
```rust
fn emit_call_direct(...) {
    if args.len() > ARG_REGS.len() {
        return Err(NativeError::TooManyArgs(args.len()));  // <-- REJECTS >8 ARGS
    }
    // ... only handles ARG_REGS[i] mapping
}
```

### Frame Layout

Current `FrameLayout` (frame.rs) has:
- `spill_base_from_sp` - spill slots for regalloc
- `lpir_slot_offsets` - LPIR semantic slots (`slotaddr`)
- `callee_save_offsets` - saved registers

**Missing**: Caller stack argument area (for outgoing calls with >8 args) and tracking for incoming stack params.

## Questions

### Q1: Do we need to support both incoming and outgoing stack parameters?

**Context**: Incoming = function receives >8 params. Outgoing = function calls another with >8 args.

**Suggested Answer**: Yes, both are needed for completeness. The `call-nested.glsl` test has `combine_transforms_nested(mat2 a, mat2 b)` which is 9 slots (vmctx + 8 scalars), requiring incoming stack support. Tests like `call-multiple.glsl` may need outgoing stack support.

### Q2: Should stack parameters use the same spill slot mechanism or a separate system?

**Research - How Cranelift/emulator do it:**
- `abi_helper.rs:198-199`: `ArgSlot::Stack(offset, ty)` - stack args have positive offsets from SP
- `function_call.rs:473`: "Stack arguments are at positive offsets from SP (above SP)" 
- The caller reserves space, callee accesses via its `s0` (frame pointer = caller's SP at entry)

**Key difference from spills:**
| Aspect | Spills | Stack Arguments |
|--------|--------|-----------------|
| Location | Callee's frame | Caller's frame |
| Offset sign | Negative from `s0` | Positive from `s0` |
| Determined by | Regalloc pressure | ABI signature |
| Lifetime | Dynamic | Fixed at compile time |

**Answer: Separate tracking is correct.**

**Incoming stack params:**
- Regalloc assigns a register to each incoming param
- Prologue emits `lw reg, s0 + ABI_offset` to load from caller's stack
- Track in `Allocation.incoming_stack_params: Vec<(VReg, i32)>`

**Outgoing stack args:**
- Add `caller_arg_stack_size: u32` to `FrameLayout` (max stack args needed for any call)
- Before call with >8 args: store arg 8+ to `sp + offset` (into caller's reservation)
- Callee reads via its `s0` (same address, now positive offset)

### Q3: How do we handle sret + stack params interaction?

**Answer**: The ABI classification already handles this via `reg_idx = if is_sret { 1 } else { 0 }`. When sret is active, the first arg goes to a1, so we have 7 regs before stack. Stack classification naturally handles this. We just ensure regalloc and emit respect the shift.

### Q4: What test coverage is needed?

**Answer**:
1. **Existing**: `call-nested.glsl` - `combine_transforms_nested(mat2, mat2)` (9 slots)
2. **New**: `lpvm/native/stack-params-simple.glsl` - 10 scalar params, sum them
3. **New**: `lpvm/native/stack-params-mixed.glsl` - float + mat3 (10 slots)
4. **Unit tests**: `emit_call_direct` with 10 args, prologue loading, regalloc assignment

### Q5: Should we optimize the common case (<=8 params)?

**Answer**: Yes. Keep current fast path for <=8 params. For >8 params, add stack setup. The arg count is known at compile time, so this is straightforward.

## Notes

- Reference: RISC-V psABI doc (local copy at `oss/riscv-elf-psabi-doc`)
- QBE `rv64/emit.c` has `slot()` function for stack addressing we can reference
- The emulator (`lp-riscv-emu`) already supports stack arguments - see `stack_args_tests.rs`
