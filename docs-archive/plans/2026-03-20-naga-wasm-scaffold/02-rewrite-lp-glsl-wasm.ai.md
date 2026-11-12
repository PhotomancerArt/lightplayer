# Phase 2: Rewrite lps-wasm

## Scope

Replace `lps-wasm`'s internals. The crate keeps its public API shape
(`glsl_wasm()` → `WasmModule`) but consumes `naga::Module` instead of
`TypedShader`. The old `codegen/` tree (32 files) is deleted and replaced
with a small set of files.

Phase I scope: scalars only (float, int, uint, bool). No vectors, no builtins,
no control flow beyond `return`, no user-defined function calls (each function
is independent).

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. Update Cargo.toml

```toml
[package]
name = "lps-wasm"
version.workspace = true
edition.workspace = true
license.workspace = true

[lints]
workspace = true

[dependencies]
lps-frontend = { path = "../lps-frontend" }
naga = { version = "29.0.0", default-features = false, features = ["glsl-in"] }
wasm-encoder = "0.245"
log = { workspace = true, default-features = false }

[dev-dependencies]
anyhow = { workspace = true }
wasmtime = "42"
```

Removed: `lps-frontend`, `lps-builtin-ids`, `glsl`, `hashbrown`.
Added: `lps-frontend`, `naga`.

### 2. Delete old codegen tree

Remove the entire `src/codegen/` directory (32 files). It will be replaced
with `src/emit.rs` and `src/locals.rs`.

### 3. src/lib.rs (entry point)

```rust
#![no_std]

extern crate alloc;

mod emit;
mod locals;
pub mod module;
pub mod options;
pub mod types;

pub use lps_frontend::{FloatMode, GlslType};
pub use module::{WasmExport, WasmModule};
pub use options::WasmOptions;

use alloc::vec::Vec;
use lps_frontend::{CompileError, NagaModule};

pub fn glsl_wasm(source: &str, options: WasmOptions) -> Result<WasmModule, CompileError> {
    let naga_module = lps_frontend::compile(source)?;
    let wasm_bytes = emit::emit_module(&naga_module, &options)?;
    let exports = collect_exports(&naga_module, &options);
    Ok(WasmModule {
        bytes: wasm_bytes,
        exports,
    })
}

fn collect_exports(naga_module: &NagaModule, options: &WasmOptions) -> Vec<WasmExport> {
    naga_module
        .functions
        .iter()
        .map(|fi| {
            let params: Vec<_> = fi
                .params
                .iter()
                .flat_map(|(_, ty)| types::glsl_type_to_wasm_components(ty, options.float_mode))
                .collect();
            let results = types::glsl_type_to_wasm_components(&fi.return_type, options.float_mode);
            WasmExport {
                name: fi.name.clone(),
                params,
                results,
                return_type: fi.return_type.clone(),
                param_types: fi.params.iter().map(|(_, ty)| ty.clone()).collect(),
            }
        })
        .collect()
}
```

### 4. src/module.rs (updated — no FunctionSignature)

```rust
#![allow(unused)]

use alloc::{string::String, vec::Vec};
use lps_frontend::GlslType;
pub use wasm_encoder::ValType as WasmValType;

#[derive(Debug, Clone)]
pub struct WasmModule {
    pub bytes: Vec<u8>,
    pub exports: Vec<WasmExport>,
}

#[derive(Debug, Clone)]
pub struct WasmExport {
    pub name: String,
    pub params: Vec<WasmValType>,
    pub results: Vec<WasmValType>,
    pub return_type: GlslType,
    pub param_types: Vec<GlslType>,
}
```

Key change: `signature: FunctionSignature` replaced with `return_type: GlslType`
and `param_types: Vec<GlslType>`. This removes the dependency on
`lps-frontend`.

### 5. src/options.rs

```rust
use lps_frontend::FloatMode;

#[derive(Debug, Clone)]
pub struct WasmOptions {
    pub float_mode: FloatMode,
}

impl Default for WasmOptions {
    fn default() -> Self {
        Self {
            float_mode: FloatMode::Q32,
        }
    }
}
```

Removed `max_errors` (Naga handles its own error collection).

### 6. src/types.rs

Same logic as before but uses `GlslType` from `lps-frontend`:

