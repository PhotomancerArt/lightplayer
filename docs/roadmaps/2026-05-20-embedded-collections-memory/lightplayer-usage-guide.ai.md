# LightPlayer Collection Usage Guide

## Purpose

This document describes where the embedded collection types should be considered
throughout LightPlayer. It is intentionally not an implementation plan. Treat it
as a profiling guide and a representation-choice guide for future memory work.

The next migration pass should start from a fresh allocation profile. Older
profiles are useful for prioritization, but every code change should still name
the current pressure it addresses: resident bytes, temporary peak, allocation
count, allocator rounding overhead, or fragmentation risk.

## Selection Rules

Use the shape of the data before choosing a type:

| Data Shape | Preferred Type | Use When | Avoid When |
| --- | --- | --- | --- |
| Usually empty or tiny list | `TinyVec<T, N>` | Most instances fit in `N`, `T` is small, and contiguous fallback is acceptable. | `N` is speculative, `T` is large, or the list usually grows. |
| Tiny list with rare large fallback | `TinyChunkedVec<T, N>` | Most instances fit in `N`, but large fallback may happen under fragmented heap pressure. | Callers need a single slice after spill. |
| Large growable sequence | `ChunkedVec<T>` | Growth is real and one large contiguous buffer is risky. | The API needs `&[T]`, or the common case is tiny. |
| Tiny ordered map/set | `FlatMap<K, V, N>` / `FlatSet<K, N>` | Cardinality is small, deterministic iteration matters, mutation is not hot. | Keys are dense ids or the map grows large. |
| Dense id side table | `DenseIdMap<Id, V>` / `DenseIdSet<Id>` | Ids are locally allocated, compact, stable, and mostly contiguous. | Keys are sparse, external, or should compact on remove. |
| Mutable arbitrary-key map/set | `ChunkedHashMap<K, V>` / `ChunkedHashSet<K>` | Keys are not dense, order is not semantic, and the map may outgrow flat storage. | The table is tiny, dense-id keyed, or build-once. |
| Build-once lookup table | `FrozenMap<K, V>` | Data is assembled during setup and then read without mutation. | Entries change during normal operation. |
| Static lookup table | `StaticMap<'static, K, V>` | Data can live as a sorted static slice. | Data is runtime-authored or needs mutation. |
| Append-only phase graph | `ChunkedArena<T, Id>` | A phase allocates many objects, references by stable id, then drops/reset all. | Entries have independent lifetimes or need removal. |

Keep a normal `Vec` for byte buffers, JIT code buffers, wire payloads, image or
texture data, and APIs that fundamentally require contiguous slices. Keep
`BTreeMap` at public serialization boundaries until the API can hide the
internal representation.

## Project Load And Runtime Model

### Slot Shapes And Static Model Metadata

Likely collection shapes:

- `StaticMap` for generated static registries whose data can live in flash.
- `FrozenMap` for registries assembled at startup and then queried.
- `TinyVec` for owned shape fields, enum variants, custom refs, labels, and
  other small lists that currently allocate as `Vec`.
- `DenseIdMap<SlotShapeId, _>` only after confirming ids are dense enough; hash
  derived ids are not automatically dense.

Candidate surfaces:

- `lp-core/lpc-model/src/slot/static_slot_shape.rs`
- `lp-core/lpc-model/src/slot/slot_shape_registry.rs`
- `lp-core/lpc-model/src/slot/slot_shape.rs`
- generated code from `lp-core/lpc-slot-macros`

Guidance:

Static authored shapes should prefer borrowed/static descriptors for resident
runtime lookup. Owned `SlotShape` conversion should be a debug, serialization,
or cold-path operation. If public structs expose `Vec` today, introduce view or
iterator APIs before changing representation.

### Authored Project Definitions And Slot Data

Likely collection shapes:

- `FlatMap` for small authored maps where deterministic order matters.
- `FrozenMap` for project indexes that are built during load and queried after.
- `TinyVec` for small record fields, children, binding lists, and mutation
  batches.
- Keep `BTreeMap` for wire/TOML-facing structures until parsing and
  serialization boundaries are narrowed.

Candidate surfaces:

- `lp-core/lpc-model/src/slot/slot_data.rs`
- `lp-core/lpc-model/src/value/lp_value.rs`
- `lp-core/lpc-shared/src/project/builder.rs`
- shader and fixture authored definitions under `lp-core/lpc-model/src/nodes`

Guidance:

Do not replace all authored `BTreeMap`s mechanically. First separate external
document shape from runtime shape. The embedded runtime wants compact lookup and
iteration; file formats want stable readable structure.

### Node Tree And Artifact Store

Likely collection shapes:

