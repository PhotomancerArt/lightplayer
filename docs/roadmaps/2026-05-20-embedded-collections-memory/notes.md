# Embedded Collections Memory Investigation

## Scope

Investigate whether the existing `lp_collection` helpers should grow into a
broader embedded collection toolbox, and where those collection shapes would
actually relieve ESP32 memory pressure. This roadmap complements
`docs/roadmaps/2026-05-20-project-load-memory/`: that roadmap owns the
project-load memory campaign, while this one owns reusable collection and
scratch-storage primitives that can support project load, the GLSL frontend,
LPIR lowering, native JIT codegen, and firmware runtime tables.

## User Notes

- The button-sign project is currently failing on real ESP32 hardware because
  resident and peak heap usage are too high.
- The compiler must remain present and executable on-device. Do not solve memory
  pressure by gating out GLSL compile or JIT execution.
- The earlier `lp_collection` work was useful, but it was created before the
  project had a stronger embedded-storage vocabulary.
- This investigation should start from actual usage of `Vec`, `BTreeMap`, and
  arena-like patterns, then propose reusable embedded approaches.

## Existing Helpers

`/Users/yona/dev/photomancer/lp-common/lp-collection/src/chunked_vec.rs`

- `ChunkedVec<T>` bounds individual inner allocations and is effective for
  append-heavy storage where stable contiguous layout is not required.
- Current chunk cap is 64 elements. Older archived notes say dynamic chunk sizing
  produced the best March result, but the current source exposes a fixed cap;
  that mismatch should be clarified before relying on the archive as current
  behavior.
- API covers `push`, `resize`, `swap_remove`, indexing, iteration,
  `binary_search_by`, and `sort_by_key`.
- Important drawback: `sort_by_key` clones the whole sequence into a temporary
  `Vec`, so it is not suitable for memory-critical sorting unless replaced.

`/Users/yona/dev/photomancer/lp-common/lp-collection/src/chunked_hashmap.rs`

- `ChunkedHashMap<K, V>` is optimized for bounded allocation size and small-map
  behavior, not for high-performance hashing.
- It starts flat with linear scan up to 16 entries, then migrates to 12 fixed
  buckets backed by `ChunkedVec`.
- `with_capacity` is currently a no-op.
- This shape is good for low-fragmentation, moderate-size mutable maps, but it
  is not the default answer for tiny maps, dense-id maps, build-once maps, or
  static registries.

Current workspace use of `lp_collection` is narrow:

- `lp-shader/lpir/src/lpir_module.rs` uses `ChunkedVec<LpirOp>` for `LpirBody`.
- `lp-shader/lps-glsl/src/hir/arena.rs` uses `ChunkedVec` for HIR expressions
  and places, while expression lists still use a plain `Vec<ExprId>`.
- Cranelift/regalloc forks also use the common helper per workspace dependency
  comments.

## Profile Evidence

March compile memory notes:

- `docs/reports/2026-03-10-heap-peak-analysis.md` measured 232,140 bytes live at
  peak during shader compilation, with 95,540 bytes free from a 320 KB heap.
- That peak was dominated by Cranelift IR/codegen, GLSL frontend cloning, and
  Vec/HashMap growth.
- `ChunkedVec` and `ChunkedHashMap` were working as intended: about 22 KB in 90
  live allocations, with bounded allocation sizes.
- `docs-archive/optimizations/00-index.md` records material prior wins:
  `ChunkedVec` tuning, direct q32, AST early free, and static LPFX registry work.

May project-load memory notes:

- `docs/roadmaps/2026-05-20-project-load-memory/m1-load-only-memory-instrumentation-results.md`
  records clean load-only traces.
- `examples/basic`: 87,578 captured live bytes after project load.
- `examples/button-sign`: 110,919 captured live bytes after project load.
- Top current load-only signals are TOML parsing, `toml::Value`, `BTreeMap`
  leaves, `RawVecInner::try_allocate_in`, slot shape field `Vec` creation, and
  binding/shader slot definition maps.

Hardware traces:

- Multiple `traces/2026-05-20T0*--esp32c6--demo-button-sign` runs OOM in
  `project reload: load core project`, with failures for requests around
  576-4096 bytes while free heap is already near zero.
- Successful traces still show project load/reload reducing free heap sharply.
  A representative success shows load beginning around 233 KB free and later
  heartbeat memory around 80,824 free / 223,176 used.
- This strongly suggests that collection work should not focus only on the old
  shader-compile peak. The current pressure point is the loaded project
  representation and reload lifetime overlap.

Allocator accounting:

- `docs/reports/2026-03-12-allocation-overhead-analysis.md` notes that RV32
  `linked_list_allocator` rounds tiny allocations to a 16-byte minimum. Trace
  bytes are therefore a lower bound, especially for tiny strings, vectors, and
  BTree nodes.

