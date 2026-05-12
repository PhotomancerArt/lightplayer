# Stack Parameter Support - Summary

## Completed Work

Implemented RISC-V ILP32 stack parameter support for `lpvm-native`, enabling functions with more than 8 parameter slots.

## Key Changes

### Regalloc (phases 1)
- Added `incoming_stack_params: Vec<(VReg, i32)>` to `Allocation`
- Modified `GreedyAlloc::allocate_with_func_abi` to assign registers for `ArgLoc::Stack` params
- Tracks stack offsets from ABI classification for prologue load generation

### Frame Layout (phase 2)
- Added `caller_arg_stack_size: u32` to `FrameLayout`
- Computes max outgoing stack args needed across all calls in function
- Aligns to 16 bytes per RISC-V stack alignment requirements

### Emit (phases 3-4)
- `emit_incoming_stack_param_loads`: Emits `lw reg, s0 + offset` for each incoming stack param
- `emit_call_direct` / `emit_call_sret`: Store args 8+ (or 7+ for sret) to caller arg area
- Removed `TooManyArgs` errors for >8 params

### Tests (phase 5)
- Unit tests: `stack_params_get_registers`, `prologue_loads_incoming_stack_params`, `emit_call_with_stack_args`
- File tests: `function/param-many.glsl` (new), `function/param-mixed.glsl` (updated with mat4 cases)
- Fixed: `function/call-nested.glsl` now passes

## Files Changed

```
lp-shader/lpvm-native/src/
├── regalloc/mod.rs          (+ incoming_stack_params field)
├── regalloc/greedy.rs       (+ stack param assignment logic)
├── abi/frame.rs             (+ caller_arg_stack_size field)
└── isa/rv32/emit.rs         (+ incoming loads, outgoing stores)

lp-shader/lps-filetests/filetests/function/
├── param-many.glsl          (new - comprehensive many-param tests)
└── param-mixed.glsl         (updated - added mat4 with in/out/inout)
```

## Validation Results

- All 88+ unit tests pass
- `function/call-nested.glsl`: 8/8 tests pass (was 0/8 compile-fail)
- New stack param tests: pass
- `cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf`: clean

## Architecture

Stack arguments use the standard RISC-V ILP32 convention:
- Incoming: Positive offsets from `s0` (caller's frame)
- Outgoing: Caller reserves space, stores to `sp + offset`, callee reads via `s0`
- Separate from spills (different frame, different offset sign)

## References

- RISC-V psABI doc: `oss/riscv-elf-psabi-doc/riscv-elf.adoc`
- Emulator pattern: `lp-riscv-emu/src/emu/emulator/function_call.rs`
