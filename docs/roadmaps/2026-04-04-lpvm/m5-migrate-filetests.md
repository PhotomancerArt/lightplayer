# M5: Migrate Filetests

## Goal

Port `lp-glsl-filetests` from the `GlslExecutable` trait to the LPVM trait
system. All three backends (JIT, RV32, WASM) should be exercised through the
uniform LPVM API. This is the primary validation step.

## Context for Agents

### How filetests work today

`lp-glsl-filetests` is the main test infrastructure. It:

1. Reads `.glsl` test files with expected outputs in comments
2. Compiles each test for one or more backends (`Backend::Jit`, `Backend::Rv32`,
   `Backend::Wasm`)
3. Creates a `Box<dyn GlslExecutable>` for the chosen backend
4. Calls functions and compares results to expected values

`compile.rs` dispatches on `Backend`:
- `Jit` → `LpirJitExecutable` (wraps `lpir_cranelift::JitModule`)
- `Rv32` → `LpirRv32Executable` (object compile + link + emulate)
- `Wasm` → `WasmExecutable` (emit WASM + wasmtime)

### `GlslExecutable` trait methods

- `call_void`, `call_i32`, `call_f32`, `call_bool`
- `call_vec`, `call_ivec`, `call_uvec`, `call_bvec`, `call_mat`
- `call_array`
- `get_function_signature`, `list_functions`
- Debug: `format_emulator_state`, `format_clif_ir`, `format_vcode`,
  `format_disassembly` (std only, optional)

### What changes

Replace `Box<dyn GlslExecutable>` with the LPVM trait system. The exact
approach depends on the trait design from M1, but conceptually:

1. Compile LPIR → `LpvmModule` (via the chosen backend)
2. Create `LpvmInstance` from the module
3. Call functions via `LpvmInstance`
4. Compare results

Since the backends are now in separate crates (`lpvm-cranelift`, `lpvm-rv32`,
`lpvm-wasm`), filetests depends on all three.

### Key consideration: filetests API ergonomics

The current `GlslExecutable` trait has convenience methods like `call_f32`,
`call_vec`, etc. The LPVM traits may have a more generic `call(name, args) →
LpvmValue` interface.

Options for maintaining test ergonomics:
1. Build a test helper layer in filetests that wraps `LpvmInstance::call` with
   typed convenience methods.
2. Add convenience methods to `LpvmInstance` itself.
3. Use extension traits in `lpvm` for typed calls.

The goal is: filetests should be easy to write and read. Don't sacrifice test
clarity for API purity.

### Key consideration: debug output

`GlslExecutable` has optional debug methods (`format_clif_ir`, `format_vcode`,
etc.) used in test output. These are backend-specific. The LPVM trait may not
include them.

Options:
1. Backend crates expose debug formatting as separate methods (not through the
   trait).
2. The trait has an optional debug trait bound.
3. Filetests downcast or use backend-specific types for debug output.

### Backend-specific behaviors to preserve

**JIT (`lpvm-cranelift`):**
- Compilation errors are reported with source locations
- Debug output: Cranelift IR, VCode, disassembly

**RV32 (`lpvm-rv32`):**
- Emulator state can be formatted for debugging
- Instruction count / execution metrics available
- MAX_INSTRUCTIONS limit for timeout

**WASM (`lpvm-wasm`):**
- WAT disassembly via wasmprinter
- Fuel-based execution limits
- Shadow stack reset between calls

## Migration Strategy

### Phase 1: Add LPVM path alongside GlslExecutable

Keep the existing `GlslExecutable` code working. Add a parallel code path that
uses LPVM traits. This allows incremental migration and A/B comparison.

### Phase 2: Switch filetests to LPVM path

Once the LPVM path passes all tests, switch over. Remove the `GlslExecutable`
implementations.

### Phase 3: Clean up

Remove `GlslExecutable` dependency. Update imports. Simplify.

## Filetest Dependencies After Migration

```toml
[dependencies]
lpvm = { path = "../../lpvm/lpvm" }
lpvm-cranelift = { path = "../../lpvm/lpvm-cranelift" }
lpvm-rv32 = { path = "../../lpvm/lpvm-rv32" }
lpvm-wasm = { path = "../../lpvm/lpvm-wasm", features = ["runtime"] }
lpir = { path = "../lpir" }
lp-glsl-naga = { path = "../lp-glsl-naga" }
# ... test infrastructure deps
```

No longer needed:
- `lp-glsl-exec`
- `lp-glsl-abi` (use `lpvm` instead)
- Direct `lpir-cranelift` dependency (use `lpvm-cranelift` instead)
- Direct `lp-riscv-emu`, `lp-riscv-elf` dependencies (wrapped by `lpvm-rv32`)
- Direct `wasmtime` dependency (wrapped by `lpvm-wasm` runtime)

## What NOT To Do

- Do NOT break existing filetests during migration. Keep the old path working
  until the new one is validated.
- Do NOT skip any backend. All three (JIT, RV32, WASM) must work.
- Do NOT remove debug output capabilities. Find a way to preserve them.
- Do NOT change test expectations. If tests fail, the bug is in the migration,
  not the tests. (See project rule: NEVER change a test to make it pass.)

## Done When

- All existing filetests pass on all three backends through LPVM traits
- `GlslExecutable` is no longer used by filetests
- Debug output (IR, disassembly, emulator state) still available
- `lp-glsl-exec` dependency removed from filetests
- Test ergonomics are good — tests are readable and concise