```rust
use alloc::vec::Vec;
use lps_frontend::{FloatMode, GlslType};
use wasm_encoder::ValType;

pub fn glsl_type_to_wasm_components(ty: &GlslType, float_mode: FloatMode) -> Vec<ValType> {
    match ty {
        GlslType::Void => alloc::vec![],
        GlslType::Bool | GlslType::Int | GlslType::UInt => alloc::vec![ValType::I32],
        GlslType::Float => alloc::vec![scalar_float_vt(float_mode)],
        GlslType::Vec2 | GlslType::IVec2 | GlslType::UVec2 | GlslType::BVec2 => {
            alloc::vec![component_vt(ty, float_mode); 2]
        }
        GlslType::Vec3 | GlslType::IVec3 | GlslType::UVec3 | GlslType::BVec3 => {
            alloc::vec![component_vt(ty, float_mode); 3]
        }
        GlslType::Vec4 | GlslType::IVec4 | GlslType::UVec4 | GlslType::BVec4 => {
            alloc::vec![component_vt(ty, float_mode); 4]
        }
    }
}

fn scalar_float_vt(fm: FloatMode) -> ValType {
    match fm {
        FloatMode::Q32 => ValType::I32,
        FloatMode::Float => ValType::F32,
    }
}

fn component_vt(ty: &GlslType, fm: FloatMode) -> ValType {
    match ty {
        GlslType::Vec2 | GlslType::Vec3 | GlslType::Vec4 => scalar_float_vt(fm),
        _ => ValType::I32,
    }
}
```

### 7. src/locals.rs (local allocation)

```rust
use alloc::collections::BTreeMap;
use naga::{Expression, Function, Handle, LocalVariable, Statement};

/// WASM local index allocator for a single function.
///
/// WASM locals 0..n-1 are the function parameters.
/// Additional locals are allocated for naga `LocalVariable`s that are
/// not parameter aliases.
pub struct LocalAlloc {
    param_count: u32,
    param_aliases: BTreeMap<Handle<LocalVariable>, u32>,
    local_map: BTreeMap<Handle<LocalVariable>, u32>,
    next_local: u32,
}

impl LocalAlloc {
    pub fn new(func: &Function) -> Self {
        let param_count = func.arguments.len() as u32;
        let param_aliases = build_param_aliases(func);

        let mut local_map = BTreeMap::new();
        let mut next = param_count;
        for (handle, _lv) in func.local_variables.iter() {
            if !param_aliases.contains_key(&handle) {
                local_map.insert(handle, next);
                next += 1;
            }
        }

        Self {
            param_count,
            param_aliases,
            local_map,
            next_local: next,
        }
    }

    /// Number of extra locals (beyond parameters) to declare in WASM.
    pub fn extra_local_count(&self) -> u32 {
        self.next_local - self.param_count
    }

    /// Resolve an expression to a WASM local index.
    /// Returns the argument index for parameter aliases, or the allocated
    /// local index for non-parameter locals.
    pub fn resolve_local_variable(&self, lv: Handle<LocalVariable>) -> Option<u32> {
        self.param_aliases
            .get(&lv)
            .or_else(|| self.local_map.get(&lv))
            .copied()
    }
}
```

`build_param_aliases()` is the same logic as `param_local_to_argument()` from
the spike — walks the function body to find `Store { LocalVariable, FunctionArgument }`
pairs.

### 8. src/emit.rs (WASM emission)

Core of the rewrite. Walks `naga::Module` and emits WASM via `wasm_encoder`.

