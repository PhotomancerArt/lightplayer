# Place-Based Aggregate Lowering Notes

## Scope

Fix the `lps-glsl` frontend architecture for reads and writes through places, with the fluid compute shader as the motivating failure:

```glsl
emitters[0].pos = vec2(...);
```

This should not rebuild the whole `emitters[4]` aggregate in LPIR. The plan should define an architecture that lowers place paths to either direct lane access or direct memory access, and reserves select/rebuild lowering for genuinely dynamic register-backed aggregate updates.

The work is compiler architecture work, not a product workaround. The on-device GLSL JIT remains required on ESP32-C6.

## Current State

### Triggering Failure

`examples/fluid/compute.glsl` is very small: it writes a few fields on four `FluidEmitter` entries. The OOM backtrace from ESP32-C6 shows the large allocation happening while `Vec<LpirOp>` grows during `lps_glsl` lowering:

```text
alloc::vec::Vec<lpir::lpir_op::LpirOp>::push
lps_glsl::lower::ops::index::assign_index_value
lps_glsl::lower::ops::place_project::assign_segments
lps_glsl::lower::ops::place_write::assign_target
lps_glsl::lower::lower_expr
lps_glsl::lower::lower_statements
lps_glsl::job::CompileJob::step
lpc_engine::gfx::native_jit::Graphics::compile_compute_shader
```

The allocation request was 81,920 bytes with roughly 108 KiB free. This is a symptom of excessive LPIR op generation, not shader complexity.

### Fluid Emitter Shape

`examples/fluid/compute.toml` declares:

```toml
[produced_slots.emitters]
kind = "map"
key = "u32"
value = "lp::fluid::Emitter"
mapping = { kind = "sentinel", len = 4, key = "id", empty_key = 0 }
```

The generated shader header exposes this as roughly:

```glsl
struct FluidEmitter {
    uint id;
    vec2 pos;
    vec2 dir;
    float radius;
    vec3 color;
    float velocity;
    float intensity;
};

FluidEmitter emitters[4];
```

`FluidEmitter` is 11 scalar lanes in the frontend's flattened representation. `emitters[4]` is therefore 44 lanes.

### Current lps-glsl Lowering

Relevant files:

- `lp-shader/lps-glsl/src/hir/place.rs`
- `lp-shader/lps-glsl/src/lower/ops/place_write.rs`
- `lp-shader/lps-glsl/src/lower/ops/place_project.rs`
- `lp-shader/lps-glsl/src/lower/ops/index.rs`
- `lp-shader/lps-glsl/src/lower/storage.rs`

HIR places already carry useful metadata:

- `PlaceRoot::{Local, Param, Uniform, Global}` identifies the root.
- `PlaceSegment::Field` carries lane offset, lane count, and byte offset.
- `PlaceSegment::Swizzle` carries relative lanes.
- `PlaceSegment::Index` carries the typed index expression and element type.
- `HirPlace::lanes` tracks direct root lanes only until an index appears.

The problem is that lowering does not preserve a place as an address/lane path. Once `HirPlace::lanes` becomes `None`, `place_write::assign_target` falls back to:

1. `root_value`: read the entire root aggregate into a flat `LowerValue`.
2. `assign_segments`: recursively project/update a subvalue.
3. `write_root_back_if_memory_root`: write the entire aggregate back for memory roots.

For an index segment, `assign_segments` calls:

- `lower_index` to select one element from a full aggregate value.
- `assign_index_value` to merge the updated element back into the full aggregate.

`assign_index_value` always treats the index as dynamic. It clamps the index, loops over every possible element, compares the index against each element number, selects between the new value and old value for each lane, and copies the result back.

That is a reasonable fallback for dynamic writes to flat register aggregates, but it is pathological for `emitters[0].pos`:

- The index is a constant.
- The destination is a produced global/output backed by VMContext memory.
- Only two lanes need to change.
- The current path can touch all 44 lanes and emit select/copy scaffolding for each field write.

It also does extra root load/store work around each field assignment.

### Comparison With the Naga Frontend

Relevant older frontend files:

- `lp-shader/lps-frontend/src/lower_access.rs`
- `lp-shader/lps-frontend/src/lower_aggregate_write.rs`
- `lp-shader/lps-frontend/src/naga_util.rs`
- `lp-shader/lps-frontend/src/lower_array.rs`
- `lp-shader/lps-frontend/src/lower_array_multidim.rs`
- `lp-shader/lps-frontend/src/lower_ctx.rs`

The Naga-based frontend has a clearer split:

- Dynamic vector/matrix register access uses select/merge helpers like `select_lane_dynamic`, `merge_flat_index_store`, and matrix-specific variants.
- Aggregate arrays/structs are represented with `AggregateLayout` and `AggregateInfo`.
- Memory-backed aggregate operations calculate addresses and emit `Load`, `Store`, or `Memcpy`.
- `store_through_access` recognizes stores through access chains and routes array aggregate writes to `store_array_element_dynamic`.
- `lower_aggregate_write` contains a typed slot writer, struct member layout support, and memcpy fast paths for whole aggregate copies.

