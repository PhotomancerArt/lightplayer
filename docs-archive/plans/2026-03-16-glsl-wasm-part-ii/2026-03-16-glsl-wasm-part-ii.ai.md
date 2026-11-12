# Part ii: WASM codegen foundation + filetest infrastructure

Roadmap: `docs/roadmaps/2026-03-13-glsl-wasm-playground/`

## Prerequisites

Part i is complete: `lps-frontend` extracted, `lps-compiler`
renamed to `lps-cranelift`, workspace builds clean.

## Scope

Create the `lps-wasm` crate with enough codegen to compile and
execute trivial GLSL functions (scalar arithmetic, return values).
Extend the filetest infrastructure to run the same tests against both
the Cranelift (rv32 emulator) and WASM (wasmtime) backends. End state:
at least one existing filetest passes on the WASM backend via wasmtime.

This phase intentionally does NOT cover vectors, matrices, control flow,
builtins, or the web playground. Those are parts iii and iv.

## Cleanup from Part i

The extraction was mechanical and the workspace builds, but a few items
should be tidied before building on top:

1. **Verify no Cranelift types leak through lps-frontend's public
   API.** Audit `lps-frontend/src/lib.rs` re-exports and ensure no
   `cranelift_codegen` types appear. (Confirmed clean in current code,
   but worth a grep.)

2. **Remove stale comment in lps-cranelift `exec/execution.rs`.**
   The file says `// Re-exports the shared execute functions from
   lps-compiler.` — update to say `lps-cranelift`.

3. **Verify `cargo test` passes for all crates.** The extraction commit
   was tested with `cargo build` but full `cargo test` should be run
   and any regressions fixed.

## Design

### lps-wasm crate

```
lp-shader/lps-wasm/
├── Cargo.toml
└── src/
    ├── lib.rs              # Public API: glsl_wasm(source, options) → WasmModule
    ├── options.rs           # WasmOptions (float_mode, max_errors)
    ├── module.rs            # WasmModule: holds compiled WASM bytes + metadata
    ├── types.rs             # GLSL Type → WASM ValType mapping
    └── codegen/
        ├── mod.rs           # compile_function() entry point
        ├── context.rs       # WasmCodegenContext (locals, label stack)
        ├── func_builder.rs  # WASM function body builder (wasm-encoder)
        ├── expr/
        │   ├── mod.rs       # emit_rvalue() dispatch
        │   ├── literal.rs   # int/float/bool constants
        │   ├── binary.rs    # arithmetic, comparison
        │   └── variable.rs  # local.get / local.set
        ├── stmt/
        │   ├── mod.rs       # emit_statement() dispatch
        │   ├── declaration.rs
        │   ├── return.rs
        │   └── expr.rs      # expression statements (drop result)
        └── numeric.rs       # NumericMode: Q32 (i32 ops) vs Float (f32 ops)
```

This mirrors the Cranelift codegen's structure (`codegen/expr/`,
`codegen/stmt/`) so developers familiar with one can navigate the other.

### Dependencies

```toml
[dependencies]
lps-frontend = { path = "../lps-frontend" }
lps-builtin-ids = { path = "../lps-builtin-ids" }
wasm-encoder = "0.227"      # WASM binary encoding
log = { workspace = true, default-features = false }

[dev-dependencies]
wasmtime = "29"              # Execute WASM in tests
```

No Cranelift dependency. No std requirement (the crate should be
`#![no_std]` with `extern crate alloc`). `wasmtime` is test-only.

Version numbers above are indicative — use latest at time of
implementation.

### WASM module structure

A compiled shader becomes a single WASM module:

```wasm
(module
  ;; Type section: function signatures
  (type $return_i32 (func (result i32)))
  (type $add_ints (func (param i32 i32) (result i32)))

  ;; Import section: builtins (empty for phase ii, populated in phase iii/iv)
  ;; (import "builtins" "__lp_q32_sin" (func $__lp_q32_sin (param i32) (result i32)))

  ;; Function section: user functions
  (func $add_ints (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.add
  )

  ;; Export section: all user functions + main
  (export "add_ints" (func $add_ints))
)
```

Key design decisions:

- **One WASM module per shader compilation.** Contains all user functions.
- **Functions are exported by name.** The runner looks up functions by
  name, same as `GlslExecutable::call_i32("add_ints", &[])`.
- **Builtins are WASM imports.** Declared in the import section, provided
  at instantiation time. Not needed for phase ii (no builtins yet).
- **No linear memory in phase ii.** Scalars use WASM locals only.
  Memory will be needed later for arrays and out parameters.
- **Q32 mode uses i32 WASM type.** Float literals are converted to
  Q16.16 at compile time. Arithmetic uses `i32.add`, `i32.sub`, etc.
  This matches the Cranelift Q32 strategy exactly.

### Type mapping

