# Phase 1: Build Tools

## Goal

Create the shared embedded collection vocabulary in `lp-common/lp-collection`
before changing this repo's runtime/application code.

## Datatypes

- `TinyVec<T, N>` for inline-first small lists.
- `TinyChunkedVec<T, N>` for inline-first lists whose large case should avoid a
  large contiguous allocation.
- `FlatMap<K, V, N>` and `FlatSet<K, N>` for tiny sorted key collections.
- `DenseIdMap<Id, V>` and `DenseIdSet<Id>` for compact dense-id side tables.
- `FrozenMap<K, V>` and `StaticMap<K, V>` for build-once/static sorted lookup
  tables.
- `ChunkedArena<T, Id>` for append-only phase-local storage with stable typed
  ids.

## Validation

- Unit tests for inline-to-heap/chunked spill behavior, sorted lookup behavior,
  dense-id iteration, duplicate-key handling, and arena id stability.
- `cargo test -p lp-collection`.
- `cargo check -p lp-collection --target riscv32imac-unknown-none-elf`.

## Notes

This phase belongs in the common repo. This repo should only receive roadmap and
investigation updates until the tools are ready to apply.
