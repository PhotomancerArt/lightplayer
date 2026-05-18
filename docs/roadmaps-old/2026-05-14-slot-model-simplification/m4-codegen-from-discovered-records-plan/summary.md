# Summary

## What Was Built

- Added a shared discovered slot record model in `lpc-slot-codegen` and reused it
  for slot view generation.
- Taught the mockup codec generator to construct slotted source records directly
  instead of calling codec-only domain constructors.
- Removed mockup `from_codec` / `*_from_codec` constructors from source records
  and enum helpers.
- Kept enum/discriminator handling explicit for this milestone.
- Added a codegen guard that generated mockup codec output does not depend on
  domain codec constructors.
- Added explicit `Affine2d` semantic conversion to `LpValue::Mat3x3` with
  fuzzy affine-row validation.
- Added authored TOML matrix support for `Mat2x2`, `Mat3x3`, and `Mat4x4`
  values.

## Decisions For Future Reference

#### Generated Code Owns Slot Wrapping

- **Decision:** Generated readers wrap decoded values into `ValueSlot`,
  `OptionSlot`, and `MapSlot` directly.
- **Why:** Mechanical slot wrapping is codec generation work, not domain model
  API surface.
- **Rejected alternatives:** Keeping `from_codec` constructors on source
  records.
- **Revisit when:** A domain constructor expresses real behavior beyond codec
  assembly.

#### Enum Codec Policy Stays Explicit

- **Decision:** M4 keeps enum/discriminator readers and writers explicit.
- **Why:** Records are now discovered, but `MappingConfig`/`PathSpec` still need
  a clearer slot enum metadata story before deriving their codec bodies.
- **Rejected alternatives:** Inferring complex enum behavior from ad hoc Rust
  enum parsing during this pass.
- **Revisit when:** We add explicit slot enum codec metadata or a derive.

#### Affine2d Uses Matrix Storage

- **Decision:** `Affine2d` is a custom semantic value stored as
  `LpValue::Mat3x3`.
- **Why:** It has mathematical semantics beyond a six-field Rust struct and can
  validate that incoming matrices are affine.
- **Rejected alternatives:** Relying on named-struct `SlotValue` derive for
  affine transforms.
- **Revisit when:** We introduce broader matrix/transform value conventions.

