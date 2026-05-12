# Phase 3: Build Codegen Helper

## Scope Of Phase

Add a small build-time helper crate that generates static slot shape bootstrap
code into `OUT_DIR`.

In scope:

- Add `lp-core/lpc-slot-codegen` as a workspace member.
- Implement root discovery for `#[derive(SlotRecord)]` + `#[slot(root)]`.
- Generate `register_all_static_slot_shapes`.
- Generate `ensure_static_slot_shape`.
- Add focused tests for discovery and generated output.

Out of scope:

- Applying generated code to `lpc-source`.
- Runtime dynamic shape registration.
- Committed generated files.
- Linker-section or inventory registration.

## Code Organization Reminders

- Keep the helper std-only and build-time only.
- Prefer simple string generation over clever macro output if it is easier to
  inspect.
- Keep file scanning and Rust-item parsing in separate helpers if the file grows.
- Tests belong at the bottom of files.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests.
- If blocked, stop and report.
- Report changed files, validation, and deviations.

## Implementation Details

Relevant files:

- root `Cargo.toml`
- new `lp-core/lpc-slot-codegen/Cargo.toml`
- new `lp-core/lpc-slot-codegen/src/lib.rs`

Suggested public API:

```rust
pub struct SlotShapeCodegenConfig {
    pub crate_root: PathBuf,
    pub out_file: PathBuf,
}

pub fn generate_slot_shapes(config: SlotShapeCodegenConfig) -> Result<(), Error>;
```

Discovery rules:

- Scan `src/**/*.rs`.
- Parse with `syn`.
- Find named structs with:
  - derive path ending in `SlotRecord`,
  - `#[slot(root)]`.
- Infer public type path:
  - `src/source/project_def.rs` + `ProjectDef` -> `crate::source::ProjectDef`
    by assuming concept files re-export headline types from the parent module.
  - `src/node/project/mod.rs` + `ProjectDef` ->
    `crate::node::project::ProjectDef`.
  - `src/lib.rs` + `Root` -> `crate::Root`.

Generated functions:

```rust
pub fn register_all_static_slot_shapes(
    registry: &mut ::lpc_model::SlotShapeRegistry,
) -> Result<(), ::lpc_model::SlotShapeRegistryError> {
    ensure_static_slot_shape(registry, <crate::path::Type as ::lpc_model::StaticSlotShape>::SHAPE_ID)?;
    Ok(())
}

pub fn ensure_static_slot_shape(
    registry: &mut ::lpc_model::SlotShapeRegistry,
    id: ::lpc_model::SlotShapeId,
) -> Result<bool, ::lpc_model::SlotShapeRegistryError> {
    // generated match over known roots
}
```

Lazy reference behavior:

- After registering a known shape, inspect that registered shape for
  `SlotShape::Ref` ids.
- Recursively call `ensure_static_slot_shape` for each referenced id.
- If a referenced id is not recognized by this generated module and not already
  registered, return a clear missing-reference error.

Tests:

- Use temp source trees to test path inference and discovery.
- Assert generated code contains expected type paths and function names.
- Keep generated-code compile testing for the mockup application phase.

## Validate

```bash
cargo fmt --package lpc-slot-codegen
cargo test -p lpc-slot-codegen
cargo clippy -p lpc-slot-codegen --all-targets -- -D warnings
```
