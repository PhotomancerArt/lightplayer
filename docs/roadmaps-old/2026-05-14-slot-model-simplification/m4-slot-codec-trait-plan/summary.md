# SlotCodec Trait Plan Summary

## What Was Built

- Added the `SlotCodec` trait as the type-owned slot serialization contract.
- Promoted format-neutral writer names (`SlotWriter`, `SlotValueWriter`, `SlotWrite`, etc.) while keeping JSON aliases for compatibility.
- Added `LpType`-driven `LpValue` read/write helpers.
- Implemented `SlotCodec` for `ValueSlot<T>`, `MapSlot<String, V>`, `MapSlot<u32, V>`, `OptionSlot<T>`, and `GlslOpts`.
- Generated `SlotCodec` impls for discovered mockup `SlotRecord` types.
- Cut real mockup root read/write functions over to generated record body helpers plus a small discriminator surface list.
- Removed `mockup_codec_policy()` and the old generated per-field policy machinery.
- Added explicit mockup `SlotCodec` impls for `MappingConfig` and `PathSpec`.
- Updated the generated fixture TOML test shape so `Affine2d` uses its `Mat3x3` slot value representation.

## Decisions For Future Reference

#### Cursor-Based Trait Boundary

- **Decision:** `SlotCodec` reads from `ValueReader` and writes to `SlotValueWriter`.
- **Why:** These are streaming cursors over one value, so the codec path avoids a materialized syntax tree.
- **Rejected alternatives:** Passing raw JSON/TOML into codecs; parsing into a generic tree first.

#### Field Types Own Codec Behavior

- **Decision:** Generated record code delegates fields to `SlotCodec`.
- **Why:** This is the serde-like shape we wanted: the generator discovers fields, while field/container/value types own behavior.
- **Rejected alternatives:** A mockup policy table with per-field read/write expressions.

#### Surface Policy Only

- **Decision:** Mockup-specific policy is now limited to root surfaces and discriminators.
- **Why:** Root `kind` handling is usage-level context, not field serialization behavior.
- **Rejected alternatives:** Keeping `mockup_codec_policy()` as a full shadow schema.

#### Affine2d Matrix Shape

- **Decision:** The generated mockup TOML fixture uses the `Mat3x3` value shape for `Affine2d`.
- **Why:** `Affine2d` now owns custom value semantics through `SlotValue`; the mock authored shape should follow that source of truth.
- **Rejected alternatives:** Preserving the older object-shaped `m00/m01/...` authored transform in generated codec tests.

## Validation

```bash
cargo fmt -p lpc-model -p lpc-wire -p lpc-slot-codegen -p lpc-slot-mockup
cargo test -p lpc-model
cargo test -p lpc-wire
cargo test -p lpc-slot-codegen
cargo test -p lpc-slot-mockup
cargo check -p lpc-model --no-default-features
```

All passed.

