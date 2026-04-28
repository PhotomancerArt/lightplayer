# P2 — Layout authority migration (`lps_shared::layout::std430`)

Plan dir: `docs/plans/2026-04-22-lp-shader-aggregates-m1-pointer-abi-foundation/`
Read first: `00-design.md`, `00-notes.md` (Q6 records the decision).
Parallel with: P1 (touches independent files in `lp-shader/lpir/`).

## Scope of phase

Make `lps_shared::layout::std430` the single source of truth for
aggregate byte layout in the frontend. After this phase:

- The frontend's array slot layout (currently driven by Naga's
  `TypeInner::Array { stride }` field) computes element strides and
  total sizes from `lps_shared::layout::array_stride(...,
  LayoutRules::Std430)` and `type_size(..., Std430)`.
- A new `lower_aggregate_layout` module is the single funnel the
  frontend uses to ask "for this Naga type, what is the (size, align,
  lps_type)?".
- The `min_layout_stride = ir_components * 4` patch is removed —
  std430 already gives the correct stride for every type the frontend
  supports.
- A debug assertion (test) confirms every supported aggregate type
  has matching frontend slot size and `lps_shared::layout::type_size`.

**Out of scope:**

- Pointer ABI (P3).
- Backend changes.
- Filetest CHECK rewrites — those happen in P9 once the rest of the
  ABI lands. **However,** if existing array filetests start failing
  because the byte-layout of `vec3[N]` (or similar) changed, that's
  expected — fix the affected `CHECK:` lines in this phase. Keep the
  diff small and obvious.

## Code organization reminders

- New module: `lp-shader/lps-frontend/src/lower_aggregate_layout.rs`,
  registered in `lp-shader/lps-frontend/src/lib.rs`.
- One concept per file. The new module is *purely* a funnel from
  Naga types to `lps_shared::layout` — no slot allocation, no IR
  emission.
- Helpers at the bottom; entry points and tests at the top.
- Don't add a function unless it's used. Wait for P3 to land its
  callers; only export what P2 itself needs.
- Do not introduce changes outside `lps-frontend/` and possibly
  `lps-filetests/filetests/` (for affected baselines).

## Sub-agent reminders

- Do **not** commit.
- Stay strictly within `lp-shader/lps-frontend/` (and
  `lp-shader/lps-filetests/filetests/` for re-baseline only).
