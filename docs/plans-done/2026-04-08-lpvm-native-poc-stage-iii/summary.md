# M3 Integration Summary: lpvm-native with Emulation Runtime

## Completed Work

This milestone completed the end-to-end pipeline for `lpvm-native`, enabling compilation, linking, and execution of RV32 machine code via the `lp-riscv-emu` emulator.

### Architecture

The `lpvm-native` crate now provides:

- **Core emission** (no_std): Lowering, register allocation, and ELF object emission
- **rt_emu module** (feature `emu`): `NativeEmuEngine`, `NativeEmuModule`, `NativeEmuInstance`

```
GLSL / LPIR
    │
    ▼
┌─────────────────┐
│  NativeEmuEngine │ (compile + link)
│  • lower()       │
│  • regalloc()    │
│  • emit ELF      │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ NativeEmuModule  │ (linked image)
│  • ElfLoadInfo   │
│  • signatures    │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│NativeEmuInstance│ (emulate)
│  • call()       │
│  • call_q32()   │
└─────────────────┘
```

### Filetest Integration

- Added `Backend::Rv32lp` to `lps-filetests`
- New target: `rv32lp.q32` (native compiler + Q32 floats + emulator)
- Side-by-side comparison with Cranelift-based `rv32.q32`

### Smoke Tests

Two tests verify the pipeline:

1. `rv32lp_native_emulator_compiles_and_runs_iadd` - Tests `call()` with `LpsValueF32`
2. `rv32lp_native_emulator_call_q32_flat` - Tests `call_q32()` with flat `i32` args

Both tests construct simple LPIR (integer add) without imports, compile through the native backend, link with builtins, and execute via emulator.

### Validation

All checks pass:

- ✅ `cargo check -p lpvm-native --target riscv32imac-unknown-none-elf`
- ✅ `cargo test -p lpvm-native --features emu --lib` (23 tests)
- ✅ `cargo test -p lps-filetests --lib` (74 tests)
- ✅ `cargo test -p lps-filetests --test rv32lp_smoke` (2 tests)
- ✅ `cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server`
- ✅ `cargo +nightly fmt -p lps-filetests -- --check` (clean)

### Key Decisions

1. **Separate runtime modules**: `rt_emu/` for emulation, future `rt_jit/` for JIT
2. **Feature `emu`**: Gates the entire emulation runtime (std dependencies)
3. **Base crate remains no_std**: Core lowering and emission work on embedded

### Next Steps (M4+)

- JIT buffer output for direct execution
- Broader filetest matrix (control flow, builtins)
- Spilling beyond greedy limit
- F32 float mode support
