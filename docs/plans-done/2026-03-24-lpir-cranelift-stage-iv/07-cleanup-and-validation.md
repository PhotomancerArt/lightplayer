# Phase 7: Cleanup & validation

## Scope

- Grep for **TODO**, **FIXME**, **dbg!**, stray **`println!`**
- **`cargo +nightly fmt`** on touched crates
- **`cargo clippy -p lpir -p lp-glsl-naga -p lpir-cranelift -- -D warnings`**
- Full tests for the three crates
- **`summary.md`** in plan dir
- Move plan to **`docs/plans-done/`**
- Conventional commit

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### Commit message (example)

```
feat(lpir-cranelift): Stage IV API, GlslMetadata, call interfaces

- Glsl metadata types in lpir; lowering produces GlslModuleMeta
- Param qualifiers on FunctionInfo; LowerError::InFunction
- JitModule, CompileOptions, CompilerError; function names on codegen errors
- jit() and jit_from_ir_owned pipeline
- GlslQ32 and Level 1 call(); DirectCall Level 3
```

## Validate

```
cargo +nightly fmt -p lpir -p lp-glsl-naga -p lpir-cranelift
cargo clippy -p lpir -p lp-glsl-naga -p lpir-cranelift -- -D warnings
cargo test -p lpir -p lp-glsl-naga -p lpir-cranelift
```