- `DenseIdMap<NodeId, NodeEntry<_>>` or an equivalent dense storage for
  `NodeTree::nodes`.
- `TinyVec<NodeId, N>` for node children where fanout is usually small.
- `FrozenMap` for load-complete path and sibling indexes if they do not need
  normal runtime mutation.
- `ChunkedHashMap` for mutable path/location maps with arbitrary keys when
  freezing is not viable.
- `DenseIdMap<ArtifactId, ArtifactEntry>` for handle-keyed artifact storage.

Candidate surfaces:

- `lp-core/lpc-engine/src/node/node_tree.rs`
- `lp-core/lpc-engine/src/node/node_entry.rs`
- `lp-core/lpc-engine/src/artifact/artifact_store.rs`
- `lp-core/lpc-engine/src/engine/project_loader.rs`

Guidance:

The node id and artifact handle domains are controlled locally, so dense storage
is attractive. Path and location lookup are different: those keys are external
and should become frozen, flat, or chunked hash tables depending on mutation
requirements and measured cardinality.

### Binding And Demand Indexes

Likely collection shapes:

- `TinyVec<BindingRef, N>` for lists of bindings per target.
- `FlatMap` for small deterministic indexes keyed by node/slot/channel.
- `FrozenMap` for indexes rebuilt after project load or after rare structural
  edits.
- `DenseIdSet<NodeId>` for per-frame visited/ticked producer sets.

Candidate surfaces:

- `lp-core/lpc-engine/src/node/node_binding_index.rs`
- `lp-core/lpc-engine/src/engine/engine.rs`
- `lp-core/lpc-engine/src/dataflow/resolver`
- shader materialization and input binding code under `lp-core/lpc-engine/src/nodes/shader`

Guidance:

Binding indexes are a good place for phase-specific representations: mutable
while loading or editing, then frozen or compact during steady runtime. Per-frame
sets should avoid tree nodes when the key is `NodeId`.

### Runtime Buffers And Output Sinks

Likely collection shapes:

- `DenseIdMap<RuntimeBufferId, RuntimeBuffer>` and `DenseIdMap<RuntimeBufferId, NodeId>`
  for runtime buffer stores.
- `DenseIdMap<RuntimeBufferId, OutputSinkBinding>` when sink ids share the same
  compact domain.
- Keep plain `Vec<u8>` for payload bytes and texture/output backing buffers.

Candidate surfaces:

- `lp-core/lpc-engine/src/resources/buffer/runtime_buffer_store.rs`
- `lp-core/lpc-engine/src/resources/buffer/runtime_buffer.rs`
- `lp-core/lpc-engine/src/engine/engine_services.rs`
- `lp-core/lpc-shared/src/output/memory.rs`

Guidance:

Buffer contents are intentionally contiguous payloads; do not chunk them unless
a backend can consume segmented buffers. The indexes around those payloads are
the better collection target.

## Shader Frontend And IR

### GLSL HIR

Likely collection shapes:

- `ChunkedArena<HirExpr, ExprId>` and `ChunkedArena<HirPlace, PlaceId>` as a
  reusable form of the existing HIR arena pattern.
- Keep the current contiguous expression-list pool unless a segmented list-pool
  API is added, because callers currently need `&[ExprId]`.
- `TinyVec` for short temporary argument and lane lists.
- `FlatMap` or `FrozenMap` for small symbol tables once build/update phases are
  explicit.

Candidate surfaces:

- `lp-shader/lps-glsl/src/hir/arena.rs`
- `lp-shader/lps-glsl/src/hir.rs`
- `lp-shader/lps-glsl/src/hir/typeck.rs`
- `lp-shader/lps-glsl/src/lower.rs`

Guidance:

The current HIR arena is already conceptually right. The future work is to make
that pattern reusable and decide whether expression-list storage needs its own
range-pool abstraction.

### Naga Frontend Lowering

Likely collection shapes:

- `DenseIdMap` for Naga handles if their indices are compact and stable through
  the phase.
- `FlatMap` for small ordered maps such as import groups and temporary scans.
- `TinyVec` for short VReg/lane groups.
- `FrozenMap` for builtins/import tables after registration.

Candidate surfaces:

- `lp-shader/lps-frontend/src/lower.rs`
- `lp-shader/lps-frontend/src/lower_ctx.rs`
- `lp-shader/lps-frontend/src/lower_lpfn.rs`
- `lp-shader/lps-frontend/src/readonly_in_scan.rs`

Guidance:

Naga handle maps should not stay as tree maps if profiling shows them hot and
the handle index space is dense. Confirm density first. Some existing
`SmallVec` use is already the right shape; do not replace it unless unifying on
`TinyVec` has a concrete benefit.

### LPIR

Likely collection shapes:

