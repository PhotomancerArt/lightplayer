# Future Work

## Explicit Stable Ids

- **Idea:** Add `#[slot_value(id = "...")]` and possibly `#[slot(shape_id = "...")]` only where stable migration demands it.
- **Why not now:** The current direction is moving fast with Rust-name-derived ids.
- **Useful context:** Default id is the Rust type name in one global namespace.

## Rename `WithRevision`

- **Idea:** Rename `WithRevision<T>` to `Revisioned<T>` or `TrackedValue<T>`.
- **Why not now:** The storage concept is not the main source of complexity; semantic leaf duplication is.
- **Useful context:** Keep it a struct, not a trait, because it stores the revision.

## Minimize Monomorphs

- **Idea:** Audit generated slot and codec code for monomorphization-driven binary growth.
- **Why not now:** First reshape the model; then measure and optimize.
- **Useful context:** Embedded code size is a primary motivator for the custom serialization project.

## Full Custom Slot Codec Adoption

- **Idea:** Use the new `SlotValue` model as the basis for the TOML/JSON custom codec and eventually remove serde from no-std core paths.
- **Why not now:** This plan only reshapes the leaf model that the codec will depend on.
- **Useful context:** Existing roadmap phases cover SlotCodec parser/writer/codegen work.
