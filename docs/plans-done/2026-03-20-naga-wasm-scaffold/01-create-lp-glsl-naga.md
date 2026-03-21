# Phase 1: Create lp-glsl-naga crate

## Scope

Create `lp-glsl/lp-glsl-naga/`, a `no_std`-compatible crate that wraps
`naga::front::glsl::Frontend` and returns a `NagaModule` (the `naga::Module`
plus per-function metadata as `FunctionInfo`).

This crate owns `FloatMode` and `GlslType` — the shared types that downstream
crates (`lp-glsl-wasm`, `lp-glsl-filetests`) will use instead of the ones from
`lp-glsl-frontend`.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. Cargo.toml

```toml
[package]
name = "lp-glsl-naga"
version.workspace = true
edition.workspace = true
license.workspace = true

[lints]
workspace = true

[dependencies]
naga = { version = "29.0.0", default-features = false, features = ["glsl-in"] }
```

Add `"lp-glsl/lp-glsl-naga"` to both `members` and `default-members` in the
workspace `Cargo.toml`.

### 2. src/lib.rs

```rust
#![no_std]

extern crate alloc;

use alloc::{string::String, vec::Vec};
use core::fmt;

pub use naga;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FloatMode {
    Q32,
    Float,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GlslType {
    Void,
    Float,
    Int,
    UInt,
    Bool,
    Vec2,
    Vec3,
    Vec4,
    IVec2,
    IVec3,
    IVec4,
    UVec2,
    UVec3,
    UVec4,
    BVec2,
    BVec3,
    BVec4,
}

#[derive(Clone, Debug)]
pub struct FunctionInfo {
    pub name: String,
    pub params: Vec<(String, GlslType)>,
    pub return_type: GlslType,
}

pub struct NagaModule {
    pub module: naga::Module,
    pub functions: Vec<FunctionInfo>,
}

#[derive(Debug)]
pub enum CompileError {
    Parse(String),
    UnsupportedType(String),
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(msg) => write!(f, "GLSL parse error: {msg}"),
            Self::UnsupportedType(msg) => write!(f, "unsupported type: {msg}"),
        }
    }
}

pub fn compile(source: &str) -> Result<NagaModule, CompileError> {
    let module = parse_glsl(source)?;
    let functions = extract_functions(&module)?;
    Ok(NagaModule { module, functions })
}
```

### 3. Helper functions (bottom of lib.rs)

`parse_glsl(source) → Result<naga::Module, CompileError>`:
- Create `naga::front::glsl::Frontend::default()`
- Use `naga::front::glsl::Options::from(naga::ShaderStage::Vertex)`
- Call `frontend.parse(&options, source)`, map errors to `CompileError::Parse`

`extract_functions(module) → Result<Vec<FunctionInfo>, CompileError>`:
- Iterate `module.functions.iter()`
- For each function with a name, extract:
  - `name` from `function.name.clone().unwrap_or_default()`
  - `params` from `function.arguments` — resolve `module.types[arg.ty].inner`
    via `naga_type_to_glsl()`
  - `return_type` from `function.result` (or `GlslType::Void` if `None`)

`naga_type_to_glsl(inner: &naga::TypeInner) → Result<GlslType, CompileError>`:
- `TypeInner::Scalar { kind: Float, width: 4 }` → `GlslType::Float`
- `TypeInner::Scalar { kind: Sint, width: 4 }` → `GlslType::Int`
- `TypeInner::Scalar { kind: Uint, width: 4 }` → `GlslType::UInt`
- `TypeInner::Scalar { kind: Bool, .. }` → `GlslType::Bool`
- `TypeInner::Vector { size: Bi, scalar: { kind: Float, .. } }` → `GlslType::Vec2`
- ... (all vector combinations)
- Anything else → `CompileError::UnsupportedType`

### 4. Tests (in mod tests at top of file, below the public API)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_float_add() {
        let src = "float add(float a, float b) { return a + b; }";
        let result = compile(src).unwrap();
        assert_eq!(result.functions.len(), 1);
        assert_eq!(result.functions[0].name, "add");
        assert_eq!(result.functions[0].return_type, GlslType::Float);
        assert_eq!(result.functions[0].params.len(), 2);
    }

    #[test]
    fn parse_int_function() {
        let src = "int negate(int x) { return -x; }";
        let result = compile(src).unwrap();
        assert_eq!(result.functions[0].return_type, GlslType::Int);
    }

    #[test]
    fn parse_void_function() {
        let src = "void do_nothing() { }";
        let result = compile(src).unwrap();
        assert_eq!(result.functions[0].return_type, GlslType::Void);
    }

    #[test]
    fn parse_multiple_functions() {
        let src = r#"
            float foo() { return 1.0; }
            float bar() { return 2.0; }
        "#;
        let result = compile(src).unwrap();
        assert_eq!(result.functions.len(), 2);
    }

    #[test]
    fn naga_module_accessible() {
        let src = "float f() { return 1.0; }";
        let result = compile(src).unwrap();
        assert!(result.module.functions.len() >= 1);
    }
}
```

## Validate

```bash
cargo test -p lp-glsl-naga
cargo check -p lp-glsl-naga
```

Ensure no warnings.
