# Milestone 5: Migrate Filetests

## Goal

Port `lps-filetests` from the `GlslExecutable` trait to LPVM traits. All
three backends (Cranelift JIT, WASM, RV32 emulator) run through the unified
LPVM interface.

## Suggested plan name

`lpvm2-m5`

## Scope

### In scope

- Replace `GlslExecutable` usage in `lps-filetests` with LPVM traits
- Update test execution to use `LpvmEngine::compile()` →
`LpvmModule::instantiate()` → `LpvmInstance::call()`
- Update the filetest runner to be generic over `LpvmEngine` or use a
dispatch mechanism for backend selection
- Migrate `LpirRv32Executable` to use `lpvm-emu`'s `EmuEngine`
- Migrate `LpirJitExecutable` to use `lpvm-cranelift`'s `CraneliftEngine`
- Migrate WASM executables to use `lpvm-wasm`'s engines
- All existing filetests must pass with the new infrastructure
- Remove old `GlslExecutable` implementations from filetest code

### Out of scope

- New filetests for shared memory features (textures, cross-shader data)
- Removing `GlslExecutable` trait definition from `lps-exec` (M7)
- Performance optimization of the filetest path

## Key Decisions

1. **Backend selection**: Filetests select backends via directive (e.g.,
  `// backend: rv32`, `// backend: jit`, `// backend: wasm`). The runner
   creates the appropriate `LpvmEngine` and runs through the trait interface.
2. **Shared engine per test suite**: One engine instance per backend per test
  run. Modules and instances are created per-test. This exercises the
   multi-module-per-engine pattern.
3. **Value marshaling unchanged**: `LpvmInstance::call()` already handles
  `LpsValue` marshaling. Filetests continue to use `LpsValue` for expected
   values.

## Deliverables

- Updated `lp-shader/lps-filetests/src/test_run/execution.rs`
- Updated/replaced backend-specific executable files
- All existing filetests passing on all three backends
- Removal of old `GlslExecutable` wrappers from filetests

## Dependencies

- Milestone 2 (WASM shared memory) — WASM backends updated
- Milestone 3 (Cranelift update) — JIT backend updated
- Milestone 4 (lpvm-emu) — emulator backend available

## Estimated scope

~400–600 lines changed. Mostly restructuring existing code to use the new
trait interface. The test infrastructure change is medium-sized but
well-scoped.