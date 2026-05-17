## Derive-Emitted Codec Hooks

- **Idea:** Move private-field construction/access glue into the `SlotRecord`
  derive or module-local generated impls.
- **Why not now:** The mockup can validate metadata-driven root rendering with
  explicit hooks first.
- **Useful context:** Current mockup hooks live in
  `lp-core/lpc-slot-mockup/src/source/*.rs`.

## Metadata-Driven Specialized Helpers

- **Idea:** Generate helpers for leaf structs and slot enums such as `Dim2u`,
  `Affine2d`, `MappingConfig`, and `PathSpec`.
- **Why not now:** The source-root adapter generator is the next useful proof;
  helper inference has separate complexity around enum variants and semantic
  leaf parsing.
- **Useful context:** Explicit helpers currently live in generated
  `generated_slot_codec.rs` output from `lpc-slot-codegen`.
