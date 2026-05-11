# Milestone 3 — Arrays of structs (locals + params)

## Status

**Complete** — 2026-04-23. All array-of-struct filetests pass on `wasm.q32`,
`rv32c.q32`, and `rv32n.q32` with parity. Plan and phase notes live under
`docs/roadmaps/2026-04-22-lp-shader-aggregates/m3-arrays-of-structs/`.

## Goal

Extend `ArrayInfo` to allow `LpsType::Struct` as the leaf element type,
so GLSL programs can declare and use `Particle ps[8];` as locals and
function parameters. The array machinery already addresses elements by
flat row-major index × leaf stride; the change is to make leaf stride
and element access aware that the leaf is itself a heterogeneous
aggregate.

## Suggested plan name

`lp-shader-aggregates-m3-arrays-of-structs`

## Scope

### In scope

- Extend `ArrayInfo` (`lp-shader/lps-frontend/src/lower_ctx.rs:53`) so
  `leaf_element_ty` may resolve to a `LpsType::Struct`. `leaf_stride`
  becomes std430-aligned struct size for that case.
- Extend `lp-shader/lps-frontend/src/lower_array_multidim.rs` /
  `flatten_local_array_shape` to permit struct leaves. Total slot size
  = `element_count × leaf_stride`.
- Extend `lower_array.rs` element load/store helpers: when the leaf is
  a struct, element access produces a struct-element address (= array
  base + index × stride) which is then handed to `lower_struct.rs`
  helpers from M2 for member access / Memcpy / Compose-into-slot.
- Extend `lower_expr.rs` `AccessIndex` and `Access` (dynamic index)
  handling so a chain like `ps[i].position.x` resolves correctly:
  array index → element address → struct member offset → typed Load.
- Extend `lower_stmt.rs` for stores into struct array elements:
  `ps[i] = q;` → element address + struct Memcpy from RHS slot.
  `ps[i].x = 5.0;` → element address + struct member Store.
- Extend `lower_access.rs` for stores through array-of-struct via an
  `inout`/`out` param pointer.
- Toggle off `@unimplemented` markers on
  `lp-shader/lps-filetests/filetests/const/array-size/struct-field.glsl`
  (the existing struct-with-array-field-and-struct-array test).
- Add 2–3 new filetests under `lp-shader/lps-filetests/filetests/struct/`
  or a new `lp-shader/lps-filetests/filetests/array/of-struct/`
  subdirectory exercising:
  - Local `Point ps[4];` + element member access (`ps[0].x`, `ps[i].y`).
  - `inout Point ps[4]` function parameter with member writes inside the
    callee.
  - Nested: struct containing an array of struct (if not already covered
    by the const-array-size test).
- Per Q7: also enable on `rv32c.q32` / `rv32n.q32` after backend
  validation; anything that doesn't pass becomes a known issue with
  TODO + filed follow-up.

### Out of scope

- Uniform-with-array-of-struct (M4 — separate code path in
  `load_lps_value_from_vmctx`).
- Read-only-`in` optimisation (M5).
- Nested arrays of arrays of structs (`Point ps[4][4]`) — defer unless
  the existing multidim machinery extends naturally; otherwise file a
  follow-up.

## Key decisions

- **Leaf-stride is std430-aligned struct size.** Same rule as
  `LpsType::array_stride` already implements; reused here.
- **Element address representation.** Whether to introduce a new helper
  `array_of_struct_element_address(ctx, info, idx_vreg) -> VReg` or
  reuse the existing `compute_array_element_address` with a different
  stride. Settled in the M3 plan; expected to reuse with stride
  parameterisation.
- **Whether to add a new `lps-filetests/filetests/array/of-struct/`
  directory or extend `struct/`.** Settled in the M3 plan; either is
  fine. Probably new directory for discoverability.

## Deliverables

- Modified files:
  - `lp-shader/lps-frontend/src/lower_ctx.rs` (ArrayInfo leaf relaxation)
  - `lp-shader/lps-frontend/src/lower_array_multidim.rs` /
    `lower_array.rs` (struct-leaf stride + element address paths)
  - `lp-shader/lps-frontend/src/lower_expr.rs` (chained AccessIndex
    through array-of-struct)
  - `lp-shader/lps-frontend/src/lower_stmt.rs` (struct-element stores)
  - `lp-shader/lps-frontend/src/lower_access.rs` (struct-element stores
    via pointer)
- New filetests (2–3) for array-of-struct local + param scenarios.
- `@unimplemented` markers off on `const/array-size/struct-field.glsl`.

## Dependencies

- **Requires M1 complete** (unified pointer ABI; array machinery is
  pointer-based for params).
- **Requires M2 complete** (`lower_struct.rs` helpers — member load /
  store / memcpy / compose-into-slot — are reused at struct-element
  addresses).

## Execution strategy

**Option B — `/plan-small`.**

Justification: M1 + M2 have already established both the array
machinery (with pointer ABI) and the struct lowering. M3 is "extend
the array leaf type" — a focused extension across a handful of files,
no new module, ~2–3 phases. There's one design question worth pinning
(element-address helper shape), but no major architectural choices.

**Suggested chat opener:**

> I suggest we use the `/plan-small` process for this milestone, after
> which I will automatically implement. Agree?