The old frontend still has complexity and accumulated milestone code, but architecturally it keeps an important distinction that `lps-glsl` currently lacks:

> A place path is not the same thing as a value. Reads may materialize a value, but writes should usually lower through the path to the destination.

### Why This Is a Frontend Rewrite Bug

The new `lps-glsl` frontend introduced its own HIR and place representation, but the lowering currently handles complex places by converting the root to a flat value too early. This loses the information needed to emit a narrow update.

The bug is not just "constant indices should be optimized." Constant-index fast paths are necessary, but the larger issue is that write lowering is not place-based:

- It lacks a reusable lowered-place abstraction.
- It lacks a shared aggregate layout model for byte offsets and strides.
- It lacks a direct-store path for `Global`, pointer `Param`, and slot-backed `Local` roots.
- It uses dynamic select/rebuild operations for cases that are statically addressable.

## Desired Direction

Introduce a place-lowering layer in `lps-glsl` that can lower an HIR place into one of a few explicit forms:

- Direct flat lanes: register-backed scalar/vector/matrix/struct leaves with known lane indexes.
- Direct memory address: base pointer plus byte offset for memory-backed roots.
- Dynamic memory address: base pointer plus runtime-computed byte offset for dynamic array indices.
- Dynamic register selection: select/rebuild fallback for flat register aggregates when no direct memory address exists.

Writes should prefer the most direct representation:

1. Constant-index and field paths into memory-backed roots become stores to exact offsets.
2. Constant-index paths into flat locals become copies to exact lane indexes.
3. Dynamic-index paths into memory-backed aggregate roots compute an address once and store the leaf there.
4. Dynamic-index paths into flat register aggregates keep using select/merge, but only when needed.

Reads should use the same place-lowering model:

1. Direct flat lanes return existing vregs.
2. Direct memory paths emit narrow loads.
3. Dynamic memory paths compute an address and emit loads for the requested leaf.
4. Dynamic register paths use select chains.

## Architecture Goals

- Preserve on-device `no_std + alloc` operation.
- Avoid host precompilation or compiler feature gating.
- Keep HIR place metadata source-of-truth; do not infer paths from strings.
- Share layout logic where possible, but do not blindly import Naga-specific types into `lps-glsl`.
- Make LPIR op count proportional to the written leaf, not the whole aggregate, for statically addressable places.
- Keep dynamic register-backed operations correct, even if less compact.
- Add tests that assert both behavior and op-count shape for fluid-style aggregate writes.

## Open Questions

### 1. Should `lps-glsl` share aggregate layout code with `lps-frontend`, or grow a frontend-neutral layout module?

Context: `lps-frontend` has `AggregateLayout`, `AggregateInfo`, and aggregate write helpers, but they are coupled to Naga handles and Naga expression forms. `lps-glsl` already has `LpsType` and byte offsets on `PlaceSegment::Field`.

Suggested answer: create a small frontend-neutral aggregate/place layout layer inside `lps-glsl` first, backed by `LpsType` and `lps_shared` layout functions. Reuse concepts from the Naga frontend, not the Naga-specific structs. Later, if both frontends survive long-term, extract shared layout code into a common crate/module.

### 2. Should the first implementation support dynamic memory indexing, or only constant indexing?

Context: the fluid bug only needs constant indices like `emitters[0].pos`, but the right architecture can naturally compute `base + index * stride + field_offset`.

Suggested answer: include dynamic memory indexing in the design, but phase it after constant-index lane/address paths if needed. The first correctness/perf win should be constant-index writes and reads because that is the current blocker and easiest to validate.

Answer: constant-index paths come first, but dynamic memory indexing must be included as part of the plan. The architecture should not paint us into a constant-only corner.

### 3. What should happen to the existing select/rebuild path?

Context: `lower_index` and `assign_index_value` are still useful for dynamic indexing into register-backed vectors/matrices/small flat aggregates.

Suggested answer: keep them as a fallback with clearer names and narrower use. Add tests ensuring constant indexed aggregate writes do not go through `assign_index_value`.

### 4. Should the plan treat the Naga frontend as a reference only, or also clean it up?

Context: current engine compute compilation uses `lps_glsl::compile` by default. The Naga path is still present behind feature support and has valuable architecture, but fixing both would expand scope.

Suggested answer: use Naga frontend as design reference only. Do not modify it except perhaps adding comparative tests or comments if necessary.

## User Notes

- The on-device JIT remains non-negotiable; no std-only or host-precompiled workaround.
- The user wants this treated as a compiler architecture improvement, not a quick local fix.
- The current branch already has a committed checkpoint: `7c5ab58a Improve ESP32 demo and panic diagnostics`.
- Leave unrelated `docs/use-cases/2025-05-08-fyeah-sign.md` changes alone.
