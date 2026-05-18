# M2 Design: Codegen In The Mockup

## Scope Of Work

Build a generated slot-native codec slice in the mockup crate.

In scope:

- Add shared codec helper APIs where M1.1 manual code was repetitive.
- Generate typed read/write functions for a representative mockup bundle.
- Keep the generated output small and helper-driven.
- Use the M1.1 manual tests as behavioral acceptance tests.
- Document codegen rough points before broader adoption.

Out of scope:

- Production loader/message replacement.
- Removing Serde from core model crates.
- Full derive support for every slot shape.
- Deep enum wrapper support beyond the one-level variants needed by the mockup.

## File Structure

```text
lp-core/lpc-model/src/slot_codec/
  ... existing codec files
  record_codec.rs                 shared compact record/map/option helpers, if useful

lp-core/lpc-slot-codegen/src/
  lib.rs                          extend build-script generation entry points

lp-core/lpc-slot-mockup/
  build.rs                        generate mock slot codec module
  src/
    generated_slot_codec.rs       include!(OUT_DIR generated file) or module wrapper
    tests/
      generated_shape_codec.rs    generated-code acceptance tests
      manual_shape_codec.rs       stays as reference/contract

docs/roadmaps/2026-05-13-slot-native-streaming-serialization/
  m2-codegen-mockup/
    00-notes.md
    00-design.md
    01-shared-helper-shape.md
    02-generated-test-bundle.md
    03-real-mockup-root-slice.md
    04-cleanup-validation.md
    summary.md
```

## Architecture Summary

M2 should avoid large bespoke generated scanners. The target shape is:

```text
generated metadata/adapters
        │
        ▼
shared slot_codec helpers
        │
        ├── SlotReader + JsonSyntaxSource / TomlSyntaxSource
        └── SlotJsonWriter
```

Generated code may still contain per-type field dispatch, but common behavior
should live in helpers:

- required field diagnostics
- unknown field diagnostics
- discriminator validation
- unit variant finish
- fixed-size arrays
- map scanning/writing
- option/default policy

## Code Size Strategy

For the first generated slice, favor this shape:

- Generated `const` field-name arrays for diagnostics.
- Generated small per-type `read_*` and `write_*` functions.
- Shared helpers for maps, arrays, options, and enum discriminators.
- Avoid generating a full custom `while next_prop` body when a compact helper
  can own the loop.

Do not optimize blindly. If generated helper indirection becomes awkward or
causes generic explosion, record it and narrow the helper.

## Acceptance Tests

Generated code should pass equivalents of:

- JSON round-trip for the representative source bundle.
- TOML read through the same generated reader.
- unknown field diagnostics.
- invalid discriminator diagnostics.
- missing required field diagnostics.

The manual M1.1 test remains useful as a reference.
