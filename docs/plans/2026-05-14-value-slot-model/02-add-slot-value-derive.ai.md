# Phase 2: Add `#[derive(SlotValue)]`

## Scope Of Phase

Add a derive macro for simple semantic leaf values.

In scope:

- Add `SlotValue` derive to `lpc-slot-macros`.
- Generate ids from Rust type names by default.
- Generate `ToLpValue`, `FromLpValue`, and `SlotValue`.
- Support a small editor attribute surface.
- Add macro tests for tuple newtypes and simple named-field structs.

Out of scope:

- Supporting every possible Rust type shape.
- Supporting explicit ids unless needed to unblock implementation.
- Workspace-wide duplicate detection.

## Code Organization Reminders

- Put the new derive in `lp-core/lpc-slot-macros/src/value.rs`.
- Keep shared attribute parsing helpers readable in `attr.rs`.
- Keep `record.rs` focused on `SlotRecord`.
- Update `lib.rs` docs after the macro works.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Update:

- `lp-core/lpc-slot-macros/src/lib.rs`
- `lp-core/lpc-slot-macros/src/attr.rs`
- add `lp-core/lpc-slot-macros/src/value.rs`
- relevant macro tests, likely under `lp-core/lpc-model/tests/` or macro crate tests depending current test style

Macro API:

```rust
#[derive(SlotValue)]
pub struct Ratio(pub f32);
```

Default generated id:

```rust
SlotShapeId::from_static_name("Ratio")
```

Initial supported Rust forms:

- tuple newtype with one public field:

  ```rust
  pub struct SourcePath(pub String);
  pub struct Ratio(pub f32);
  ```

- simple named-field struct where all fields can convert to/from `LpValue`:

  ```rust
  pub struct Dim2u {
      pub width: u32,
      pub height: u32,
  }
  ```

Initial attribute surface:

```rust
#[slot_value(editor = plain)]
#[slot_value(editor = path)]
#[slot_value(editor = node_ref)]
#[slot_value(editor = dimensions)]
#[slot_value(editor = affine2d)]
#[slot_value(editor = resource)]
#[slot_value(editor = runtime_buffer_resource)]
#[slot_value(editor = visual_product)]
#[slot_value(editor = control_product)]
#[slot_value(editor = slider(min = 0.0, max = 1.0, step = 0.01))]
#[slot_value(editor = number(min = 0.0, max = 10.0, step = 0.1))]
```

If an attribute parser gets too large, implement only what is needed for the first converted leaves and leave a clear TODO for the rest.

Type inference:

- `f32` -> `LpType::F32`
- `i32` -> `LpType::I32`
- `u32` -> `LpType::U32`
- `bool` -> `LpType::Bool`
- `String` -> `LpType::String`
- `[f32; 2]` -> `LpType::Vec2`
- `[f32; 3]` -> `LpType::Vec3`
- named struct -> `LpType::Struct { name: Some(type name), fields: ... }`

For tuple newtypes, the `LpType` is the wrapped field type.

For named structs, generated `ToLpValue` should emit `LpValue::Struct` with fields in declaration order. Generated `FromLpValue` should require the same struct name and field names. Unknown/missing fields are errors for now.

## Validate

```bash
cargo fmt
cargo test -p lpc-slot-macros
cargo test -p lpc-model slot_value
```