```rust
use alloc::vec::Vec;
use crate::locals::LocalAlloc;
use crate::options::WasmOptions;
use lps_frontend::{CompileError, FloatMode, NagaModule};
use naga::{BinaryOperator, Expression, Handle, Module, Function, Statement, TypeInner, ScalarKind, Literal};
use wasm_encoder::{
    CodeSection, ExportKind, ExportSection, Function as WasmFunction,
    FunctionSection, Instruction, Module as WasmModule, TypeSection, ValType,
};

pub fn emit_module(naga_module: &NagaModule, options: &WasmOptions) -> Result<Vec<u8>, CompileError> {
    let module = &naga_module.module;
    let mode = options.float_mode;

    let mut types_sec = TypeSection::new();
    let mut func_sec = FunctionSection::new();
    let mut export_sec = ExportSection::new();
    let mut code_sec = CodeSection::new();

    for (func_idx, (handle, func)) in module.functions.iter().enumerate() {
        let name = func.name.as_deref().unwrap_or("_unnamed");
        let fi = &naga_module.functions[func_idx]; // parallel iteration

        // Build WASM function type
        let params: Vec<ValType> = fi.params.iter()
            .flat_map(|(_, ty)| crate::types::glsl_type_to_wasm_components(ty, mode))
            .collect();
        let results: Vec<ValType> =
            crate::types::glsl_type_to_wasm_components(&fi.return_type, mode);

        let type_idx = func_idx as u32;
        types_sec.ty().function(params, results);
        func_sec.function(type_idx);
        export_sec.export(name, ExportKind::Func, func_idx as u32);

        let alloc = LocalAlloc::new(func);
        let extra = alloc.extra_local_count();
        let locals: Vec<(u32, ValType)> = if extra > 0 {
            // TODO: determine correct ValType per local (Phase II)
            let vt = match mode {
                FloatMode::Q32 => ValType::I32,
                FloatMode::Float => ValType::F32,
            };
            alloc::vec![(extra, vt)]
        } else {
            alloc::vec![]
        };
        let mut wasm_fn = WasmFunction::new(locals);

        emit_block(module, func, &func.body, &mut wasm_fn, mode, &alloc)?;

        wasm_fn.instruction(&Instruction::End);
        code_sec.function(&wasm_fn);
    }

    let mut out = WasmModule::new();
    out.section(&types_sec);
    out.section(&func_sec);
    out.section(&export_sec);
    out.section(&code_sec);
    Ok(out.finish())
}
```

### emit_block / emit_stmt / emit_expr

`emit_block(block)`: iterates statements, dispatches to `emit_stmt`.

`emit_stmt(stmt)`:

- `Statement::Emit(range)`: evaluates expressions in range that need it
  (Phase I: no-op, expressions are emitted on demand when referenced)
- `Statement::Store { pointer, value }`: emit value, `local.set` the target
- `Statement::Return { value }`: emit value expression, `return`
- `Statement::Block(inner)`: recurse into `emit_block`

`emit_expr(expr_handle)`: recursive, pushes one scalar value onto WASM stack.

- `Expression::Literal(Float32(v))`:
    - Float mode: `f32.const v`
    - Q32 mode: `i32.const (v * 65536.0) as i32`
- `Expression::Literal(Sint32(v))`: `i32.const v`
- `Expression::Literal(Uint32(v))`: `i32.const v as i32`
- `Expression::Literal(Bool(b))`: `i32.const (b as i32)`
- `Expression::FunctionArgument(idx)`: `local.get idx`
- `Expression::Load { pointer }`:
    - If `pointer` → `LocalVariable(lv)`: `local.get alloc.resolve(lv)`
- `Expression::Binary { op, left, right }`:
    - Emit left, emit right
    - Match `(op, scalar_kind, mode)` to WASM instruction:
        - `(Add, Float, Float)` → `f32.add`
        - `(Add, Float, Q32)` → `i32.add` (TODO: saturating in Phase II)
        - `(Add, Sint/Uint, _)` → `i32.add`
        - `(Sub, ...)` → `f32.sub` / `i32.sub`
        - `(Mul, Float, Float)` → `f32.mul`
        - `(Mul, Float, Q32)` → Q32 multiply sequence (mul, shift)
        - `(Mul, Sint/Uint, _)` → `i32.mul`
        - `(Div, Float, Float)` → `f32.div`
        - `(Div, Float, Q32)` → Q32 divide sequence
        - `(Div, Sint, _)` → `i32.div_s`
        - `(Div, Uint, _)` → `i32.div_u`
        - `(Equal, ...)` → `f32.eq` / `i32.eq`
        - `(NotEqual, ...)` → `f32.ne` / `i32.ne`
        - `(Less, Float, Float)` → `f32.lt`
        - `(Less, Sint, _)` → `i32.lt_s`
        - `(Less, Uint, _)` → `i32.lt_u`
        - (and so on for `LessEqual`, `Greater`, `GreaterEqual`)
- `Expression::Unary { op: Negate, expr }`:
    - Float mode: emit expr, `f32.neg`
    - Q32/Int: `i32.const 0`, emit expr, `i32.sub`
