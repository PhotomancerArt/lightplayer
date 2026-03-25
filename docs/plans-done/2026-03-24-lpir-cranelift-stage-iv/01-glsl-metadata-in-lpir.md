# Phase 1: Glsl metadata types in `lpir`

## Scope

Add `glsl_metadata.rs` to the `lpir` crate with types needed for calling
conventions and Level 1 marshalling. Decide where **`GlslType`** lives: either
**move** `GlslType` from `lp-glsl-naga` into `lpir` (and re-export from naga), or
define a parallel `GlslType` in `lpir` filled during lowering — **prefer move**
to a single source of truth if the diff stays small.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### Types (in `lpir/src/glsl_metadata.rs`)

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GlslParamQualifier {
    In,
    Out,
    InOut,
}

#[derive(Clone, Debug)]
pub struct GlslParamMeta {
    pub name: String,
    pub qualifier: GlslParamQualifier,
    pub ty: GlslType, // same enum as today in naga, moved here
}

#[derive(Clone, Debug)]
pub struct GlslFunctionMeta {
    pub name: String,
    pub params: Vec<GlslParamMeta>,
    pub return_type: GlslType,
}

#[derive(Clone, Debug, Default)]
pub struct GlslModuleMeta {
    pub functions: Vec<GlslFunctionMeta>,
}
```

### `GlslType`

Move the existing `GlslType` enum from `lp-glsl-naga/src/lib.rs` into `lpir`
(or keep in naga and duplicate — document choice in this phase).

### `lpir/src/lib.rs`

`pub use glsl_metadata::{GlslFunctionMeta, GlslModuleMeta, GlslParamMeta, GlslParamQualifier, GlslType};`

### Tests

Unit test: construct a minimal `GlslModuleMeta`, assert field access.

## Validate

```
cargo check -p lpir
cargo test -p lpir
```

After moving `GlslType`, update `lp-glsl-naga` imports in **Phase 2** (or same
commit if preferred).