- Do **not** suppress warnings or add `#[allow(...)]`.
- Do **not** weaken or `#[ignore]` tests. If a filetest fails because
  byte layout changed, **update the `CHECK:` lines** to the new layout
  (don't disable the test).
- If the byte-layout change cascades into more test failures than the
  immediate "vec3 stride" cases, stop and report — don't push through
  bulk rewrites without checking.
- Report back: files changed, validation output, deviations.

## Implementation details

### 1. New module — `lower_aggregate_layout.rs`

Skeleton:

```rust
//! Single funnel from Naga types → `lps_shared::layout::std430`.
//!
//! All aggregate slot allocation, sret-arg sizing, and host
//! marshalling buffer sizing in this crate must go through the helpers
//! in this module. This is what makes "LpvmDataQ32 bytes are the
//! shader's slot bytes" true by construction.

use lps_shared::{LayoutRules, LpsType};
use lps_shared::layout::{array_stride, type_alignment, type_size};
use naga::{Handle, Module, Type, TypeInner};

use crate::error::LowerError;

/// `(size_bytes, align_bytes)` for `naga_ty` under std430.
pub(crate) fn aggregate_size_and_align(
    module: &Module,
    naga_ty: Handle<Type>,
) -> Result<(u32, u32), LowerError> {
    let lps = naga_to_lps_type(module, naga_ty)?;
    let size = type_size(&lps, LayoutRules::Std430);
    let align = type_alignment(&lps, LayoutRules::Std430);
    Ok((size as u32, align as u32))
}

/// Element stride for an array of `element_naga_ty` under std430.
pub(crate) fn array_element_stride(
    module: &Module,
    element_naga_ty: Handle<Type>,
) -> Result<u32, LowerError> {
    let lps = naga_to_lps_type(module, element_naga_ty)?;
    Ok(array_stride(&lps, LayoutRules::Std430) as u32)
}

/// Convert a Naga type handle to the `LpsType` used by
/// `lps_shared::layout`. Errors on types the frontend does not
/// support (opaque, pointer, etc.).
pub(crate) fn naga_to_lps_type(
    module: &Module,
    handle: Handle<Type>,
) -> Result<LpsType, LowerError> {
    let inner = &module.types[handle].inner;
    naga_inner_to_lps_type(module, inner)
}

fn naga_inner_to_lps_type(
    module: &Module,
    inner: &TypeInner,
) -> Result<LpsType, LowerError> {
    use naga::ScalarKind;
    use naga::VectorSize::*;
    Ok(match inner {
        TypeInner::Scalar(s) => match (s.kind, s.width) {
            (ScalarKind::Float, _) => LpsType::Float,
            (ScalarKind::Sint,  _) => LpsType::Int,
            (ScalarKind::Uint,  _) => LpsType::UInt,
            (ScalarKind::Bool,  _) => LpsType::Bool,
            (k, w) => return Err(LowerError::UnsupportedType(alloc::format!(
                "lower_aggregate_layout: unsupported scalar {k:?}:{w}"
            ))),
        },
        TypeInner::Vector { scalar, size } => match (scalar.kind, *size) {
            (ScalarKind::Float, Bi)   => LpsType::Vec2,
            (ScalarKind::Float, Tri)  => LpsType::Vec3,
            (ScalarKind::Float, Quad) => LpsType::Vec4,
            (ScalarKind::Sint,  Bi)   => LpsType::IVec2,
            (ScalarKind::Sint,  Tri)  => LpsType::IVec3,
            (ScalarKind::Sint,  Quad) => LpsType::IVec4,
            (ScalarKind::Uint,  Bi)   => LpsType::UVec2,
            (ScalarKind::Uint,  Tri)  => LpsType::UVec3,
            (ScalarKind::Uint,  Quad) => LpsType::UVec4,
            (ScalarKind::Bool,  Bi)   => LpsType::BVec2,
            (ScalarKind::Bool,  Tri)  => LpsType::BVec3,
            (ScalarKind::Bool,  Quad) => LpsType::BVec4,
            (k, sz) => return Err(LowerError::UnsupportedType(alloc::format!(
                "lower_aggregate_layout: unsupported vector ({k:?}, {sz:?})"
            ))),
        },
        TypeInner::Matrix { columns, rows, scalar: _ } => match (*columns, *rows) {
            (Bi, Bi)   => LpsType::Mat2,
            (Tri, Tri) => LpsType::Mat3,
            (Quad, Quad) => LpsType::Mat4,
            (c, r) => return Err(LowerError::UnsupportedType(alloc::format!(
                "lower_aggregate_layout: unsupported matrix dim {c:?}x{r:?}"
            ))),
        },
        TypeInner::Array { base, size, .. } => {
            let element = naga_to_lps_type(module, *base)?;
            let len = match size {
                naga::ArraySize::Constant(nz) => nz.get(),
                _ => return Err(LowerError::UnsupportedType(String::from(
                    "lower_aggregate_layout: only constant-sized arrays supported",
                ))),
            };
            LpsType::Array {
                element: alloc::boxed::Box::new(element),
                len,
            }
        }
        TypeInner::Struct { members, .. } => {
            // M2 will exercise this. For M1 we still allow construction
            // because P2 introduces the funnel; only `array_element_stride`
            // / `aggregate_size_and_align` need it for arrays today.
            let mut out = alloc::vec::Vec::with_capacity(members.len());
            for m in members {
                let ty = naga_to_lps_type(module, m.ty)?;
                out.push(lps_shared::StructMember {
                    name: m.name.clone(),
                    ty,
                });
            }
            LpsType::Struct { name: None, members: out }
        }
        other => return Err(LowerError::UnsupportedType(alloc::format!(
            "lower_aggregate_layout: unsupported type {other:?}"
        ))),
    })
}
```

(Adapt `LpsType::Struct` / `StructMember` field names to match the
actual `lps_shared` definitions — see `lp-shader/lps-shared/src/types.rs`.
If `StructMember::name` is `Option<String>`, propagate it; otherwise drop
the line.)

Register the module in `lp-shader/lps-frontend/src/lib.rs`:

```rust
pub(crate) mod lower_aggregate_layout;
```

### 2. Migrate `flatten_local_array_shape` and `flatten_array_type_shape`

In `lp-shader/lps-frontend/src/lower_array_multidim.rs`, replace
"Naga's `TypeInner::Array { stride }`" with "`array_element_stride`":

```rust
use crate::lower_aggregate_layout::array_element_stride;

pub(crate) fn flatten_local_array_shape(
    module: &Module,
    func: &Function,
    var: &LocalVariable,
) -> Result<(SmallVec<[u32; 4]>, Handle<Type>, u32), LowerError> {
    let mut dimensions = SmallVec::<[u32; 4]>::new();
    let mut cur_ty = var.ty;
    let leaf_ty = loop {
        match &module.types[cur_ty].inner {
            TypeInner::Array { base, size, .. } => {
                let n = match size {
                    ArraySize::Constant(nz) => nz.get(),
                    ArraySize::Pending(_) | ArraySize::Dynamic => {
                        // Unchanged: infer from initializer.
                        if !dimensions.is_empty() {
                            return Err(LowerError::UnsupportedType(String::from(
                                "only outermost array may use inferred size (`[]`)",
                            )));
                        }
                        let Some(init_h) = var.init else {
                            return Err(LowerError::UnsupportedType(String::from(
                                "unsized local array requires an initializer",
                            )));
                        };
                        match &func.expressions[init_h] {
                            Expression::Compose { components, .. } => {
                                u32::try_from(components.len()).map_err(|_| {
                                    LowerError::Internal(String::from(
                                        "inferred array length overflows u32",
                                    ))
                                })?
                            }
                            _ => {
                                return Err(LowerError::UnsupportedType(String::from(
                                    "array size must be constant or inferable from `{ ... }` init",
                                )));
                            }
                        }
                    }
                };
                dimensions.push(n);
                match &module.types[*base].inner {
                    TypeInner::Array { .. } => cur_ty = *base,
                    _ => break *base,
                }
            }
            _ => return Err(LowerError::Internal(String::from(
                "flatten_local_array_shape: local is not array-typed",
            ))),
        }
    };

    let leaf_stride = array_element_stride(module, leaf_ty)?;
    dimensions.reverse();
    Ok((dimensions, leaf_ty, leaf_stride))
}
```

Same for `flatten_array_type_shape` — drop the `aligned_stride =
stride_v.max(4)` and `min_layout_stride` patches, get the stride from
`array_element_stride`.

Note: the existing helpers return `(dimensions, leaf_ty, leaf_stride)`.
**Keep that signature.** Only the `leaf_stride` value changes
(and only for types where Naga's stride disagreed with std430 — primarily
vec3[N] and any nested arrays).

### 3. Slot total sizes

Find any place in the frontend that computes "total slot bytes for an
aggregate" by hand. Common pattern: `element_count * leaf_stride` or
similar. Where it's a plain array of leaves, the new product still works
(`element_count * leaf_stride` continues to be correct since
`leaf_stride` is now std430-conformant). Only fix it if the frontend
allocates a slot whose size could now disagree with `aggregate_size_and_align`.

A quick consistency check: in `lower_ctx.rs::LowerCtx::new`, the array
slot allocation today computes `total = element_count.checked_mul(leaf_stride)`.
With the new strides this is still right for plain leaf arrays. Add a
debug assertion or a side-by-side comparison in tests (see #4).

### 4. Test — layout cross-check

Add a test (in `lp-shader/lps-frontend/src/lower_aggregate_layout.rs`
or a sibling `tests.rs`) that, for a representative set of types,
asserts that `aggregate_size_and_align(naga_ty)` matches what the
frontend's slot machinery would allocate. At minimum cover:

- `float[4]`: size 16, align 4.
- `vec2[3]`: size 24, align 8.
- `vec3[3]`: size 36, align 4. ← The interesting case (vec3 is 12B
  in this project's std430, not 16B).
- `vec4[2]`: size 32, align 16.
- `bvec4[2]`: size 32, align 16.
- `mat3` (as a return value): size 36, align 4.
- `mat4` (as a return value): size 64, align 16.
- For M2-readiness, also verify `Struct { vec3, float }` → size 16,
  align 4 (don't lower it through the frontend yet — just call
  `aggregate_size_and_align` on the type handle).

Use a tiny helper that builds Naga `Module`s with a single type and
queries it. If that's too plumbing-heavy, port the test to call into
`lps_shared::layout` directly with hand-built `LpsType` instances and
just assert the std430 numbers. The point is: lock down the std430
numbers we're committing to in M1.

### 5. Find and re-baseline affected filetests

Candidates likely affected:

- Any filetest with `vec3 ARR[N]` (vec3 elements switch from 16B to 12B
  stride if Naga had been giving 16).
- Tests asserting slot total sizes in CHECK lines.
- `lp-shader/lps-filetests/filetests/global/layout-alignment.glsl`.

Sweep:

```
rg -l 'vec3.*\[' lp-shader/lps-filetests/filetests/
rg -l 'slot ss' lp-shader/lps-filetests/filetests/
```

For each affected `CHECK:` line, update to the new byte offsets.
Document any baseline that flipped in the report-back (so the diff is
auditable).

If filetests start cascading into many failures (>10), **stop and
report** — that signals a layout interpretation mismatch we should
discuss before mass-rewriting.

## Validate

```
cargo check -p lps-frontend
cargo test  -p lps-frontend
just test-glsl
just test-glsl-filetests   # checks all four backends; expect a few re-baselines
just check
```

`just check` and `cargo test -p lps-frontend` must be green. Filetests
must be green after re-baselining.

## Done when

- `lower_aggregate_layout` module exists, registered, and used by
  `lower_array_multidim`.
- Slot strides for vec3 (and any other type Naga over-aligned) match
  `lps_shared::layout::std430` exactly.
- Cross-check test passes for the listed types.
- Affected filetest CHECK lines are updated.
- All listed validation commands are green.
- No new `#[allow(...)]`; no `#[ignore]`d tests.
