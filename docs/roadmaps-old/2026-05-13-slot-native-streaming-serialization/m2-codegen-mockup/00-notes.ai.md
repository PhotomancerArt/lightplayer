# M2 Notes: Codegen In The Mockup

## Scope Of Work

Generate slot-native typed readers and writers for representative mockup shapes,
using M1.1 manual tests as the behavioral contract.

This milestone is about proving the codegen approach, not adopting it in real
production loaders yet.

## Current State

M1/M1.1 provide:

- `lpc_model::slot_codec` with streaming JSON read, TOML value read, and JSON
  write.
- Manual mockup source-bundle tests in
  `lp-core/lpc-slot-mockup/src/tests/manual_shape_codec.rs`.
- Clean generated-code target APIs:
  - `value.object()?`
  - `object.expect_discriminator("kind", expected)?`
  - `object.next_prop()?`
  - `object.finish()?`
  - `object.missing_required_field("field")`
  - `value.f32_array::<N>()?`

Current codegen/macro infrastructure:

- `lp-core/lpc-slot-macros/src/record.rs` derives slot shape/access impls for
  named structs.
- `lp-core/lpc-slot-codegen/src/lib.rs` scans source files in build scripts and
  generates static slot-shape/bootstrap modules.
- `lpc-slot-mockup` already has a build script generating `slot_shapes.rs`.

## Code Size Constraint

Binary size is a leading motivation for custom serialization. M2 must avoid
simply replacing Serde bloat with custom generated bloat.

Guiding principle:

- Generate small per-type adapters and metadata.
- Centralize record scanning, discriminator validation, option policy, map
  loops, fixed array reads, and common diagnostics in shared helpers.
- Prefer a compact helper call over repeated bespoke control flow, even if the
  generated source looks slightly less direct.
- Watch generic monomorphization. Do not introduce deeply generic helper stacks
  without a reason.

## User Notes

- The M1.1 tests are the contract.
- Generated code does not need to look exactly like hand-written code.
- It should preserve behavior and errors while staying compact.
- Err toward more abstract helpers.

## Open Questions

### Q1. Derive macro or build-script codegen first?

Suggested answer: start with build-script codegen for test-local mockup shapes.
It is easier to inspect and iterate without changing the public `SlotRecord`
derive contract. Once the compact shape is validated, move the proven pattern
into derive/build generation where it belongs.

### Q2. Should M2 generate for real mockup source structs immediately?

Suggested answer: begin with a generated equivalent of the M1.1 test-local
bundle. That isolates codegen mechanics from private fields and real model
constructor friction. Then add one real mockup source root after the generated
shape is stable.

### Q3. What counts as success for M2?

Suggested answer: generated readers/writers replace the M1.1 manual functions
for a representative bundle, with the same tests passing and no large repeated
per-type scanner loops beyond small field dispatch/adapters.
