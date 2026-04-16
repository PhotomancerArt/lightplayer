# M3 Integration Plan Notes

## Scope of Work

M3 completes the end-to-end pipeline for `lpvm-native`. M2 already produces valid ELF object files; M3 adds linking, instantiation, and execution via `lp-riscv-emu`.

### In Scope

- `rt_emu` module: `NativeEmuEngine`, `NativeEmuModule`, `NativeEmuInstance`
- Link native ELF objects with builtins via `lpvm_cranelift::link_object_with_builtins`
- Full pipeline: lower → regalloc → emit → link → emulate
- `rv32lp` backend in `lps-filetests` (native compiler alongside Cranelift-based `rv32`)
- At least one arithmetic filetest passes on `rv32lp.q32`

### Out of Scope

- JIT buffer output (M4+)
- Broad control-flow / full filetest matrix (annotate or skip as needed)
- Spilling beyond current greedy limit
- F32 float mode on native (Q32 only for M3)

## Crate Structure

```
lpvm-native/
├── src/
│   ├── lib.rs           # Core emission, re-exports
│   ├── error.rs         # Shared errors (LowerError, NativeError)
│   ├── native_options.rs # NativeCompileOptions (float_mode, debug_info)
│   ├── lower.rs, regalloc/, isa/  # Shared lowering + emission
│   ├── debug_asm.rs     # Annotated assembly output (host debugging)
│   └── rt_emu/          # Emulation runtime (feature "emu")
│       ├── mod.rs       # Re-exports
│       ├── engine.rs    # NativeEmuEngine (compile + link)
│       ├── module.rs    # NativeEmuModule (linked image)
│       └── instance.rs  # NativeEmuInstance (emulate)
```

Feature `emu` gates the entire `rt_emu` module and its dependencies (`std`, Cranelift linker, builtins ELF, `lp-riscv-emu`).

## Dependencies / Patterns

- `lpvm_cranelift::link_object_with_builtins(&[u8]) -> Result<ElfLoadInfo, CompilerError>` — requires `lpvm-cranelift` with `riscv32-object`
- `lpvm_emu::EmuSharedArena` — shared memory for VMContext allocations
- `lp_riscv_emu::Riscv32Emulator` — execution engine
- `lpvm` helpers: `flat_q32_words_from_f32_args`, `q32_to_lps_value_f32`, `decode_q32_return`

## Decisions (resolved)

### Q1: Backend naming in filetests

**Answer:** Add `Backend::Rv32lp` and target `rv32lp.q32` in `ALL_TARGETS`. Side-by-side comparison for the POC.

### Q2: Architecture — one generic engine or separate runtimes?

**Answer:** Separate runtime modules. `rt_emu/` contains `NativeEmuEngine`, `NativeEmuModule`, `NativeEmuInstance` dedicated to the emulation path. Future `rt_jit/` will host JIT variants. Shared logic (lower, regalloc, emit) stays in the crate root.

### Q3: Feature naming

**Answer:** Feature `emu` (short, matches `rt_emu`). Gates the entire emulation runtime module.

### Q4: Module structure after linking

**Answer:** `NativeEmuModule` holds `Arc<ElfLoadInfo>` (linked image), `IrModule` (for debug/calls), `LpsModuleSig`, and `EmuSharedArena` reference. Object bytes retained as `_elf` for debugging only.

### Q5: Argument marshalling for Q32

**Answer:** Same as `lpvm-emu`: `flat_q32_words_from_f32_args`, vmctx in `a0`, args in `a1+`, decode with `decode_q32_return`.

### Q6: Debug info and source-to-assembly correlation

**Answer:** Handled in M2.1 (`debug_asm.rs`, `LineTable`). M3 does not add DWARF; use existing tools when debugging.

## Notes

### Filetest integration (expected touchpoints)

- `lps-filetests/src/targets/mod.rs` — `Backend::Rv32lp`, `ALL_TARGETS` entry
- `lps-filetests/src/test_run/filetest_lpvm.rs` — `CompiledShader::Native`, `FiletestInstance::Native`
- `lps-filetests/Cargo.toml` — `lpvm-native` with `emu` feature enabled

### Validation targets

```bash
# Without emu feature (no_std only)
cargo check -p lpvm-native --target riscv32imac-unknown-none-elf

# With emu feature (host link + emulate)
cargo check -p lpvm-native --features emu
cargo test -p lpvm-native --features emu
```

### Success criteria

```bash
./scripts/glsl-filetests.sh filetests/scalar/float/op-add.glsl rv32lp.q32
# Produces: PASS with numeric result matching expected
```

## Status

- ✅ Phase 1: Cargo features and linking setup (`emu` feature)
- ✅ Phase 2: Errors and module API (`NativeEmuEngine`/`Module`/`Instance`)
- ✅ Phase 3: Native instance emulator (argument marshalling, VMContext, execution)
- ✅ Phase 4: Call and Q32 interface (flat i32 args, return decoding)
- ✅ Phase 5: Filetest target `rv32lp.q32` (`Backend::Rv32lp` added)
- ✅ Phase 6: Wire up filetest integration (`CompiledShader::Native`, `FiletestInstance::Native`)
- ✅ Phase 7: Smoke filetest passes (two tests: `call()` and `call_q32()`)
- ⬜ Phase 8: Cleanup and validation (final formatting, documentation)
