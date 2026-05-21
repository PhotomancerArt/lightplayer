# Phase 2: Use Tools

## Goal

Apply the shared collection types only where they reduce measured memory
pressure or remove clearly wasteful embedded allocation shapes.

Use [LightPlayer Collection Usage Guide](lightplayer-usage-guide.md) as the
starting map for choosing candidate migrations after a fresh profile pass.

## Initial Application Order

1. Slot shape and generated model metadata: prefer `TinyVec`, `FlatMap`, and
   `StaticMap` where field counts are small and deterministic.
2. Runtime dense side tables: prefer `DenseIdMap`/`DenseIdSet` when ids are
   compact and already newtyped.
3. Parse/lowering phase scratch: prefer `ChunkedArena` for append-only nodes and
   `TinyChunkedVec` for small lists with uncommon growth.
4. Build-once registries: prefer `FrozenMap` when data is assembled during load
   and then queried without mutation.
5. Existing broad `Vec`/`BTreeMap` surfaces: narrow APIs first, then change
   representation behind those APIs.

## Rules Of Thumb

- Use `TinyVec` only with small inline capacities; object size still matters.
- Use `FlatMap` for tiny maps with low mutation and deterministic iteration.
- Use `DenseIdMap` only after confirming ids are dense enough.
- Use `FrozenMap`/`StaticMap` for read-mostly or immutable tables.
- Use chunked storage when the risk is contiguous allocation failure, not as a
  universal replacement for ordinary vectors.

## Validation

Each migration should include a before/after note tied to profile data or an
inventory finding, plus the narrowest relevant host and RV32 checks from
`AGENTS.md`.
