# Future Work

## Derive Slot Enum Codec Metadata

- **Idea:** Add an explicit derive or metadata system for slot enum/discriminator
  codec generation.
- **Why not now:** M4 can remove record shadow schemas while keeping enum
  handling explicit.
- **Useful context:** `MappingConfig`, `PathSpec`, and future `NodeDef` wrapper
  enums are the motivating cases.

## Move Authored TOML Codec To Model

- **Idea:** Move shape-driven authored TOML conversion from `lpc-wire` into
  `lpc-model` or a clearer slot codec module.
- **Why not now:** It is not required to remove `from_codec` and static record
  schemas.
- **Useful context:** Current file is
  `lp-core/lpc-wire/src/slot/authored_toml.rs`.

## Measure Generated Code Size

- **Idea:** Compare generated codec size and embedded binary impact before and
  after helper consolidation.
- **Why not now:** First make the mockup codegen model clean enough to measure.
- **Useful context:** Embedded code size is a primary motivation for the custom
  serializer.
