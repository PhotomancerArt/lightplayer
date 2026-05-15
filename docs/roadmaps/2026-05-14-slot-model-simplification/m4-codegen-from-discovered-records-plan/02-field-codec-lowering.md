# Phase 2: Field Codec Lowering

## Scope Of Phase

Teach codegen how discovered field types map to generated reader/writer
fragments.

In scope:

- field lowering for the mockup source records
- shared lowering for common wrappers:
  - `ValueSlot<T>`
  - semantic slot aliases like `Dim2uSlot`, `Affine2dSlot`, `ColorOrderSlot`
  - `OptionSlot<T>`
  - `MapSlot<K, V>`
  - nested `SlotRecord` structs
- preserve current authored JSON/TOML shapes

Out of scope:

- generic support for every possible Rust type
- production domain adoption
- removing `from_codec` callers

## Code Organization Reminders

- Keep lowering rules explicit and testable.
- Do not create a broad Serde-like attribute language.
- If a helper is temporary or mockup-specific, name it that way.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-slot-codegen/src/lib.rs`
- `lp-core/lpc-slot-mockup/src/source/*.rs`
- `lp-core/lpc-model/src/slot_codec/*`

Expected changes:

- Add a field lowering representation, for example:

```rust
struct LoweredCodecField {
    wire_name: String,
    local_name: String,
    init_expr: String,
    read_assignment: String,
    write_expr: String,
    construct_expr: String,
}
```

- Generate direct slot-field assignments such as:
  - `Dim2uSlot::new(read_dim2u(prop.value())?)`
  - `Affine2dSlot::new(read_affine2d(prop.value())?)`
  - `ColorOrderSlot::new(read_color_order(prop.value())?)`
  - `OptionSlot::some(...)` / `OptionSlot::none()`
  - `MapSlot::new(prop.value().string_key_map(...)? )`
- Keep default initialization explicit:
  - from `Default` where available
  - from a narrow mockup policy where the current codec omits fields like
    `bindings` or `sampling`

Tests to add/update:

- Unit tests for lowering representative field types.
- Snapshot-like string tests are acceptable if they are focused and stable.

## Validate

```bash
cargo test -p lpc-slot-codegen
cargo test -p lpc-slot-mockup generated_shape_codec
```

