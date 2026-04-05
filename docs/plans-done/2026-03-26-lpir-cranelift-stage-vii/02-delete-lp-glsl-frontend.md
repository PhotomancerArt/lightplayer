# Phase 2: Delete `lp-glsl-frontend`

## Scope

Migrate the two remaining consumers of `lp-glsl-frontend`, then delete the
crate. Acceptance criteria: `lp-glsl-frontend` directory is gone and workspace
compiles.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. `lp-glsl-filetests` — replace `CompilationPipeline::parse`

In `lp-shader/lp-glsl-filetests/src/test_run/test_glsl.rs`:

- Replace `use lp_glsl_frontend::CompilationPipeline;` with
  `use glsl::parser::Parse;` and `use glsl::syntax::TranslationUnit;`.
- Change `CompilationPipeline::parse(source)` → `TranslationUnit::parse(source)`.
- `CompilationPipeline::parse` returns `ParseResult { shader, .. }`.
  `TranslationUnit::parse` returns `Result<TranslationUnit, ParseError>`.
  The call sites use `.shader` — replace with the direct `TranslationUnit`.

Example diff:

```rust
// Before:
let parse_result = CompilationPipeline::parse( & full_function_code);
Ok(parse_result) => {
match glsl_for_fn_graph( & parse_result.shader, ...) {

// After:
let parse_result = TranslationUnit::parse( & full_function_code);
Ok(tu) => {
match glsl_for_fn_graph( & tu, ...) {
```

Same for the test functions that use `CompilationPipeline::parse`.

Drop `lp-glsl-frontend` from `lp-glsl-filetests/Cargo.toml`.

### 2. `lp-glsl-builtins-gen-app` — inline types

The gen-app uses these from `lp-glsl-frontend`:

- `semantic::types::Type` — enum (~20 variants)
- `semantic::functions::FunctionSignature` — struct (name, return_type, parameters)
- `semantic::functions::Parameter` — struct (name, ty, qualifier)
- `semantic::functions::ParamQualifier` — enum (In, Out, InOut)
- `semantic::passes::function_signature::extract_function_signature` — function

**Action:** Create a new module `lp-glsl-builtins-gen-app/src/lpfx/types.rs`
with inlined versions of these types. Only include the variants/fields actually
used by the gen-app. The types are simple data structs with no complex logic.

```rust
// src/lpfx/type

/// GLSL type (subset needed for builtin signature parsing)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type {
    Void,
    Bool,
    Int,
    UInt,
    Float,
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
    Mat2,
    Mat3,
    Mat4,
}

#[derive(Debug, Clone)]
pub struct FunctionSignature {
    pub name: String,
    pub return_type: Type,
    pub parameters: Vec<Parameter>,
}

#[derive(Debug, Clone)]
pub struct Parameter {
    pub name: String,
    pub ty: Type,
    pub qualifier: ParamQualifier,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamQualifier {
    In,
    Out,
    InOut,
}
```

Then inline or rewrite `extract_function_signature`. Read the current
implementation in `lp-glsl-frontend/src/semantic/passes/function_signature.rs`
to understand what it does — it maps `glsl::syntax::FunctionPrototype` fields
to `FunctionSignature`. The mapping is straightforward: extract name, map
`glsl` type specifiers to `Type` variants, extract parameters. Write a local
version in `types.rs` or `glsl_parse.rs`.

**Update imports** in:

- `src/main.rs` — `use lp_glsl_frontend::semantic::types::Type` →
  `use crate::lpfx::types::Type`
- `src/lpfx/glsl_parse.rs` — replace `lp_glsl_frontend` imports with local
- `src/lpfx/validate.rs` — same
- `src/lpfx/generate.rs` — same

Drop `lp-glsl-frontend` from `lp-glsl-builtins-gen-app/Cargo.toml`.

### 3. Remove old generation paths from gen-app `main.rs`

Delete or comment out these generation calls and their functions:

- `generate_registry(...)` — writes into `lp-glsl-cranelift/` (deleted)
- `generate_testcase_mapping(...)` — writes into `lp-glsl-cranelift/` (deleted)
- `generate_lpfx_fns_file(...)` — writes into `lp-glsl-frontend/` (deleted)

Remove `registry_path`, `mapping_rs_path`, `lpfx_fns_path` from the
`format_generated_files` call.

Delete the corresponding `generate_*` function bodies at the bottom of
`main.rs`.

### 4. Delete `lp-glsl-frontend`

```bash
rm -rf lp-shader/lp-glsl-frontend
```

Remove from root `Cargo.toml`:

- `[workspace] members`: `"lp-shader/lp-glsl-frontend"`
- `[workspace] default-members`: `"lp-shader/lp-glsl-frontend"`

### 5. Also check: `glsl` crate dependency

`lp-glsl-frontend` depended on the `glsl` crate (Rust GLSL parser, git dep).
Other crates still use `glsl` directly:

- `lp-glsl-filetests` (via `Cargo.toml`)
- `lp-glsl-builtins-gen-app` (via `Cargo.toml`)

So the `glsl` workspace dependency stays. Just verify no workspace-level
`[patch]` or `[dependencies]` entry exists solely for `lp-glsl-frontend`.

## Validate

```bash
cargo check -p lp-glsl-builtins-gen-app
cargo check -p lp-glsl-filetests
cargo run -p lp-glsl-builtins-gen-app   # verify generation still works
cargo test -p lp-glsl-filetests -- test_glsl   # verify filetest GLSL parsing
```
