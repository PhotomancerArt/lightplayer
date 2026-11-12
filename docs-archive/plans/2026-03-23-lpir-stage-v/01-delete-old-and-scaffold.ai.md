# Phase 1: Delete Old Emitter + Scaffold New Structure

## Scope

Delete the old Naga-direct emitter code. Create the new `emit/` directory
module with stubs. Update `Cargo.toml` and `lib.rs`. The crate should
compile but `emit_module` returns an error stub.

## Deletions

- `src/emit.rs` — 1970-line Naga-direct emitter
- `src/emit_vec.rs` — vector lowering
- `src/locals.rs` — complex local allocation
- `src/lpfn.rs` — LPFX resolution from Naga
- `src/types.rs` — Naga type → WASM type mapping

## New files

### `src/emit/mod.rs`

```rust
pub(crate) fn emit_module(
    ir: &lpir::module::IrModule,
    options: &crate::options::WasmOptions,
) -> Result<Vec<u8>, String>
```

Initial implementation: return `Err("not yet implemented")`.

### `src/emit/func.rs`

### `src/emit/ops.rs`

### `src/emit/q32.rs`

### `src/emit/control.rs`

### `src/emit/memory.rs`

### `src/emit/imports.rs`

All empty stubs.

### `Cargo.toml`

Add `lpir` dependency:

```toml
lpir = { path = "../lpir" }
```

Remove `naga` direct dependency if no longer needed (check if `module.rs`
or other kept files still reference it). Likely keep `lps-frontend`
(for `compile()` and `FloatMode`).

### `lib.rs`

Update module declarations:

```rust
mod emit;
pub mod module;
pub mod options;
```

Remove `emit_vec`, `locals`, `lpfn`, `types` module declarations.

Update `glsl_wasm()` to call the new pipeline:

```rust
pub fn glsl_wasm(source: &str, options: WasmOptions) -> Result<WasmModule, GlslWasmError> {
    let naga_module = lps_frontend::compile(source)?;
    let ir_module = lps_frontend::lower::lower(&naga_module)
        .map_err(|e| GlslWasmError::Codegen(e.to_string()))?;
    let wasm_bytes = emit::emit_module(&ir_module, &options)
        .map_err(GlslWasmError::Codegen)?;
    let exports = collect_exports(&ir_module, &naga_module, &options);
    Ok(WasmModule { bytes: wasm_bytes, exports })
}
```

### `module.rs`

Update `WasmExport` to not depend on deleted `types.rs`. The WASM
`ValType` mapping is simple enough to inline or move into `module.rs`:

- Q32: float → `I32`, int/uint/bool → `I32`
- Float: float → `F32`, int/uint/bool → `I32`

Keep `GlslType` from `lps-frontend` for export metadata.

### `collect_exports`

Update to take both `&IrModule` (for function names, entry flags) and
`&NagaModule` (for GLSL-level type metadata needed by the filetest
runner).

## Validate

```
cargo check -p lps-wasm
```

Crate compiles. Tests will fail (emitter returns error). That's expected.
