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

- Unit enum variants need an explicit body finish step after reading `kind`.
  Forgetting this leaves the closing object token in the stream and makes the
  parent record terminate early.
- Field enum readers naturally receive a `ValueReader`, while
  `expect_discriminator` currently lives on `SlotReader`. The manual slice uses
  an `ObjectReader` helper instead. M2 codegen should either get an
  object-level discriminator helper or a first-class enum reader helper.
- Fixed-size array helpers in the manual test are enough for valid fixtures, but
  M2 should add friendly length errors before relying on generated code for
  authored data.

## Reader/Writer API Changes Before M2

- Added `SlotReader::missing_required_field`.
- Added `ObjectReader::missing_required_field`.
- Added `ObjectReader::invalid_discriminator_value`.

Recommended follow-up before or during M2:

- Add `ObjectReader::expect_discriminator("kind", expected)` that consumes the
  first field and validates the value.
- Consider making `ObjectReader::finish()` consume and validate the object end
  instead of being a no-op, or add an explicit `finish_empty_variant()` helper.

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
