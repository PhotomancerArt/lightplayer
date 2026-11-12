# Phase 3: Compilation pipeline

## Scope

Implement `compile.rs`: GLSL source -> LPIR -> compiled `LpvmModule`.
Validate that manifest inputs have matching `input_*` uniforms.
Wire `FxEngine::instantiate` to call this pipeline.

## Code organization reminders

- One concept per file.
- Place public functions/entry points first, helpers at the bottom.
- Keep related functionality grouped together.

## Implementation

### 3.1 `lpfx/lpfx-cpu/src/compile.rs`

This module is backend-agnostic -- it works with the `LpvmEngine` trait.

```rust
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use lpir::lpir_module::LpirModule;
use lps_shared::LpsModuleSig;
use lpvm::LpvmEngine;

use lpfx::FxManifest;
use lpfx::input::FxInputType;

pub struct CompiledEffect<M> {
    pub module: M,
    pub meta: LpsModuleSig,
    pub ir: LpirModule,
}

pub fn compile_glsl<E: LpvmEngine>(
    engine: &E,
    glsl: &str,
) -> Result<CompiledEffect<E::Module>, String> {
    let naga = lps_frontend::compile(glsl)
        .map_err(|e| format!("GLSL parse: {e}"))?;
    let (ir, meta) = lps_frontend::lower(&naga)
        .map_err(|e| format!("LPIR lower: {e}"))?;
    drop(naga);
    let module = engine.compile(&ir, &meta)
        .map_err(|e| format!("compile: {e}"))?;
    Ok(CompiledEffect { module, meta, ir })
}
```

### 3.2 Input-to-uniform validation

After compilation, check that every manifest `[input.X]` has a matching
`input_X` field in `LpsModuleSig::uniforms_type`:

```rust
pub fn validate_inputs(
    manifest: &FxManifest,
    meta: &LpsModuleSig,
) -> Result<(), String> {
    let uniforms = meta.uniforms_type.as_ref();
    for (name, _def) in &manifest.inputs {
        let uniform_name = format!("input_{name}");
        // Check that uniform_name exists in the uniforms struct
        if let Some(ut) = uniforms {
            if ut.type_at_path(&uniform_name).is_err() {
                return Err(format!(
                    "manifest input `{name}` has no matching uniform `{uniform_name}` in shader"
                ));
            }
        } else {
            return Err(format!(
                "shader has no uniforms but manifest declares input `{name}`"
            ));
        }
    }
    Ok(())
}
```

Uses `LpsTypePathExt::type_at_path` from `lps_shared::path_resolve` which
is already used by `encode_uniform_write`.

### 3.3 Wire into `FxEngine::instantiate`

In `lib.rs`, the `instantiate` method:

1. Calls `compile_glsl` with the module's GLSL source
2. Calls `validate_inputs` with the manifest and compiled metadata
3. Creates an `LpvmInstance` from the compiled module
4. Sets default uniform values from the manifest
5. Creates `CpuFxInstance` holding the instance + metadata

The backend-specific parts (which `LpvmEngine` to use, how to get a
`DirectCall`) depend on the feature. For `cranelift`:

```rust
#[cfg(feature = "cranelift")]
impl FxEngine for CpuFxEngine {
    // ...
    fn instantiate(&mut self, module: &FxModule, output: TextureId)
        -> Result<Self::Instance, Self::Error>
    {
        // compile, validate, instantiate, set defaults
    }
}
```

### 3.4 `FxValue` to `LpsValueF32` conversion

Add a helper to convert `lpfx::FxValue` -> `lps_shared::LpsValueF32` for
passing to `set_uniform`:

```rust
fn fx_value_to_lps(value: &FxValue) -> LpsValueF32 {
    match value {
        FxValue::F32(v) => LpsValueF32::Float(*v),
        FxValue::I32(v) => LpsValueF32::Int(*v),
        FxValue::Bool(v) => LpsValueF32::Int(if *v { 1 } else { 0 }),
        FxValue::Vec3(v) => LpsValueF32::Vec3(*v),
    }
}
```

## Validate

```bash
cargo check -p lpfx-cpu
```

No runtime tests yet -- the render loop (phase 4) is needed to exercise
the full pipeline.
