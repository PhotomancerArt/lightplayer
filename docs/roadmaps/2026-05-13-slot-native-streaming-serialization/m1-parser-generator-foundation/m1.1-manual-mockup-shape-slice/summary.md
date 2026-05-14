# M1.1 Summary: Manual Mockup Shape Slice

## What Was Proven

- Added a manual mockup source bundle codec test that exercises multiple
  persisted roots and node definition variants.
- JSON writing uses `SlotJsonWriter`; JSON reading uses `JsonSyntaxSource`;
  TOML reading uses `TomlSyntaxSource`.
- JSON and TOML share the same hand-written typed reader functions.
- The manual slice covers:
  - roots and node invocations
  - discriminator-first node definition enums
  - nested records
  - string-key maps
  - numeric-key maps
  - present and absent options
  - unit and record enum variants
  - scalar leaves
  - fixed numeric arrays
  - vector/list values
  - binding definitions and `ref` / `value` endpoints
- Added negative tests for unknown fields, invalid discriminators, and missing
  required fields.

## What Still Feels Rough

- Map helpers are still hand-written in the mockup slice. This is acceptable
  for codegen, but handwritten code remains verbose.

## Reader/Writer API Changes Before M2

- Added `SlotReader::missing_required_field`.
- Added `ObjectReader::missing_required_field`.
- Added `ObjectReader::invalid_discriminator_value`.
- Added `ObjectReader::expect_discriminator("kind", expected)` so enum readers
  can validate the first property from object context.
- Made `ObjectReader::finish()` consume and validate the end of an unfinished
  object, which gives unit enum variants a safe generated-code target.
- Added `ValueReader::f32_array::<N>()` with friendly fixed-size length errors.
- Fixed array item path tracking so sibling items use stable paths such as
  `items[0]` and `items[1]` instead of accumulating segments.

Resolved rough edges:

- Unit variants now read `kind` and call `object.finish()?`.
- Enum readers now use `object.expect_discriminator(...)`.
- Fixed numeric arrays no longer use test-local indexing helpers.
- Nested array diagnostics now report stable item paths.

## Domain Shape Notes

- The manual fixture uses test-local structs instead of the real mockup source
  structs because many real fields are private and the goal was codegen-shape
  pressure, not source-model mutation.
- Binding endpoints are modeled as `{ ref = "..." }` and `{ value = ... }`,
  matching the user preference for lower-case single-value enum keys.
- M1.1 writes JSON only. TOML is still read from `toml::Value`, which matches
  the current plan that disk-authored TOML is small enough to parse into a value
  tree for now.

## Final Validation

```bash
cargo fmt
cargo test -p lpc-slot-mockup manual_shape_codec
cargo test -p lpc-slot-mockup native_stream
cargo test -p lpc-model slot_codec
cargo check -p lpc-model --no-default-features
cargo check -p lpc-wire --no-default-features
```
