# M2 Stage I: lpvm-wasm - LPVM WASM Backend

## Scope

Create `lpvm/lpvm-wasm/` crate that implements the LPVM traits (`LpvmEngine`,
`LpvmModule`, `LpvmInstance`) for the WASM backend. This validates the M1
trait design against the strictest runtime model (wasmtime/browser APIs).

**Emission** (always, `no_std` + alloc): LPIR → WASM bytes via `wasm-encoder`.
**Runtime** (optional, `runtime` feature): wasmtime implementation of LPVM traits.

## Current State

### Existing WASM emission (`lp-shader/legacy/lps-wasm/`)

- **Full pipeline**: GLSL → LPIR → WASM (via `lps-frontend`)
- **Emission**: `emit/` module uses `wasm-encoder` (`no_std` + alloc)
- **Module structure**: `WasmModule` { bytes, exports, shadow_stack_base }
- **Exports**: `WasmExport` with WASM val types + logical param types from `lps-frontend`

### Existing WASM runtime (in `lp-shader/lps-filetests/src/test_run/`)

- `wasm_runner.rs`: `WasmExecutable` implements `GlslExecutable` via wasmtime
- `wasm_link.rs`: Links shader WASM with `lps-builtins-wasm.wasm`, shared memory
- Pattern: Engine → Store → Instance, reset shadow stack, call with VMContext

### LPVM traits (from `lp-shader/lpvm/`)

```rust
pub trait LpvmEngine {
    type Module: LpvmModule;
    type Error;
    fn compile(&self, ir: &IrModule, meta: &LpsModuleSig) -> Result<Self::Module, Self::Error>;
}

pub trait LpvmModule {
    type Instance: LpvmInstance;
    type Error;
    fn signatures(&self) -> &LpsModuleSig;
    fn instantiate(&self) -> Result<Self::Instance, Self::Error>;
}

pub trait LpvmInstance {
    type Error;
    fn call(&mut self, name: &str, args: &[LpsValue]) -> Result<LpsValue, Self::Error>;
}
```

## Answers

### Q1: Emission code strategy
**Answer:** Option A — Copy/adapt emission to `lpvm-wasm`. Keep `lps-wasm`
unchanged. This is parallel infrastructure; we'll switch to it later.

### Q2: WasmEngine structure
**Answer:** `WasmEngine` holds `wasmtime::Engine` + builtins bytes.
Parsed `Module` created per-compile (simple, matches `wasm_link.rs` pattern).

### Q3: State reset per call
**Answer:** `WasmInstance` holds `Store` (mutable). `call()` does shadow stack
+ fuel reset internally.

### Q4: Error types
**Answer:** Single `WasmError` enum with variants (Emission, Instantiation, Call).
Clean and maps to trait associated types.

### Q5: Raw bytes exposure
**Answer:** `WasmModule` exposes `fn bytes(&self) -> &[u8]`. Browser path needs
this. Not part of `LpvmModule` trait, but concrete method on `WasmModule`.

## Notes

### Emission code reference

Source files to adapt from `lp-shader/legacy/lps-wasm/src/emit/`:
- `mod.rs` - Main emission entry point (`emit_module`)
- `control.rs` - If/else, loops, switch emission
- `func.rs` - Function encoding, signatures, `FuncEmitCtx`
- `imports.rs` - Import filtering, builtin mapping
- `memory.rs` - Shadow stack, slot layout
- `ops.rs` - LPIR opcodes to WASM instructions
- `q32.rs` - Q32 arithmetic helpers

### Runtime code reference

Source files to adapt from `lp-shader/lps-filetests/src/test_run/`:
- `wasm_runner.rs` - wasmtime Store/Instance management, call marshaling
- `wasm_link.rs` - Linking builtins + shader, shared memory setup

### WASM constraints

- VMContext passed as first I32 parameter to all shader functions
- Shadow stack global (if present) must be reset to base before each call
- Fuel limits enforced via wasmtime fuel consumption
- Builtins linked from `lps-builtins-wasm.wasm` via "builtins" import module
- Memory imported/exported as "env.memory"
