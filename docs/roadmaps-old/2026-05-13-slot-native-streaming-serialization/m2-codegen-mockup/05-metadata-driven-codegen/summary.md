# Metadata-Driven Codegen Summary

## What was built

- Added a compact build-time `SlotCodec` metadata model in
  `lpc-slot-codegen`.
- Added a mockup source-root metadata table for `ProjectDef`, `OutputDef`,
  `TextureDef`, `FixtureDef`, and `ShaderDef`.
- Replaced the hardcoded generated source-root reader/writer section with
  renderers over that metadata table.
- Kept specialized semantic helpers explicit for shapes such as `Dim2u`,
  `MappingConfig`, `PathSpec`, `GlslOpts`, and `ShaderParamDef`.
- Added `lpc-slot-codegen` unit coverage for the source-root metadata table.
- Added durable slot design docs for the slot overview, slot values, and
  SlotCodec serialization rationale.

## Decisions for future reference

#### SlotCodec is build-time metadata

- **Decision:** `SlotCodecModule`, `SlotCodecRoot`, and `SlotCodecField` are
  generator metadata, not runtime slot data.
- **Why:** The generator needs a compact Serde-derive-like model without adding
  another runtime value tree.
- **Rejected alternatives:** Decode through a generic `SlotData` tree for all
  production paths; make the format parser target-aware.
- **Revisit when:** Production adoption needs dynamic runtime codecs that cannot
  be generated ahead of time.

#### Root adapters are generated, semantic helpers stay explicit

- **Decision:** Generate the five source-root adapters from metadata, but leave
  semantic leaf/enum helpers handwritten.
- **Why:** Root records have enough common shape now; semantic leaves still need
  policy decisions before broad inference makes sense.
- **Rejected alternatives:** Generate every helper in one pass; keep all root
  readers and writers as hardcoded template text.
- **Revisit when:** Two or more semantic helpers settle into the same metadata
  pattern.

#### Code size remains a measured constraint

- **Decision:** Continue leaning on shared helpers and record generated source
  size before production adoption.
- **Why:** Embedded code size is a leading motivation, so replacement code must
  be measured instead of assumed smaller.
- **Rejected alternatives:** Optimize generated code only after production
  migration.
- **Revisit when:** The minimize-the-monomorphs pass has concrete before/after
  firmware numbers.

## Validation

```bash
cargo fmt
cargo test -p lpc-slot-codegen
cargo test -p lpc-model slot_codec
cargo test -p lpc-slot-mockup generated_shape_codec
cargo test -p lpc-slot-mockup
cargo check -p lpc-model --no-default-features
cargo check -p lpc-wire --no-default-features
cargo check -p lpc-slot-mockup
```

All commands passed.

## Remaining rough edges

- The mockup source-root table still carries Rust expression strings for
  construction, reads, and writes. That is acceptable for this bridge, but
  derive-emitted or module-local codegen should eventually provide these hooks.
- `lpc-slot-codegen/src/lib.rs` now has a clear `SlotCodec` section, but the
  codegen crate could still be split into `slot_codec_model.rs` and
  `mockup_slot_codec.rs` once the next milestone starts.
- Generated mockup codec output is currently about 1264 lines. That includes
  fixture types and explicit semantic helpers, so the next size work should
  measure firmware output rather than judging only source line count.
