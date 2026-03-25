# Phase 2: Builtin Declaration, Import Resolution, and Symbol Lookup

## Scope

Create `builtins.rs` in `lpir-cranelift`. Implement import resolution
(`ImportDecl` → `BuiltinId`), builtin declaration in the JIT module, and
the symbol lookup function. Update `Cargo.toml` with new dependencies.
Update `jit_module.rs` to accept `FloatMode` and wire up builtins. No Q32
emission yet — this phase sets up the JIT plumbing.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. Update `Cargo.toml`

Add dependencies:

```toml
lp-glsl-builtin-ids = { path = "../lp-glsl-builtin-ids" }
lp-glsl-builtins = { path = "../lp-glsl-builtins" }
```

### 2. Create `src/builtins.rs`

#### resolve_import

Same logic as WASM emitter's `resolve_builtin_id` in
`lp-glsl-wasm/src/emit/imports.rs`. Dispatches on `ImportDecl.module_name`:

```rust
use lp_glsl_builtin_ids::{
    BuiltinId, glsl_builtin_mapping::{
        glsl_q32_math_builtin_id, lpir_q32_builtin_id, glsl_lpfx_q32_builtin_id,
        GlslParamKind,
    },
};
use lpir::module::ImportDecl;
use lpir::FloatMode;

use crate::error::CompileError;

pub(crate) fn resolve_import(
    decl: &ImportDecl,
    mode: FloatMode,
) -> Result<BuiltinId, CompileError> {
    match (decl.module_name.as_str(), mode) {
        ("glsl", FloatMode::Q32) => {
            let ac = decl.param_types.len();
            glsl_q32_math_builtin_id(&decl.func_name, ac)
                .ok_or_else(|| CompileError::unsupported(
                    format!("unsupported glsl import `{}` (arity {ac})", decl.func_name)
                ))
        }
        ("lpir", FloatMode::Q32) => {
            let ac = decl.param_types.len();
            lpir_q32_builtin_id(&decl.func_name, ac)
                .ok_or_else(|| CompileError::unsupported(
                    format!("unsupported lpir import `{}` (arity {ac})", decl.func_name)
                ))
        }
        ("lpfx", FloatMode::Q32) => {
            let base = lpfx_strip_suffix(&decl.func_name)?;
            let kinds = lpfx_glsl_kinds_from_decl(decl)?;
            glsl_lpfx_q32_builtin_id(base, &kinds)
                .ok_or_else(|| CompileError::unsupported(
                    format!("unsupported lpfx import `{}`", decl.func_name)
                ))
        }
        (m, _) => Err(CompileError::unsupported(
            format!("unsupported import module `{m}` with mode {mode:?}")
        )),
    }
}
```

Port `lpfx_strip_suffix` and `lpfx_glsl_kinds_from_decl` from the WASM
emitter (small string helpers for LPFX overload resolution using
`ImportDecl.lpfx_glsl_params`).

#### declare_builtins

Iterate all builtins for the current mode, declare each in the module:

```rust
use cranelift_codegen::ir::{AbiParam, Signature, types};
use cranelift_codegen::isa::CallConv;
use cranelift_jit::JITModule;
use cranelift_module::{Linkage, Module};

pub(crate) fn declare_builtins(
    module: &mut JITModule,
    mode: FloatMode,
) -> Result<(), CompileError> {
    let call_conv = module.isa().default_call_conv();
    for builtin in BuiltinId::all() {
        match builtin.mode() {
            Some(m) if mode == FloatMode::Q32 && m != lp_glsl_builtin_ids::Mode::Q32 => continue,
            Some(m) if mode == FloatMode::F32 && m != lp_glsl_builtin_ids::Mode::F32 => continue,
            _ => {} // None = mode-independent, declare for all
        }
        let sig = signature_for_builtin(*builtin, call_conv, mode);
        module.declare_function(builtin.name(), Linkage::Import, &sig)
            .map_err(|e| CompileError::cranelift(
                format!("declare builtin {}: {e}", builtin.name())
            ))?;
    }
    Ok(())
}
```

`signature_for_builtin`: derive from `BuiltinId`'s param/return counts.
For Q32, all float params are I32. For most builtins the signature is
straightforward (1-3 I32 params → 1 I32 return). For LPFX builtins with
pointer params, use `pointer_type`. Can derive from the LPIR `ImportDecl`
types at call time instead — but declaring needs a signature up front.

Approach: since `BuiltinId` doesn't carry signature info, use the LPIR
`ImportDecl` to derive the signature when resolving imports. For the
`declare_builtins` pre-declaration, use a simpler approach: iterate the
module's `imports` and declare each resolved `BuiltinId` with the correct
signature from the `ImportDecl`.

Revised approach — **declare per-module imports, not all builtins**:

```rust
pub(crate) fn declare_module_imports(
    module: &mut JITModule,
    ir: &IrModule,
    mode: FloatMode,
) -> Result<Vec<BuiltinId>, CompileError> {
    let call_conv = module.isa().default_call_conv();
    let mut builtin_ids = Vec::with_capacity(ir.imports.len());
    for decl in &ir.imports {
        let bid = resolve_import(decl, mode)?;
        let sig = import_signature(decl, call_conv, mode);
        module.declare_function(bid.name(), Linkage::Import, &sig)
            .map_err(|e| CompileError::cranelift(
                format!("declare import {}: {e}", bid.name())
            ))?;
        builtin_ids.push(bid);
    }
    Ok(builtin_ids)
}
```

This only declares what the module actually uses. The signature comes from
`ImportDecl.param_types` / `return_types`, mapped through `FloatMode`
(F32 → types::F32, Q32 with IrType::F32 → types::I32, IrType::I32 → types::I32).

#### get_function_pointer

Large match on `BuiltinId` → `extern "C" fn` pointer cast to `*const u8`.
Same pattern as old crate's `registry.rs`. Each arm looks like:

```rust
pub(crate) fn get_function_pointer(id: BuiltinId) -> *const u8 {
    match id {
        BuiltinId::LpGlslSinQ32 => lp_glsl_builtins::builtins::glsl::sin_q32::__lp_glsl_sin_q32 as *const u8,
        BuiltinId::LpLpirFaddQ32 => lp_glsl_builtins::builtins::lpir::fadd_q32::__lp_lpir_fadd_q32 as *const u8,
        // ... all variants ...
    }
}
```

This is generated code in the old crate. For now, write it by hand (or
use a macro). Check if the old crate's `registry.rs` can be reused directly
— it may be importable.

#### symbol_lookup_fn

```rust
pub(crate) fn symbol_lookup_fn(mode: FloatMode) -> Box<dyn Fn(&str) -> Option<*const u8>> {
    Box::new(move |name: &str| -> Option<*const u8> {
        for builtin in BuiltinId::all() {
            if builtin.name() == name {
                return Some(get_function_pointer(*builtin));
            }
        }
        None
    })
}
```

### 3. Update `jit_module.rs`

Change signature:

```rust
pub fn jit_from_ir(
    ir: &IrModule,
    mode: FloatMode,
) -> Result<(JITModule, Vec<FuncId>), CompileError>
```

Before creating the JIT module:

```rust
let mut jit_builder = JITBuilder::with_isa(isa, default_libcall_names());
jit_builder.symbol_lookup_fn(builtins::symbol_lookup_fn(mode));
let mut jit_module = JITModule::new(jit_builder);
```

After module creation, declare imports:

```rust
let builtin_ids = builtins::declare_module_imports(&mut jit_module, ir, mode)?;
```

Per-function, create import FuncRefs:

```rust
let import_func_refs: Vec<FuncRef> = builtin_ids.iter().map(|bid| {
    let fid = jit_module.get_name(bid.name()).expect("declared");
    jit_module.declare_func_in_func(fid, builder.func)
}).collect();
```

Wait — `declare_module_imports` declares by name and returns `BuiltinId`s.
We need `FuncId`s to create `FuncRef`s. Better to return `Vec<FuncId>`:

```rust
pub(crate) fn declare_module_imports(
    module: &mut JITModule,
    ir: &IrModule,
    mode: FloatMode,
) -> Result<Vec<FuncId>, CompileError>
```

Each declared import gives a `FuncId`. Store them. Per-function:

```rust
let import_func_refs: Vec<FuncRef> = import_func_ids.iter()
    .map(|fid| jit_module.declare_func_in_func(*fid, builder.func))
    .collect();
```

Pass `&import_func_refs` in `EmitCtx`.

### 4. Update `emit/mod.rs`

Add to `EmitCtx`:

```rust
pub(crate) struct EmitCtx<'a> {
    pub func_refs: &'a [FuncRef],
    pub import_func_refs: &'a [FuncRef],
    pub slots: &'a [StackSlot],
    pub ir: &'a IrModule,
    pub pointer_type: types::Type,
    pub float_mode: FloatMode,
}
```

### 5. Update `lib.rs`

```rust
mod builtins;

pub use jit_module::jit_from_ir;
pub use lpir::FloatMode;
```

Update existing test to pass `FloatMode::F32`:

```rust
let (jit, ids) = jit_from_ir(&ir, FloatMode::F32).expect("jit");
```

### 6. Test

**`test_jit_with_imports_rejects_without_mode`** — verify that a module with
imports but `FloatMode::F32` fails gracefully for now (F32 import resolution
not yet implemented — only Q32 mapping functions exist).

No Q32 emission tests yet (Phase 3+). This phase just verifies the plumbing
compiles and the F32 path still works.

## Validate

```
cargo check -p lpir-cranelift
cargo test -p lpir-cranelift
```
