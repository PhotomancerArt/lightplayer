# Phase 3: Update lps-filetests for new WASM backend

## Scope

Update the filetest WASM runner to use the new types from `lps-wasm`
(`GlslType`, `WasmExport`) instead of `FunctionSignature`/`Type` from
`lps-frontend`. Keep the `compile.rs` dispatch intact but adapt the
type conversions.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. Update lps-filetests/Cargo.toml

Add `lps-frontend` as a dependency (needed for `GlslType`, `FloatMode`):

```toml
lps-frontend = { path = "../lps-frontend" }
```

### 2. Update src/test_run/wasm_runner.rs

The runner currently uses:

- `lps_cranelift::semantic::functions::FunctionSignature` — for
  `exports: HashMap<String, FunctionSignature>`
- `lps_cranelift::semantic::types::Type` — for type dispatch in
  `call_bvec`, `call_ivec`, `call_uvec`, `call_vec`
- `lps_wasm::types::glsl_type_to_wasm_components` — for result type sizing
- `lps_wasm::FloatMode` — re-exported from `lps-frontend`

Changes:

**Imports**: Replace `FunctionSignature` and `Type` with `GlslType` and
`WasmExport`:

```rust
use lps_frontend::GlslType;
use lps_wasm::{WasmExport, WasmOptions, glsl_wasm};
use lps_wasm::types::glsl_type_to_wasm_components;
```

**`WasmExecutable` struct**: Change `exports` field:

```rust
pub struct WasmExecutable {
    store: Store<()>,
    instance: Instance,
    exports: HashMap<String, WasmExport>,
    float_mode: lps_frontend::FloatMode,
    wasm_bytes: Vec<u8>,
}
```

**`from_source()`**: Build `exports` from `module.exports`:

```rust
let exports: HashMap<String, WasmExport> = module
    .exports
    .into_iter()
    .map(|e| (e.name.clone(), e))
    .collect();
```

**Call dispatch methods** (`call_f32`, `call_i32`, `call_vec`, etc.):

Replace `sig.parameters.iter().map(|p| glsl_param_to_wasm(&p.ty, ...))` with:

```rust
let param_types: Vec<WasmValType> = export_info.params.clone();
```

Since `WasmExport.params` already contains `Vec<WasmValType>`, no conversion
needed for parameter types — they're already WASM types.

For return type dispatch (e.g. `call_bvec` checking `sig.return_type`), use
`export_info.return_type` which is `GlslType`:

```rust
// Before:
let ok = matches!((&sig.return_type, dim), (Type::BVec2, 2) | ...);

// After:
let ok = matches!((&export_info.return_type, dim),
    (GlslType::BVec2, 2) | (GlslType::BVec3, 3) | (GlslType::BVec4, 4));
```

For `glsl_type_to_wasm_components(&sig.return_type, ...)`, use
`&export_info.return_type`.

**`glsl_param_to_wasm()`**: This function can be removed entirely — parameter
WASM types are already in `WasmExport.params`.

**`glsl_value_to_wasm()`**: This function stays the same (converts `GlslValue`
to `wasmtime::Val` based on expected `WasmValType`).

**`GlslExecutable` trait**: The `get_function_signature()` method returns
`&FunctionSignature`. This trait is defined in `lps-cranelift` and the
WASM runner must implement it.

Options:

1. **Keep returning `FunctionSignature`** by constructing one from `WasmExport`.
   This is a bridge: build a `FunctionSignature` from `GlslType` info.
   Requires a conversion function `glsl_type_to_frontend_type()`.
2. **Change the trait** to return something generic. Too invasive for Phase I.

**Recommended**: Option 1. Add a conversion helper in `wasm_runner.rs`:

```rust
fn to_frontend_type(ty: &GlslType) -> Type {
    match ty {
        GlslType::Void => Type::Void,
        GlslType::Float => Type::Float,
        GlslType::Int => Type::Int,
        GlslType::UInt => Type::UInt,
        GlslType::Bool => Type::Bool,
        GlslType::Vec2 => Type::Vec2,
        GlslType::Vec3 => Type::Vec3,
        GlslType::Vec4 => Type::Vec4,
        GlslType::IVec2 => Type::IVec2,
        GlslType::IVec3 => Type::IVec3,
        GlslType::IVec4 => Type::IVec4,
        GlslType::UVec2 => Type::UVec2,
        GlslType::UVec3 => Type::UVec3,
        GlslType::UVec4 => Type::UVec4,
        GlslType::BVec2 => Type::BVec2,
        GlslType::BVec3 => Type::BVec3,
        GlslType::BVec4 => Type::BVec4,
    }
}

fn to_function_signature(export: &WasmExport) -> FunctionSignature {
    FunctionSignature {
        name: export.name.clone(),
        return_type: to_frontend_type(&export.return_type),
        parameters: export.param_types.iter().enumerate().map(|(i, ty)| {
            lps_cranelift::semantic::functions::Parameter {
                name: format!("p{i}"),
                ty: to_frontend_type(ty),
            }
        }).collect(),
    }
}
```

Store the converted signatures alongside the WasmExport data in the
`WasmExecutable` struct.

### 3. Update src/test_run/compile.rs

The `to_wasm_float_mode()` function converts filetest `FloatMode` to
`lps_wasm::FloatMode`. Update to use `lps_frontend::FloatMode`:

```rust
fn to_wasm_float_mode(fm: FloatMode) -> lps_frontend::FloatMode {
    match fm {
        FloatMode::Q32 => lps_frontend::FloatMode::Q32,
        FloatMode::F32 => lps_frontend::FloatMode::Float,
    }
}
```

The `WasmOptions` struct no longer has `max_errors`:

```rust
Backend::Wasm => {
    let options = WasmOptions {
        float_mode: to_wasm_float_mode(target.float_mode),
    };
    // ... rest unchanged
}
```

## Validate

```bash
cargo test -p lps-filetests -- scalar::float::op_add
```

This runs `scalar/float/op-add.glsl` on both `cranelift.q32` and `wasm.q32`
targets. The cranelift target should continue working (unchanged). The wasm
target should now pass through the new Naga pipeline.

Then run the broader scalar suite:

```bash
cargo test -p lps-filetests -- scalar
```

Fix any failures. Expect: `scalar/float/op-add`, `op-subtract`, `op-multiply`,
`op-divide` should pass. Tests involving features not yet implemented (e.g.
unary minus as a separate expression, type conversions between int/float) may
need `// unimplemented: wasm` annotations if they don't pass yet.