| GLSL Type | Q32 WASM type | Float WASM type |
|-----------|---------------|-----------------|
| int       | i32           | i32             |
| uint      | i32           | i32             |
| float     | i32 (Q16.16)  | f32             |
| bool      | i32           | i32             |
| void      | (no result)   | (no result)     |

Vectors and matrices are out of scope for phase ii. They will be
represented as multiple WASM values (multi-value returns, multiple
locals) — designed in phase iii.

### WasmCodegenContext

The context tracks state during function compilation:

```rust
pub struct WasmCodegenContext {
    /// WASM locals: maps GLSL variable name → (local_index, GlslType)
    locals: HashMap<String, LocalInfo>,
    /// Next available local index (after function params)
    next_local_idx: u32,
    /// Numeric mode (Q32 or Float)
    numeric: WasmNumericMode,
    /// Accumulated local declarations (type list for non-param locals)
    local_types: Vec<wasm_encoder::ValType>,
}
```

Unlike Cranelift, there is no SSA construction, no block sealing, no
variable declarations. WASM's local variables map directly to GLSL's
mutable variables. This is a major simplification.

### Public API

```rust
/// Compile GLSL source to a WASM module.
pub fn glsl_wasm(source: &str, options: WasmOptions) -> Result<WasmModule, GlslDiagnostics> {
    let semantic = CompilationPipeline::parse_and_analyze(source, options.max_errors)?;
    let wasm_bytes = compile_to_wasm(&semantic.typed_ast, &options)?;
    Ok(WasmModule { bytes: wasm_bytes, /* metadata */ })
}
```

```rust
pub struct WasmModule {
    /// Raw WASM binary bytes, ready for WebAssembly.instantiate() or wasmtime
    pub bytes: Vec<u8>,
    /// Exported function names and their signatures
    pub exports: Vec<WasmExport>,
}

pub struct WasmExport {
    pub name: String,
    pub params: Vec<WasmValType>,
    pub results: Vec<WasmValType>,
}
```

### Filetest infrastructure changes

The filetest runner currently hardcodes the Cranelift/rv32-emulator path.
We need to make it runtime-pluggable.

**Current flow:**

```
parse_target("riscv32.q32") → (RunMode::Emulator, DecimalFormat::Q32)
                             → glsl_emu_riscv32_with_metadata()
                             → GlslEmulatorModule (impl GlslExecutable)
                             → execute_function()
```

**New flow:**

```
parse_target("riscv32.q32") → CraneliftRunner
parse_target("wasm32.q32")  → WasmRunner

trait FiletestRunner {
    fn compile(&self, source: &str, options: RunnerOptions)
        -> Result<Box<dyn GlslExecutable>, ...>;
}
```

Changes:

1. **Add `wasm32` arch to `target.rs`.**
   `parse_target("wasm32.q32")` returns a new variant that routes to
   the WASM runner.

2. **Extract a `FiletestRunner` trait** (or just a function dispatch).
   The `run_detail.rs` code currently calls `glsl_emu_riscv32_with_metadata`
   directly. Refactor so the compilation step is dispatched by target.

3. **Implement `GlslExecutable` for WASM modules** (via wasmtime).
   This is a `WasmExecutable` struct in `lps-filetests` that:
    - Takes `WasmModule` bytes from `lps-wasm`
    - Instantiates via wasmtime
    - Implements `call_i32`, `call_f32`, etc. by calling exported WASM
      functions and converting results

4. **Add `wasmtime` as a dependency of `lps-filetests`.**

5. **Support multi-target test files.**
   Currently: `// target riscv32.q32` (one target per file).
   New: Allow `// target wasm32.q32` as an alternative. Initially, tests
   specify one target. Later, the runner may support running all targets
   by default with per-target `[expect-fail]`.

   For phase ii, we start simple: existing tests keep `riscv32.q32`,
   and we add a few new test files with `// target wasm32.q32` to
   validate the WASM path.

### Minimal GLSL subset for phase ii

The codegen must handle exactly this:

- **Types**: int, float (as Q32 i32), bool, void
- **Literals**: integer constants, float constants (→ Q16.16), bool
- **Binary ops**: `+`, `-`, `*` (for int only — Q32 mul needs builtins)
- **Comparison ops**: `==`, `!=`, `<`, `>`, `<=`, `>=`
- **Unary ops**: `-` (negate), `!` (logical not)
- **Variable declarations**: `int x = 5;`, `float y = 1.0;`
- **Return statements**: `return expr;`
- **Function parameters**: `int add(int a, int b) { return a + b; }`

This is enough to make filetests like this pass:

```glsl
// test run
// target wasm32.q32

int test_add() {
    return 1 + 2;
}
// run: test_add() == 3

int test_add_params(int a, int b) {
    return a + b;
}
// run: test_add_params(10, 20) == 30
```

## Phases (within this plan)

### Phase 1: Cleanup from part i

