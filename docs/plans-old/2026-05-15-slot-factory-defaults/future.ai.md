# Future Work

## Generated Codec Deserialization Replacement

- **Idea:** Replace generated per-record codec read functions with a generic
  default-and-mutate reader built on `registry.create_default`.
- **Why not now:** This plan only adds the factory/default primitive and proves
  it with a vertical slice.
- **Useful context:** `lp-core/lpc-slot-codegen/src/render/slot_codecs.rs` and
  `lp-core/lpc-slot-mockup/src/tests/generated_shape_codec.rs`.

## Runtime Insert Remove Wire Operations

- **Idea:** Add explicit wire ops for map insert/remove and option presence
  changes.
- **Why not now:** The current milestone is about construction/read semantics,
  not client mutation protocol expansion.
- **Useful context:** Keep `set_slot_value` conservative so typos do not create
  map keys.

## Typed Downcast For Created Objects

- **Idea:** Provide a safe way for callers that know the expected type to recover
  `Box<T>` from a `Box<dyn SlotMutAccess>`.
- **Why not now:** Generic readers only need slot access. Typed loading can
  still construct `T::default()` directly.
- **Useful context:** Consider an `Any`-like hook only if host/std/no_std
  constraints are clear.

## Binary Size Metrics

- **Idea:** Compare generated codec size before/after default-and-mutate
  adoption.
- **Why not now:** The factory primitive must exist before measuring the real
  replacement.
- **Useful context:** User specifically wants to minimize monomorphs and reduce
  generated serde/codec code size.
