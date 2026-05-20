# Embedded Collections Memory Roadmap

## Motivation And Rationale

ESP32 memory pressure is now high enough that button-sign can fail before the
system reaches the work we actually care about: on-device GLSL compilation and
runtime execution. The March work proved that collection shape matters; the May
profiles show that the next pressure points are broader than shader codegen.
Project load, slot shape construction, runtime indexes, small maps, parse
buffers, and reload lifetime overlap all need more embedded-shaped storage.

This roadmap has two phases:

1. Build reusable `no_std + alloc` collection primitives in `lp-common`.
2. Apply them narrowly in this repo where profiles and inventories show real
   pressure.

The goal is not to replace every `Vec` or `BTreeMap`. The goal is to give the
codebase a shared vocabulary for the shapes embedded systems need:

```text
tiny data        -> inline-first collections
dense ids        -> direct indexed maps/sets
build-once data  -> frozen/static tables
phase scratch    -> resettable arenas and pools
fragmented heap  -> chunked growth when contiguous realloc is risky
```

## Architecture And Design

The collection layer should live in `lp-common/lp-collection`, stay
dependency-light, and remain usable by `lpc-model`, `lpc-engine`,
`lps-glsl`, `lpir`, `lpvm-native`, and firmware crates.

Phase 1 primitives:

- `TinyVec<T, const N: usize>` and `TinyChunkedVec<T, const N: usize>` for the
  many short lists that currently allocate.
- `FlatMap<K, V, const N: usize>` for tiny deterministic maps that do not need a
  full `BTreeMap` node tree.
- `DenseIdMap<Id, V>` and `DenseIdSet<Id>` for compact newtype-indexed storage.
- `FrozenMap<K, V>` and `StaticMap<K, V>` for build-once or ROM-backed tables.
- `ChunkedArena<T, Id>` for typed, phase-local push arenas with stable ids.
- `ScratchPool` patterns only where a measured phase has reusable temporary
  buffers.

Phase 2 application should start where it can pay back quickly:

```text
lp_collection
  TinyVec / FlatMap / DenseIdMap / FrozenMap / ChunkedArena
        |
        +-- lpc-slot-macros -> inline slot shape fields
        +-- lpc-model       -> small map abstractions hidden behind APIs
        +-- lpc-engine      -> dense stores, compact binding/index tables
        +-- lps-glsl        -> HIR arena and lowering scratch
        +-- project-load    -> frozen/static registry and interned ids
```

`ChunkedVec` and `ChunkedHashMap` remain important, but they should be used for
fragmentation-sensitive growth, not as the default replacement for every
collection. Tiny, dense, frozen, and static data each deserve their own shape.

## Alternatives Considered

- Keep using `Vec`/`BTreeMap` everywhere and tune allocator behavior later.
  Rejected because profiles show collection shape and tiny allocation count are
  already material.
- Replace all maps with `ChunkedHashMap`. Rejected because many maps are tiny,
  ordered, dense-id keyed, or build-once; hashing is often the wrong storage
  model.
- Use one global bump allocator for compilation/load. Rejected as a general
  answer because many phases allocate and free; bump allocation can raise peak
  memory when destructors would otherwise return memory mid-phase.
- Host precompile or disable compiler pieces. Rejected because the product is
  on-device GLSL JIT compilation.

## Risks

- Public structs currently expose `BTreeMap` and `Vec` fields, especially in
  `lpc-model`. Some migrations require API narrowing before representation
  changes.
- Inline-first collections can increase stack/object size if used casually with
  large `N`.
- Dense-id maps waste memory when ids are sparse; each use needs a quick
  cardinality/sparsity check.
- Frozen maps improve resident memory but make live mutation boundaries more
  explicit.
- Generic helpers can become too clever. Favor simple APIs, measured use sites,
  and visible type aliases.

## Phases

- [Phase 1: Build Tools](phase-1-build-tools.md) creates and validates the
  collection datatypes in `lp-common`.
- [Phase 2: Use Tools](phase-2-use-tools.md) migrates profiled hot spots in this
  repo after the shared tools are ready.

## Scope Estimate

This is a medium-to-large hardening effort. Phase 1 is deliberately bounded and
can proceed in `lp-common` while this repo has other work in flight. Phase 2
should be split by measured pressure point so the deadline work can take the
highest-payoff migrations first.
