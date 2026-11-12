# Place-Based Aggregate Lowering Design

## Scope

This plan fixes `lps-glsl` aggregate place lowering so reads and writes through arrays, structs, fields, and swizzles remain proportional to the accessed leaf whenever the destination is statically or dynamically addressable.

The motivating blocker is `examples/fluid/compute.glsl`, where constant-index writes such as:

```glsl
emitters[0].pos = vec2(...);
```

currently lower by reading and rebuilding the whole `emitters[4]` aggregate. Constant-index paths are the first implementation priority, but dynamic memory indexing is part of the planned architecture:

```glsl
emitters[i].pos = vec2(...);
```

should lower to a computed address plus narrow stores when the root is memory-backed.

Out of scope:

- Disabling or feature-gating the on-device compiler.
- Host precompilation or bytecode upload as a workaround.
- Large rewrites of the older Naga frontend.
- Broad LPIR optimization passes that mask excessive frontend output after the fact.

## File Structure

Proposed target structure:

```text
lp-shader/lps-glsl/src/
  hir/
    place.rs
  lower/
    storage.rs
    place/
      mod.rs
      layout.rs
      path.rs
      read.rs
      write.rs
      dynamic.rs
    ops/
      place_read.rs
      place_write.rs
      place_project.rs
      index.rs
```

Existing files that remain relevant:

```text
lp-shader/lps-glsl/src/lower.rs
lp-shader/lps-glsl/src/lower/storage.rs
lp-shader/lps-glsl/src/lower/ops/place_read.rs
lp-shader/lps-glsl/src/lower/ops/place_write.rs
lp-shader/lps-glsl/src/lower/ops/place_project.rs
lp-shader/lps-glsl/src/lower/ops/index.rs
lp-shader/lps-glsl/src/hir/place.rs
lp-shader/lps-glsl/src/hir/shape.rs
lp-shader/lp-shader/src/tests.rs
```

Reference-only Naga frontend files:

```text
lp-shader/lps-frontend/src/lower_access.rs
lp-shader/lps-frontend/src/lower_aggregate_write.rs
lp-shader/lps-frontend/src/naga_util.rs
lp-shader/lps-frontend/src/lower_array.rs
lp-shader/lps-frontend/src/lower_ctx.rs
```

## Architecture Summary

`HirPlace` remains the semantic source of truth. It already records the root and the ordered path segments:

- root: local, parameter, uniform, or global
- fields: type, lane offset/count, byte offset
- swizzles: lane mapping
- indices: typed index expression and element type

The architectural gap is that lowering currently turns the root into a `LowerValue` too early. This loses the distinction between:

- "I need the value of this place"
- "I need to write through this place"
- "this place has an address"
- "this place is only flat registers"

The fix is to add a place-lowering layer that walks `PlaceRoot + [PlaceSegment]` and classifies the access before materializing values.

## Main Components

### `lower/place/layout.rs`

Provides frontend-neutral layout helpers built around `LpsType` and `lps_shared` layout rules.

Responsibilities:

- scalar lane counts for leaves
- byte size and alignment for aggregate elements
- array stride calculation
- struct field byte offset and lane offset validation
- field/member layout queries independent of Naga handles

This should reuse `lps_shared` layout functions where possible. It should not depend on `lps-frontend` Naga types.

### `lower/place/path.rs`

Walks a `HirPlace` and produces a lowered access classification.

Suggested shape:

```rust
enum LoweredPlace {
    FlatLanes {
        ty: LpsType,
        lanes: Vec<VReg>,
    },
    Memory {
        ty: LpsType,
        base: VReg,
        byte_offset: u32,
    },
    DynamicMemory {
        ty: LpsType,
        base: VReg,
        byte_offset: VReg,
    },
    DynamicFlat {
        root: LowerValue,
        segments: Vec<PlaceSegment>,
    },
}
```

Exact names can change during implementation, but the split should stay.

Path walking rules:

- Constant array index on flat lanes narrows to exact lane range.
- Constant array index on memory-backed root adds `index * stride` to byte offset.
- Field segment on flat lanes narrows by field lane offset/count.
- Field segment on memory-backed root adds field byte offset.
- Swizzle segment on flat lanes maps lanes directly.
- Swizzle segment on memory-backed root is only valid at scalar/vector leaves and should translate to per-lane offsets.
- Dynamic array index on memory-backed root emits or records `index * stride`.
- Dynamic array index on flat lanes falls back to dynamic flat selection/rebuild.

### `lower/place/read.rs`

Materializes a value from a `LoweredPlace`.

Rules:

- `FlatLanes` returns the existing vregs.
- `Memory` emits narrow `Load` ops for the leaf.
- `DynamicMemory` computes the address and emits narrow `Load` ops for the leaf.
- `DynamicFlat` uses the existing dynamic selection path, renamed or wrapped as an explicit fallback.

### `lower/place/write.rs`

Stores a value through a `LoweredPlace`.

Rules:

- `FlatLanes` emits copies to only the selected destination lanes.
- `Memory` emits narrow `Store` ops for only the leaf.
- `DynamicMemory` computes the address and emits narrow `Store` ops for only the leaf.
- `DynamicFlat` uses the existing select/rebuild path only when there is no addressable root.

For the fluid shader, the desired output shape for `emitters[0].pos = vec2(...)` is:

- no full aggregate load
- no select chain over all four emitters
- no full aggregate writeback
- two stores or two copies, depending on the root representation

### `lower/place/dynamic.rs`

Contains helpers for dynamic index math and fallback merge operations.

Responsibilities:

- clamp or define bounds policy consistently with current `lower_index`
- compute `index * stride + base_offset`
- lower dynamic memory reads/writes
- retain dynamic flat-register merge for vectors/matrices/small flat aggregate fallback

This is where existing `lower/ops/index.rs` behavior should migrate or be wrapped so it is clear that select/rebuild is a fallback, not the default for every index.

### `lower/storage.rs`

Keeps root storage primitives and exposes enough information for place lowering to classify roots.

Expected additions:

- a helper to get a memory base for pointer params
- a helper to get a memory base for slot-backed locals
- direct global/VMContext root base plus byte offset
- narrow load/store helpers usable by `lower/place/read.rs` and `lower/place/write.rs`

## Interactions

### Assignment

Current:

```text
assign_target
  root_value
  assign_segments
    lower_index / assign_index_value
  write_root_back_if_memory_root
```

Target:

```text
assign_target
  lower_place_for_write
  write_lowered_place
```

`place_project.rs` should either disappear or become a small fallback module for dynamic flat values.

### Place Read

Current:

```text
read_assign_target
  root_value
  read_segments
```

Target:

```text
read_assign_target
  lower_place_for_read
  read_lowered_place
```

### Dynamic Flat Fallback

The existing select/rebuild behavior remains correct for cases such as:

```glsl
vec4 v;
v[i] = 1.0;
```

and for non-addressable flat aggregate locals when no direct lane path can be determined. It should be explicitly named and tested as fallback behavior.

## Validation Strategy

Tests should cover both semantic correctness and LPIR shape.

Semantic tests:

- constant-index array-of-struct field writes produce the expected compute output
- dynamic-index writes into memory-backed arrays produce the expected output
- dynamic vector/matrix indexing still works
- existing shader tests continue to pass

Shape tests:

- fluid-style constant field writes do not emit `Select` chains proportional to the whole aggregate
- `emitters[0].pos` emits only leaf-width stores/copies
- LPIR op count for the fluid compute shader stays below a conservative ceiling

Firmware validation:

- host compute shader tests first
- then targeted firmware build
- then `just demo-esp32c6-host fluid` on hardware after implementation

## Design Decisions

- Constant-index aggregate paths are phase 1.
- Dynamic memory indexing is required by the plan and should not be treated as a vague future item.
- The Naga frontend is a reference for architecture, not a target for cleanup in this plan.
- The implementation should improve the frontend architecture rather than relying on a later LPIR optimization pass.