- `Expression::As { expr, kind, convert }`: type conversions
    - `float → int` (Q32): shift right 16, floor
    - `int → float` (Q32): shift left 16
    - `float → int` (Float): `i32.trunc_f32_s`
    - `int → float` (Float): `f32.convert_i32_s`
    - `uint → float`, `float → uint`, `int → uint`, `uint → int`: etc.
- `Expression::Select { condition, accept, reject }`: ternary
    - emit accept, emit reject, emit condition, `select`

### Expression type resolution

To emit the correct WASM instruction for `Binary { op, left, right }`, we need
to know the scalar type of the operands. Use `naga::Module::types` and the
expression's result type.

Helper `expr_scalar_kind(module, func, expr_handle) → ScalarKind`:

- Walk expression to find its type. For `Literal`, `FunctionArgument`,
  `Binary`, `Load`, etc., resolve to `ScalarKind` (Float, Sint, Uint, Bool).

### 9. Tests (in emit.rs or a separate test file)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use wasmtime::{Config, Engine, Instance, Linker, Store};

    fn run_f32(source: &str, func_name: &str, args: &[f32]) -> f32 {
        let opts = WasmOptions { float_mode: FloatMode::Float };
        let module = crate::glsl_wasm(source, opts).unwrap();
        // ... wasmtime boilerplate, call func_name with args, return f32
    }

    fn run_q32(source: &str, func_name: &str, args: &[f32]) -> f32 {
        let opts = WasmOptions { float_mode: FloatMode::Q32 };
        let module = crate::glsl_wasm(source, opts).unwrap();
        // ... wasmtime boilerplate, Q16.16 conversion
    }

    #[test]
    fn float_literal_return() {
        let v = run_f32("float f() { return 3.14; }", "f", &[]);
        assert!((v - 3.14).abs() < 0.01);
    }

    #[test]
    fn float_add() {
        let v = run_f32("float add(float a, float b) { return a + b; }", "add", &[1.5, 2.5]);
        assert!((v - 4.0).abs() < 0.001);
    }

    #[test]
    fn float_subtract() {
        let v = run_f32("float sub(float a, float b) { return a - b; }", "sub", &[5.0, 3.0]);
        assert!((v - 2.0).abs() < 0.001);
    }

    #[test]
    fn float_multiply() {
        let v = run_f32("float mul(float a, float b) { return a * b; }", "mul", &[3.0, 4.0]);
        assert!((v - 12.0).abs() < 0.001);
    }

    #[test]
    fn float_divide() {
        let v = run_f32("float div(float a, float b) { return a / b; }", "div", &[10.0, 4.0]);
        assert!((v - 2.5).abs() < 0.001);
    }

    #[test]
    fn local_variable() {
        let v = run_f32("float f() { float x = 2.5; return x; }", "f", &[]);
        assert!((v - 2.5).abs() < 0.001);
    }

    #[test]
    fn local_variable_reassign() {
        let src = "float f() { float x = 1.0; x = x + 2.5; return x; }";
        let v = run_f32(src, "f", &[]);
        assert!((v - 3.5).abs() < 0.001);
    }

    #[test]
    fn nested_expression() {
        let src = "float f() { return (2.0 + 3.0) + (4.0 + 5.0); }";
        let v = run_f32(src, "f", &[]);
        assert!((v - 14.0).abs() < 0.001);
    }

    #[test]
    fn q32_literal_return() {
        let v = run_q32("float f() { return 3.0; }", "f", &[]);
        assert!((v - 3.0).abs() < 0.01);
    }

    #[test]
    fn q32_add() {
        let v = run_q32("float add(float a, float b) { return a + b; }", "add", &[1.5, 2.5]);
        assert!((v - 4.0).abs() < 0.01);
    }

    #[test]
    fn int_add() {
        // Need to compile with int params. Use wasmtime i32 call.
        let src = "int add(int a, int b) { return a + b; }";
        // ... run and check result is 7 for (3, 4)
    }

    #[test]
    fn multiple_functions_exported() {
        let src = r#"
            float foo() { return 1.0; }
            float bar() { return 2.0; }
        "#;
        let opts = WasmOptions { float_mode: FloatMode::Float };
        let module = crate::glsl_wasm(src, opts).unwrap();
        assert_eq!(module.exports.len(), 2);
    }
}
```

## Validate

```bash
cargo test -p lps-wasm
cargo check -p lps-wasm
```

Ensure no warnings. The old `lps-wasm` tests will be removed with the
old codegen; new tests replace them.
