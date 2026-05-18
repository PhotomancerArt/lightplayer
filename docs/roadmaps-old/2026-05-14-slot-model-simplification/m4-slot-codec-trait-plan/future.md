# Future Work

## Generated Enum Codecs

- **Idea:** Generate `SlotCodec` impls for slot-modeled enums once enum metadata is stable.
- **Why not now:** The wrapper/discriminator shape is still being validated in the mockup.
- **Useful context:** Start from explicit impls for node definition wrappers, `MappingConfig`, `PathSpec`, and `BindingEndpoint`.

## Binary Codec Backend

- **Idea:** Add a compact binary `SyntaxEventSource`/writer pair using the same `SlotCodec` trait.
- **Why not now:** JSON/TOML need to prove the trait boundary first.
- **Useful context:** The reader/writer cursor contract is intentionally backend-neutral.

## Code Size Metrics

- **Idea:** Add before/after size measurements for serde-generated code versus generated `SlotCodec` impls.
- **Why not now:** The mockup must first be fully on `SlotCodec`.
- **Useful context:** This should become part of the adoption plan before removing serde from no_std core parts.

## Minimize Monomorphs Pass

- **Idea:** Audit generated slot and codec code for monomorphization-heavy patterns and move repeated logic behind shared helpers.
- **Why not now:** Premature until the trait shape stops moving.
- **Useful context:** Embedded binary size is the primary motivation for this work.
