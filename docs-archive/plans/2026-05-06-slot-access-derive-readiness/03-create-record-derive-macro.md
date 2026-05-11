# Phase 3: Create Record Derive Macro

## Scope Of Phase

Add the proc-macro crate and implement the first `SlotRecord` derive for explicit record field annotations.

In scope:

- Add workspace member `lp-core/lpc-slot-macros`.
- Add an optional `derive` feature on `lpc-model`.
- Re-export the derive macro from `lpc-model` when `derive` is enabled.
- Implement `#[derive(SlotRecord)]`.
- Support `#[slot(shape_id = "...")]` root records.
- Support explicit field annotations for value, leaf, record, map ref, option ref, and skipped fields if needed.
- Add macro tests through `lpc-slot-mockup` or macro crate tests.

Out of scope:

- Enum derive.
- Type inference from semantic slot aliases.
- Converting complex mockup records.
- Real `lpc-source` / `lpc-engine` conversion.

## Code Organization Reminders

- Keep macro parsing and codegen in separate files if it improves readability.
- Generated code should use fully qualified `::lpc_model::...` paths.
- Generated code must be compatible with downstream `no_std` crates.
- Keep tests at the bottom of files.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Suggested crate:

```text
lp-core/lpc-slot-macros/
  Cargo.toml
  src/
    lib.rs
    attr.rs
    record.rs
```

Suggested derive:

```rust
#[derive(SlotRecord)]
#[slot(shape_id = "source.texture")]
pub struct TextureDef {
    #[slot(leaf = dim2u_shape())]
    size: Dim2uSlot,
}
```

Generated impls:

- `impl ::lpc_model::SlotRecordShape for Type`
- `impl ::lpc_model::SlotRecordAccess for Type`
- with `shape_id`, also:
  - `impl ::lpc_model::SlotAccess for Type`
  - `impl ::lpc_model::StaticSlotAccess for Type`

Initial field annotations:

- `#[slot(value = ModelType::String)]`
- `#[slot(leaf = source_path_shape())]`
- `#[slot(record)]`
- `#[slot(map(key = "string", value_ref = "source.shader_param_def"))]`
- `#[slot(option_ref = "source.scalar_hint")]`

If useful, also support:

- `#[slot(skip)]` for fields that should not be part of the slot record.

## Validate

```bash
cargo test -p lpc-model --features derive
cargo check -p lpc-model --no-default-features
cargo check -p lpc-model --features schema-gen,derive
cargo test -p lpc-slot-mockup
git diff --check
```
