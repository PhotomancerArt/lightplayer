# Phase 1 — Plumbing: Array shape walks, address helper, init/zero threading

## Goal

Enable struct leaves in array type walks and provide the primitives that later
phases build on. After this phase, simple `Point ps[4];` declarations should
compile (declaration + zero-init only — member access still fails).

## Files to modify

| File | Changes |
|------|---------|
| `lp-shader/lps-frontend/src/lower_array_multidim.rs` | Relax `flatten_local_array_shape` and `flatten_array_type_shape` to permit `TypeInner::Struct` leaves. The walks already work; just remove/relax any assertions downstream callers make about leaf type. |
| `lp-shader/lps-frontend/src/lower_array.rs` | Add `array_element_address(ctx, info, ElementIndex) -> VReg`; refactor `load_array_element_const`, `load_array_element_dynamic`, `store_array_element_const`, `store_array_element_dynamic` to use it. ElementIndex enum: `Const(u32)` or `Dynamic(VReg)`. |
| `lp-shader/lps-frontend/src/lower_struct.rs` | Add `zero_struct_at_offset_fb(fb, base, offset, naga_ty)` variant that takes `&mut FunctionBuilder` instead of `&mut LowerCtx`. Needed because `zero_fill_array_slot` runs from `LowerCtx::new` before a full `LowerCtx` exists. |
| `lp-shader/lps-frontend/src/lower_array.rs` | Modify `zero_fill_array_slot` to detect struct leaves and call the new `zero_struct_at_offset_fb` instead of the per-IR-type loop. |
| `lp-shader/lps-frontend/src/lower_array.rs` | In `lower_array_initializer`: when `leaf_lps` is `Struct`, compute `leaf_layout = aggregate_layout(ctx.module, leaf_naga)?` once and pass `Some(&leaf_layout)` to each `store_lps_value_into_slot` call (line 413 currently passes `None`). |

## Key types and helpers

```rust
// In lower_array.rs
pub(crate) enum ElementIndex {
    Const(u32),
    Dynamic(VReg),
}

pub(crate) fn array_element_address(
    ctx: &mut LowerCtx<'_>,
    info: &AggregateInfo,
    index: ElementIndex,
) -> Result<VReg, LowerError> {
    // Clamp dynamic index to [0, element_count-1]
    // stride = info.leaf_stride()
    // base = aggregate_storage_base_vreg(ctx, &info.slot)
    // return base + (index * stride) as VReg
}
```

## Acceptance criteria

- `struct Point { float x; float y; }; Point ps[4];` compiles (filetest with `// test run` for declaration only).
- Existing scalar-leaf array tests still pass (no regression).
- `cargo test -p lps-frontend` passes.

## Out of scope for this phase

- Member access (`ps[i].x`) — Phase 2
- Outer-struct-field arrays (`s.ps[i].x`) — Phase 3
- Whole-element assignment (`ps[i] = q`) — Phase 2

## Implementation notes

### `flatten_*_array_shape` relaxation

Both functions currently break the loop on any non-`Array` `TypeInner` and
call it the leaf. They already return `(dimensions, leaf_ty, leaf_stride)`.
The `leaf_stride` comes from `array_element_stride(module, leaf_ty)` which
already handles any `LpsType` including `Struct`. So:

- No change needed to the walk logic itself.
- Verify downstream callers in `lower_ctx.rs` (where `AggregateInfo` is built)
  don't assert `leaf_ty` is scalar/vector/matrix. If they do, relax the assertion.

### Address helper refactoring

Current inlined math in the four load/store helpers:

```rust
// load_array_element_dynamic pattern (lines 259-296)
let clamped = clamp_array_index(ctx, index_v, info.element_count())?;
let stride_v = ctx.fb.alloc_vreg(IrType::I32);
ctx.fb.push(LpirOp::IconstI32 { dst: stride_v, value: info.leaf_stride() as i32 });
let byte_off = ctx.fb.alloc_vreg(IrType::I32);
ctx.fb.push(LpirOp::Imul { dst: byte_off, lhs: clamped, rhs: stride_v });
let base = aggregate_storage_base_vreg(ctx, &info.slot)?;
let addr = ctx.fb.alloc_vreg(IrType::I32);
ctx.fb.push(LpirOp::Iadd { dst: addr, lhs: base, rhs: byte_off });
// ... then Load at offset 0 from addr
```

Extract this into `array_element_address`. For `Const` index, skip the clamp
and multiply, just emit `Iadd base, IconstI32(offset)` where
`offset = index * stride`.

### `zero_struct_at_offset_fb` signature

```rust
// lower_struct.rs
pub(crate) fn zero_struct_at_offset_fb(
    fb: &mut FunctionBuilder,
    base: VReg,
    offset: u32,
    naga_ty: Handle<Type>,
) -> Result<(), LowerError> {
    // Same logic as zero_struct_at_offset but using fb directly
}
```

The existing `zero_struct_at_offset` (which takes `&mut LowerCtx`) can delegate
to this new primitive.

### `lower_array_initializer` change

Current line ~413:
```rust
crate::lower_aggregate_write::store_lps_value_into_slot(
    ctx, base, byte_off, leaf_naga, &leaf_lps, comp, None,
)?;
```

When `matches!(&leaf_lps, LpsType::Struct { .. })`:
```rust
let leaf_layout = crate::naga_util::aggregate_layout(ctx.module, leaf_naga)?
    .ok_or_else(|| LowerError::Internal(...))?;
// pass Some(&leaf_layout) instead of None
```

Do the same for the tail-zero loop (or verify `zero_leaf_lps_in_slot` already
routes Struct correctly — it does, via `lower_struct::zero_struct_at_offset`).

## Testing

Create a minimal filetest:

```glsl
// lp-shader/lps-filetests/filetests/array/of-struct/declare-zero.glsl
// test run
// expected: 0 0 0 0 0 0 0 0

struct Point {
    float x;
    float y;
};

void main() {
    Point ps[4];
    // All should be zero-initialized
    output_f32(ps[0].x);
    output_f32(ps[0].y);
    output_f32(ps[1].x);
    output_f32(ps[1].y);
    output_f32(ps[2].x);
    output_f32(ps[2].y);
    output_f32(ps[3].x);
    output_f32(ps[3].y);
}
```

This should compile and run after Phase 1 (though reads may return zeros
only if zero-fill works; member access read paths aren't wired yet, but
the compile should succeed).

Actually — with no member access wired, the reads won't compile. So instead:

```glsl
// test error
// error-pattern: AccessIndex.*not supported

struct Point { float x; float y; };
void main() {
    Point ps[4];
    // Just declaration should compile; member access expected to error
    // in this phase. Remove this test once Phase 2 lands.
}
```

Hmm, that's not very satisfying. Better: add a compile-only smoke test in the
frontend unit tests (if we have them) or just verify via `cargo build` that
the frontend doesn't panic on this input. The real acceptance is that
Phase 2 can build on this without hitting "leaf is not scalar" errors.

Simpler Phase 1 test: run the existing array test corpus and verify no
regression. The structural change (relaxing leaf type) should be invisible
to scalar-leaf arrays.
