# M3: Aggregate Foundations, Arrays, Structs, and Globals

## Objective

Establish the aggregate foundations needed for real GLSL programs: an `lps-glsl` data-shape view over the existing shared layout logic, a general place path, and predictable lowering for arrays, structs, nested access, globals, uniforms, and `out`/`inout`.

This milestone is not just "turn on arrays and structs." It should prevent the frontend from growing a separate special case for every spelling of aggregate access.

## Features

- aggregate shape metadata for scalars, vectors, matrices, arrays, and structs
- field offsets, array strides, byte sizes, and alignment delegated to the existing `lps_shared::layout` / LPVM data layout APIs
- matrix column layout and scalar lane counts expressed as frontend value-shape metadata layered on top of `LpsType`
- a general place representation:
  - root: local, param, global, uniform, temporary/slot if needed
  - path: field, index, swizzle, matrix column/element
  - access: readable, writable, assignable-to-call
- array types and constructors
- fixed-size array indexing
- multidimensional arrays where filetests require them
- struct declarations
- struct constructors
- member access
- arrays of structs and structs containing arrays as required by filetests
- global variables beyond constants
- uniform aggregate reads where examples/filetests require them
- const/global initialization rules used by the corpus
- nested lvalues combining member, index, and swizzle paths
- aggregate `in`, `out`, and `inout` call semantics
- aggregate return handling if existing filetests require it

## Implementation Notes

This milestone is mostly a type, place, and storage representation milestone.

The HIR should represent aggregate values explicitly enough for semantic checks, while lowering can choose a flattened scalar/vector storage strategy or slot-backed storage when emitting LPIR. Keep flattening and slot decisions out of syntax.

Add a type registry or equivalent semantic context for:

- named structs
- array element/count metadata
- aggregate field layout via existing shared layout helpers
- constructor compatibility
- scalar lane layout for value-like lowering
- byte layout for slot-backed lowering via `lps_shared::layout`

The recommended lowering model is hybrid:

- **Lane-flat values:** small fixed shapes with static access can remain vectors of LPIR values. This is compact and fast for many shader expressions.
- **Slot-backed aggregates:** locals, params, globals, uniforms, and dynamic indexed aggregate writes can use byte-addressed slots/pointers. This is the right foundation for arrays of structs, `inout`, and future aggregate returns.
- **Place API over both:** semantic analysis and call checking should see one `Place`/`AccessPath` shape. Lowering decides whether that place reads/writes lanes, slots, or a mix.

Nested place support is the key risk. Once a target like `foo.items[i].color.xy` can be typed and lowered as a path, most aggregate assignment follows naturally.

The initial implementation may use focused read/write patterns for concrete filetest shapes such as `array[i].field` and `struct.array[i].field`. If more custom patterns appear, switch to a small place-access walker before adding more arms. The desired walker shape is:

- resolve the root storage: local flat, local slot, param value, param pointer, uniform/global
- fold static projections: field, swizzle, known matrix/vector lanes, constant offsets
- lower dynamic projections: index select/update or byte-address calculation
- perform the final operation: read, write, or writable call-actual address

Prefer creating these internal concepts before broadening the filetest sweep:

- `TypeShape` / `LayoutView`: derived from `LpsType`, delegates byte layout to `lps_shared::layout`, and adds frontend-only facts like lane order and matrix value shape.
- `PlaceRoot`: local, param, global, uniform, temporary slot.
- `PlacePath`: field, index, swizzle, matrix column/element.
- `AccessMode`: read, write, read-write, call-actual.
- `AggregateSlot`: slot-backed storage for locals/params/temps where needed.

Use the existing layout code before adding anything new:

- `lp-shader/lps-shared/src/layout.rs` is the std430 size/alignment/stride authority for `LpsType`
- `lp-shader/lpvm/src/lpvm_data_q32.rs` is the byte-backed data/path-access reference
- `lp-shader/lps-frontend/src/lower_aggregate_layout.rs` and `naga_util::aggregate_layout` are good examples of a frontend adaptor over shared layout logic
- `lp-shader/lps-frontend/src/lower_ctx.rs`, `lower_array.rs`, `lower_struct.rs`, `lower_lvalue.rs`, `lower_call.rs`, and `lower_aggregate_write.rs` are useful references for aggregate slots, pointer args, writable actuals, row-major dynamic indexing, sret, and `Memcpy` fast paths

Use the archived aggregate roadmap as reference material, especially `docs-archive/roadmaps/2026-04-22-lp-shader-aggregates/`. That older work was for the Naga frontend and whole LPIR stack, so do not copy it blindly, but its pointer ABI decisions and use of shared layout are useful guardrails.

Important design choices to settle early in the milestone:

- whether `lps-glsl` can represent aggregate `inout` purely with local copy-in/copy-out for now, or whether it needs the shared pointer ABI immediately
- whether aggregate returns are in the current parity target or can be delayed until a filetest demands them
- whether dynamic indexing into arrays of structs requires slot-backed locals immediately, or whether a bounded lane-mux strategy is enough for the first pass
- how global mutable state maps onto existing runtime/project lifecycle semantics

## Filetest Gate

```bash
cargo run -p lps-filetests-app -- test --target rv32lpn.q32 --concise array struct global const uniform
```

Keep earlier slices live:

```bash
cargo run -p lps-filetests-app -- test --target rv32lpn.q32 --concise lps-glsl scalar vec operators control function lpfn
```

## Done

- array and struct filetests pass or have explicit, justified skips
- nested aggregate reads and writes work
- `out`/`inout` writeback works for aggregate paths, not only simple locals
- global initialization behavior matches the existing frontend for supported cases
- aggregate layout behavior is delegated to existing shared layout code and covered by adaptor/place unit tests
- aggregate lowering remains small enough for firmware use