- Keep `ChunkedVec<LpirOp>` for `LpirBody`.
- `DenseIdMap<FuncId, IrFunction>` if function ids are dense enough and sorted
  function order can be preserved without a tree.
- `TinyVec` for parameter and return type lists with low arity.
- `FlatSet` for validation sets where cardinality is small.

Candidate surfaces:

- `lp-shader/lpir/src/lpir_module.rs`
- `lp-shader/lpir/src/builder.rs`
- `lp-shader/lpir/src/validate.rs`
- `lp-shader/lpir/src/interp.rs`

Guidance:

LPIR already has one proven `ChunkedVec` win. The next pass should distinguish
semantic contiguous payloads from compiler side tables. Function maps and vreg
side tables are the likely dense-storage candidates.

### Native Backend

Likely collection shapes:

- Keep `Vec<u8>` for emitted machine code and final linked images.
- Consider `ChunkedVec<VInst>` or `ChunkedArena<VInst, _>` for intermediate
  instruction streams only if downstream code does not require slices.
- `TinyVec` for short register lists, call arguments, and small ABI metadata.
- `FrozenMap` for symbol maps after link/finalization.
- `DenseIdMap` for register, slot, and region side tables when ids are compact.

Candidate surfaces:

- `lp-shader/lpvm-native/src/lower.rs`
- `lp-shader/lpvm-native/src/emit.rs`
- `lp-shader/lpvm-native/src/link.rs`
- `lp-shader/lpvm-native/src/compile/module_job.rs`
- `lp-shader/lpvm-native/src/rt_jit/module.rs`

Guidance:

Do not segment final code buffers: executable/JIT buffers are naturally
contiguous. Target intermediate vectors and side tables first, and only after a
compile-phase profile shows they matter.

## Firmware, Hardware, And Server State

### Hardware Registries And Virtual Drivers

Likely collection shapes:

- `FlatMap`/`FlatSet` for small deterministic address/channel tables.
- `TinyVec` for endpoint lists, capabilities, aliases, and small drain buffers.
- `DenseIdSet` for compact channel ids if the id domain is dense.
- Keep `VecDeque` where queue semantics are essential.

Candidate surfaces:

- `lp-core/lpc-shared/src/hardware/hardware_registry.rs`
- `lp-core/lpc-shared/src/hardware/hardware_system.rs`
- `lp-core/lpc-shared/src/hardware/virtual_radio_driver.rs`
- `lp-core/lpc-shared/src/hardware/virtual_button_driver.rs`

Guidance:

Most hardware collections are small. Prefer flat or tiny shapes over hash/tree
maps unless the hardware abstraction truly needs arbitrary growth.

### Server And Project Manager

Likely collection shapes:

- `ChunkedHashMap` only if the embedded server manages enough arbitrary-key
  projects or changes for hash maps to matter.
- `FlatMap` for small project/change maps.
- `TinyVec` for per-tick message batches and change lists when bounded by
  protocol expectations.

Candidate surfaces:

- `lp-app/lpa-server/src/project_manager.rs`
- `lp-app/lpa-server/src/server.rs`

Guidance:

This area should wait for an embedded server-specific profile. Host convenience
paths and multi-project desktop behavior should not drive embedded collection
choices by themselves.

## What Not To Change First

- Do not replace public serialized `BTreeMap` fields before the runtime has a
  representation boundary.
- Do not replace contiguous byte/code/image buffers with chunked storage unless
  the consumer can operate on segments.
- Do not migrate debug-only parsers, filetest generators, or host tooling before
  embedded project-load and compile paths are under budget.
- Do not convert all temporary vectors to arenas. Arenas help when lifetimes
  align; they can increase peak memory when values could otherwise be dropped
  earlier.

## Profiling Checklist Before Migration

For each proposed migration, capture:

1. Cardinality distribution: min, max, typical length or map size.
2. Lifetime: resident, phase-local, per-frame, or temporary expression-level.
3. Mutation pattern: build-once, append-only, frequent insert/remove, or read-mostly.
4. Key shape: dense id, small ordered key set, arbitrary hash key, or external
   serialized key.
5. API constraint: does the caller need a contiguous slice, stable iteration, or
   serde-compatible public representation?
6. Memory target: allocation count, requested bytes, allocator-rounded bytes,
   peak free heap, or fragmentation risk.

Preferred validation loop:

```bash
cargo run -p lp-cli -- profile examples/basic --collect alloc --mode project-load
cargo run -p lp-cli -- profile examples/button-sign --collect alloc --mode project-load
cargo test -p fw-tests --test profile_alloc_emu
```

Use narrower crate tests for representation-only migrations, then run firmware
profile tests before treating a migration as a memory win.