## Inventory Summary

### Vec Patterns

- Already-good chunked append stream: `LpirBody` in
  `lp-shader/lpir/src/lpir_module.rs`.
- Append-heavy compiler state: `lp-shader/lpir/src/builder.rs` has vreg, slot,
  body, pool, and block-stack vectors.
- Project loader staging: `lp-core/lpc-engine/src/engine/project_loader.rs`
  accumulates `loaded_nodes`, consumed/produced slot names, playlist runtime
  entries, trigger sources, and shader source strings.
- Slot-shape construction: slotted derive macros generate
  `Vec::from([SlotFieldShape; N])`, and profiles show this as
  `Vec<SlotFieldShape>::from` hotspots.
- Runtime tiny lists: node children, binding sets, binding index value lists,
  output fields, demand roots, and validation stacks are usually small.
- Nested Vecs are rare in production paths; the notable production-shaped
  candidates can usually become flat buffers plus ranges.

### Map And Set Patterns

- `BTreeMap` is widespread in model/runtime code because it gives deterministic
  order and serde-friendly external representation.
- Current hot project-load map signals include `BindingDefs`, shader slot maps,
  slot shape registry maps, and TOML parse/value BTree nodes.
- Dense ID maps exist or are implied around `NodeId`, `RuntimeBufferId`,
  `ArtifactId`, `SlotShapeId`, LPIR function IDs, and naga handles. These are
  often better as `Vec<Option<T>>`, `ChunkedVec<Option<T>>`, or frozen indexed
  slices than as BTree/Hash maps.
- Symbol/name maps in `lps-frontend`, `lpvm-native`, and ELF/linker code may
  still need ordered iteration, but many are build-once and could be sorted
  vectors or frozen maps.
- Tiny firmware maps/sets exist in ESP-NOW radio state, output channel state,
  flash FS change tracking, validation duplicate sets, and small runtime
  registries.

### Arena And Lifetime Patterns

- No direct source use of `typed_arena` or `bumpalo` was found.
- `lps-glsl` already has a HIR arena with `ExprId`/`PlaceId` over
  `ChunkedVec`; it is the strongest candidate for a reusable typed arena shape.
- Parsed AST nodes use recursive `Box<ParsedExpr>` and owned `Vec`s with a
  parse/compile lifetime. That is a candidate for a typed arena or bump-like
  parse arena, though it is a bigger refactor.
- Lowering context maps and scratch vectors are per-function/per-phase and
  could be resettable scratch storage rather than repeatedly rebuilt.
- Runtime VM memory arenas are deliberate runtime allocators and should not be
  conflated with compiler scratch arenas.

## Proposed Collection Primitives

### `TinyVec<T, const N: usize>`

- Inline storage for up to `N` elements, promoting to `ChunkedVec<T>` or `Vec<T>`
  when it outgrows inline storage.
- Use for slot shape fields, node children, binding sets, binding-index value
  lists, validation stacks, shader consumed/produced slot names, and small
  project-loader staging lists.
- Preferred promotion target should be configurable by type alias:
  `TinyVec` for contiguous fallback and `TinyChunkedVec` for fragmentation-risk
  fallback.
- This is the most broadly useful new helper because many profile hotspots are
  small vectors that allocate because `Vec::from([..])` has no inline storage.

### `FlatMap<K, V, const N: usize>`

- Inline sorted key/value entries up to `N`, with binary search lookup and stable
  deterministic iteration.
- Optional promotion target can be `ChunkedHashMap` for larger mutable maps or
  `ChunkedVec<(K, V)>` kept sorted for ordered maps.
- Use for `MapSlot`-like small maps, `BindingDefs`, shader slot defs,
  validation sets, firmware channel tables, and small symbol scopes.
- This should be the default alternative to `BTreeMap` when cardinality is
  small and ordering matters.

### `DenseIdMap<Id, V>` And `DenseIdSet<Id>`

- Newtype-indexed storage over `Vec<Option<V>>`, `ChunkedVec<Option<V>>`, or a
  frozen slice.
- Requires an `IdIndex` trait implemented by local ID newtypes, e.g. `NodeId`,
  `RuntimeBufferId`, `ArtifactId`, LPIR IDs, and wrappers around naga handles.
- Use for runtime buffer stores, artifact handle stores, slot shape registry
  overlays, lowering handle maps, readonly scans, and binding references where
  the key domain is dense enough.
- This should not replace path/name lookup maps by itself; pair it with an
  intern table or a small name map when external lookup is needed.

### `FrozenMap<K, V>`

- Build mutable, then sort/deduplicate and freeze to a compact slice or boxed
  slice.
