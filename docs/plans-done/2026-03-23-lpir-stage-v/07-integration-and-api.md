# Phase 7: Integration + Public API

## Scope

Wire everything together. Update `lib.rs` with the new pipeline.
Update `module.rs` for export metadata. Update the filetest runner's
`WasmExecutable` to use the new pipeline. Ensure the crate compiles
and the public API is complete.

## Implementation

### `lib.rs` — updated pipeline

```rust
pub fn glsl_wasm(source: &str, options: WasmOptions) -> Result<WasmModule, GlslWasmError> {
    let naga_module = lps_naga::compile(source)?;
    let ir_module = lps_naga::lower::lower(&naga_module)
        .map_err(|e| GlslWasmError::Codegen(e.to_string()))?;
    let wasm_bytes = emit::emit_module(&ir_module, &options)
        .map_err(GlslWasmError::Codegen)?;
    let exports = collect_exports(&ir_module, &naga_module, &options);
    Ok(WasmModule { bytes: wasm_bytes, exports })
}
```

### `collect_exports` — metadata

Still needs GLSL-level type info for the filetest runner. The
`NagaModule::functions` provides `FunctionInfo` with name, param types
(as `GlslType`), and return type. The `IrModule::functions` provides
`is_entry`.

```rust
fn collect_exports(
    ir: &lpir::module::IrModule,
    naga: &NagaModule,
    options: &WasmOptions,
) -> Vec<WasmExport> {
    naga.functions.iter().enumerate().map(|(i, (_, fi))| {
        let params = fi.params.iter()
            .flat_map(|(_, ty)| glsl_type_to_wasm_valtypes(ty, options.float_mode))
            .collect();
        let results = glsl_type_to_wasm_valtypes(&fi.return_type, options.float_mode);
        WasmExport {
            name: fi.name.clone(),
            params,
            results,
            return_type: fi.return_type.clone(),
            param_types: fi.params.iter().map(|(_, ty)| ty.clone()).collect(),
        }
    }).collect()
}
```

The `glsl_type_to_wasm_valtypes` helper replaces the deleted `types.rs`:

```rust
fn glsl_type_to_wasm_valtypes(ty: &GlslType, mode: FloatMode) -> Vec<WasmValType> {
    match (ty, mode) {
        (GlslType::Void, _) => vec![],
        (GlslType::Float, FloatMode::Q32) => vec![WasmValType::I32],
        (GlslType::Float, FloatMode::Float) => vec![WasmValType::F32],
        (GlslType::Int | GlslType::UInt | GlslType::Bool, _) => vec![WasmValType::I32],
        // Vectors: N components of the scalar type
        // (not needed for scalar-only scope, but include for completeness)
        _ => vec![],  // error or todo for vectors
    }
}
```

### `module.rs` — minimal update

Remove the `use lps_wasm::types::*` if it existed. The `WasmValType`
re-export from `wasm_encoder::ValType` stays.

### Error handling

`GlslWasmError` already has `Frontend(CompileError)` and `Codegen(String)`.
Add a variant or use `Codegen` for LPIR lowering errors too (converting
`LowerError` to `String`).

### Filetest runner update

`lps-filetests/src/test_run/wasm_runner.rs` calls `glsl_wasm()` and
gets a `WasmModule`. Since the public API signature hasn't changed, the
filetest runner should work without modifications.

However, `wasm_link.rs` needs to link builtins when the WASM module
imports from the `builtins` module. This already works — the linker
checks for `builtins` imports and links `lps_builtins_wasm.wasm`.

Check that the import module name in the new emitter matches what
`wasm_link.rs` expects (it expects `"builtins"` for function imports
and `"env"` for memory).

## Validate

```
cargo check -p lps-wasm
cargo check -p lps-filetests
```

The full pipeline compiles. The public API is unchanged. The filetest
runner can instantiate the new WASM output.
