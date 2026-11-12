#### Shape Before Type

- **Decision:** Choose collection storage by data shape: tiny, dense, frozen, phase-local, or fragmentation-sensitive.
- **Why:** `Vec`/`BTreeMap`/`ChunkedHashMap` each solve only one part of the embedded memory problem.
- **Rejected alternatives:** Replace every map with `ChunkedHashMap`; keep host-shaped collections everywhere.
- **Revisit when:** Profiles show a new dominant pattern.

#### Inline First For Small Lists

- **Decision:** Add `TinyVec` and use it for proven-small lists before broader vector rewrites.
- **Why:** Project-load profiles show small vector allocation hotspots, especially slot shape fields.
- **Rejected alternatives:** Pre-size every `Vec`; use `ChunkedVec` for tiny lists.
- **Revisit when:** Inline capacities inflate resident object size.

#### Flat Maps For Tiny Ordered Data

- **Decision:** Use flat sorted maps for tiny deterministic maps.
- **Why:** They preserve order without BTree node allocations and are simpler than hashing for small cardinalities.
- **Rejected alternatives:** Hashing all small maps; keeping public `BTreeMap` fields forever.
- **Revisit when:** Map sizes grow beyond the inline threshold.

#### Dense IDs Need Dense Storage

- **Decision:** Use explicit dense-id helpers for handle/arena-indexed storage.
- **Why:** Many IDs are compact newtypes where tree/hash lookup is unnecessary overhead.
- **Rejected alternatives:** Treat all numeric keys as maps; hand-roll `Vec<Option<T>>` at each site.
- **Revisit when:** An ID domain proves sparse or externally keyed.

#### Frozen Beats Mutable After Load

- **Decision:** Build mutable tables only while loading/initializing, then freeze where runtime mutation is not needed.
- **Why:** Frozen slices reduce resident heap churn and make lookup memory predictable.
- **Rejected alternatives:** Keep all project indexes mutable; precompute host-side bytecode.
- **Revisit when:** Live editing requires in-place mutation.

#### Phase Arenas Stay Explicit

- **Decision:** Use typed/resettable arenas for explicit compiler/load phases, not as a hidden global allocator.
- **Why:** Phase ownership gives memory control without losing destructor-based cleanup elsewhere.
- **Rejected alternatives:** Global bump allocator; arena-converting runtime VM memory managers.
- **Revisit when:** Profiles show allocator churn inside a well-bounded phase.