- Lookup is binary search; iteration is stable; no allocator churn after freeze.
- Use for static slot shape registry data, builtins, module signature tables,
  node kind tables, symbol tables after linking, and project graph indexes after
  load.
- A `StaticMap<K, V>` variant over `&'static [(K, V)]` gives the flash/ROM
  storage shape used successfully by the LPFX static registry work.

### `TypedArena<T, Id>` / `ChunkedArena<T, Id>`

- Push-only typed arena over `ChunkedVec<T>` returning compact IDs.
- Supports `clear_reuse`, `len`, indexing, iteration, and optional list-pool
  helpers for storing ranges into side buffers.
- Use first for `HirArena` expression/place/list storage, then consider AST
  expression allocation and LPIR/lowering scratch once measurements justify it.
- Avoid global bump allocation for code paths with heavy free/reuse churn; this
  helper should be phase-local and explicit.

### `ScratchPool`

- A small owner object for reusable per-load/per-compile scratch buffers:
  tiny vectors, flat maps, string/path scratch, and typed arenas.
- Use where a phase currently allocates many temporary vectors and maps but can
  reset after each function/node/project child.
- Candidate owners: GLSL frontend compile session, `ProjectLoader`, LPIR
  builder/lowerer, and native backend lowering.

### `InternPool` / `SymbolId`

- Deduplicate repeated names and paths into compact IDs.
- Use with frozen or dense maps for `TreePath`, artifact paths, slot names,
  channel names, shader slot keys, and binding keys.
- This overlaps with project-load M4 and should probably be designed there,
  with `lp_collection` providing only the generic pool/map pieces.

## Recommended Application Order

1. Do not start by rewriting every `BTreeMap`. Start with profile-backed
   resident project-load structures.
2. Implement `TinyVec` and use it for slot shape fields generated by
   `lpc-slot-macros`. This attacks the `Vec<SlotFieldShape>::from` profile
   signal and creates a generally useful primitive.
3. Add `FlatMap`/`TinyMap` and apply it to small model/runtime maps where
   deterministic order is required and map size is known to be low.
4. Add `DenseIdMap` for ID-indexed engine/compiler maps, starting with isolated
   stores like runtime buffers, artifact handle maps, and frontend handle maps.
5. Add `ChunkedArena` by formalizing the existing HIR arena pattern.
6. Add `FrozenMap`/`StaticMap` when moving static registries and build-once
   project indexes out of live mutable maps.

## Non-Collection Approaches That Matter

- Project reload currently builds a new `Engine` while the old runtime remains
  resident until assignment. Hardware OOM traces point at this overlap. A
  deadline-oriented fix should either drop/take the old runtime before reload or
  load into a compact temporary representation before constructing the new
  runtime.
- TOML parsing and `toml::Value` trees are now major project-load costs. The
  long-term fix is direct/streaming artifact parsing or avoiding `toml::Value`
  where typed serde can be used.
- Static slot shapes and builtins should live in flash/static slices where
  possible; previous LPFX registry work proved this pattern.
- Profile reports should show allocator-rounded estimates, not only requested
  allocation sizes, before small-allocation optimizations are judged.

## Open Questions

1. Should `TinyVec` promote to plain `Vec` by default, with `TinyChunkedVec` as a
   separate alias, or should fragmentation safety be the default even if random
   access gets slightly slower?
   Suggested answer: provide both and use type aliases at call sites.

2. How much external serde compatibility must `MapSlot<K, V>` preserve if its
   internal storage stops being `BTreeMap<K, V>`?
   Suggested answer: preserve serialized order and public iterator behavior, but
   hide the internal storage behind methods before changing representation.

3. Should project reload favor memory safety or old-runtime preservation on load
   failure?
   Suggested answer for the deadline: favor memory safety on hardware and drop
   the old runtime before building the new one, with emulator tests documenting
   the behavior. Revisit transactional reload later.

4. Should the new helpers live in `lp-common/lp-collection` even when used by
   `lpc-model`?
   Suggested answer: yes, but keep them `no_std + alloc`, dependency-light, and
   generic. Domain-specific aliases can live beside the model/runtime code.

## Proposed Roadmap Overview For Review

Build an embedded collection layer around four storage shapes:

- Inline-first tiny collections for the many small vectors/maps.
- Dense-id collections for handle/index maps.
- Frozen/static maps for build-once and ROM-backed registries.
- Resettable typed arenas for parse/lower/codegen phases.

Use the current profile data to apply them narrowly: slot shape fields and small
runtime maps first, dense engine/frontend IDs second, then HIR/AST arenas and
frozen project indexes. Keep `ChunkedVec`/`ChunkedHashMap` as tools for
fragmentation-sensitive growth, but stop treating them as the universal answer.