1. Run `cargo test` across the workspace, fix any failures.
2. Fix stale comment in `exec/execution.rs`.
3. Grep for any remaining `lps-compiler` or `lps_compiler`
   references in code, docs, configs.
4. Run `cargo +nightly fmt`.

### Phase 2: Create lps-wasm crate scaffolding

1. Create `lp-shader/lps-wasm/Cargo.toml` with dependencies.
2. Create the directory structure and stub files.
3. Implement `WasmOptions`, `WasmModule`, type mapping (`types.rs`).
4. Implement the public API entry point (`glsl_wasm()`) that parses,
   analyzes, and calls the codegen.
5. Stub the codegen to produce a valid but empty WASM module.
6. Add to workspace `Cargo.toml` members (NOT default-members).
7. Verify: `cargo build -p lps-wasm` compiles.

### Phase 3: Scalar codegen

1. Implement `WasmCodegenContext` with local variable tracking.
2. Implement function signature building (GLSL params/return → WASM
   function types).
3. Implement `emit_statement` dispatch for declarations, return, and
   expression statements.
4. Implement `emit_rvalue` for literals, variables, and binary
   operations.
5. Implement the Q32 numeric mode (float literals → Q16.16 i32 at
   compile time, int ops → i32 WASM ops).
6. Wire up the module builder: type section, function section, export
   section, code section.
7. Write unit tests in the crate: compile simple GLSL, validate the
   output WASM bytes with wasmtime.
8. Verify: `cargo test -p lps-wasm` passes.

### Phase 4: GlslExecutable for WASM (wasmtime)

1. Add `wasmtime` as a dependency of `lps-filetests`.
2. Add `lps-wasm` as a dependency of `lps-filetests`.
3. Create `test_run/wasm_runner.rs` in filetests:
    - `WasmExecutable` struct wrapping a wasmtime `Instance`
    - Implement `GlslExecutable` trait (`call_i32`, `call_f32`, etc.)
    - Handle WASM multi-value results for future vector support
4. Extend `target.rs`: `parse_target("wasm32.q32")` returns a marker
   that causes the runner to use the WASM compilation path.
5. Refactor `run_detail.rs` to dispatch compilation by target:
    - `riscv32.*` → existing `glsl_emu_riscv32_with_metadata` path
    - `wasm32.*` → `glsl_wasm()` + `WasmExecutable`
6. Verify: `cargo build -p lps-filetests` compiles.

### Phase 5: First filetests passing on WASM

1. Create a few filetest files under `filetests/wasm/` (or add
   `// target wasm32.q32` variants) that exercise:
    - Integer addition, subtraction
    - Variable declarations
    - Function parameters
    - Return values
    - Float literals (Q32 encoding)
2. Run the WASM filetests: verify they pass via wasmtime.
3. Try running existing simple filetests (those that only use scalars
   and basic arithmetic) with `// target wasm32.q32`. Note which pass
   and which need features from phase iii.
4. Verify: `cargo test -p lps-filetests` still passes for all
   existing riscv32 tests (no regressions).

### Phase 6: Final validation

1. Run `cargo build` (full workspace).
2. Run `cargo test` (full workspace).
3. Run `cargo +nightly fmt`.
4. Fix any warnings.
5. Verify `just build-fw-esp32` still works.
6. Update READMEs:
    - `lp-shader/README.md`: add lps-wasm to the crate table.
    - `lp-shader/lps-wasm/README.md`: create with purpose, usage,
      and relationship to lps-frontend and lps-cranelift.
    - `lp-shader/lps-filetests/README.md`: document the new
      `wasm32.q32` target and wasmtime runner.

## Validate

```
cargo build
cargo test
cargo build -p lps-wasm
cargo test -p lps-wasm
cargo test -p lps-filetests
cargo +nightly fmt --check
just build-fw-esp32
```

## Risk

**Medium.** The WASM codegen itself is straightforward for scalars —
WASM's stack machine model is simpler than Cranelift's SSA. The main
risk areas:

- **wasm-encoder API surface**: First time using this crate. May need
  iteration to get the module structure right (type section ordering,
  function indices, etc.).

- **wasmtime integration in filetests**: Adding a second runtime to the
  filetest infrastructure requires refactoring shared code paths. Must
  not break existing tests.

- **GlslExecutable impedance mismatch**: The trait was designed around
  Cranelift's JIT and emulator. WASM function calls work differently
  (typed exports, no raw function pointers). The `DirectCallInfo` and
  `format_*` methods won't apply to WASM — implementing them as no-ops
  is fine.

## Non-goals

- Vector/matrix support (phase iii)
- Control flow: if/else, for, while (phase iii)
- Builtin function calls (phase iii/iv)
- User-defined function calls between functions (phase iii)
- Out/inout parameters (phase iii)
- WASM linear memory (phase iii)
- Browser/playground integration (phase iv)
- Float numeric mode (future)
