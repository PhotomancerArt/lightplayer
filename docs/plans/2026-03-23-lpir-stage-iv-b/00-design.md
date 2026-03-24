# Stage IV-B: Vector & Matrix Scalarization — Design

## Scope

Extend the Naga → LPIR lowering (`lp-glsl-naga`) to scalarize all vector
and matrix types and operations. After this stage, one Naga expression of
type `vec3` maps to 3 scalar LPIR VRegs, a `mat4` maps to 16 VRegs, and
all vector/matrix operations decompose into per-component scalar LPIR ops.

This is required for the web demo (which uses `vec2`, `vec3`, `mat2`) and
the majority of filetests.

## File structure

```
lp-glsl/lp-glsl-naga/
├── Cargo.toml                    # UPDATE: add smallvec dependency
└── src/
    ├── lower_ctx.rs              # UPDATE: SmallVec expr cache, multi-VReg
    │                             #   locals/params, ensure_expr_vec
    ├── lower_expr.rs             # UPDATE: Compose, Splat, Swizzle,
    │                             #   AccessIndex, component-wise Binary/
    │                             #   Unary/Select/As, vector ZeroValue,
    │                             #   vector Constant
    ├── lower_stmt.rs             # UPDATE: vector Store (N copies),
    │                             #   vector Return, vector Call results
    ├── lower_math.rs             # UPDATE: component-wise math dispatch,
    │                             #   vector broadcast for mixed args
    ├── lower_lpfx.rs             # UPDATE: vector out-params (slot + loads),
    │                             #   vector value args
    ├── lower_matrix.rs           # NEW: mat*vec, mat*mat, transpose,
    │                             #   determinant, inverse
    ├── lower.rs                  # UPDATE: vector/matrix params and returns
    ├── expr_scalar.rs            # UPDATE: handle vector/matrix type queries
    └── lib.rs                    # UPDATE: pub(crate) mod lower_matrix
```

## Conceptual architecture

```
Naga Expression (may be vec3, mat4, scalar)
  │
  ▼
ensure_expr_vec(handle) → SmallVec<[VReg; 4]>
  │
  ├─ cache hit → return cached SmallVec
  │
  ├─ scalar path (unchanged from Stage IV)
  │   └─ returns [vreg]  (1-element SmallVec)
  │
  ├─ vector path
  │   ├─ Compose → collect component VRegs
  │   ├─ Splat → [scalar_vreg; N]
  │   ├─ Swizzle → pick from base components by pattern
  │   ├─ AccessIndex → [base_components[index]]
  │   ├─ Binary/Unary → N scalar ops (broadcast if width mismatch)
  │   ├─ Select → N scalar Select ops
  │   ├─ As → N scalar cast ops
  │   ├─ Math → N scalar math ops (or matrix-specific)
  │   ├─ ZeroValue → N zero constants
  │   ├─ Constant → N constants from global expr arena
  │   ├─ FunctionArgument → N consecutive param VRegs
  │   ├─ Load(LocalVariable) → N local VRegs
  │   └─ CallResult → N result VRegs from call
  │
  └─ matrix path
      ├─ Compose → collect column vectors → flatten to N*M VRegs
      ├─ Binary(mat*scalar) → component-wise N*M scalar ops
      ├─ Binary(mat*vec) → dot products (lower_matrix)
      ├─ Binary(mat*mat) → column-wise mat*vec (lower_matrix)
      ├─ Math(Transpose) → index rearrangement
      ├─ Math(Determinant) → cofactor expansion (lower_matrix)
      ├─ Math(Inverse) → adjugate/det (lower_matrix)
      └─ ZeroValue/Constant → N*M zero/const ops
```

## Key API change: multi-VReg expression cache

The expression cache changes from `Vec<Option<VReg>>` to
`Vec<Option<SmallVec<[VReg; 4]>>>`. Two expression-lowering entry points:

- `ensure_expr_vec(handle)` → `SmallVec<[VReg; 4]>` — returns all
  components (1 for scalar, N for vecN, N*M for matN×M).
- `ensure_expr(handle)` → `VReg` — convenience wrapper that asserts
  scalar (1 component) and returns the single VReg. Existing scalar call
  sites remain unchanged.

## Main components

### `lower_ctx.rs` — multi-VReg infrastructure

Changes:
- `expr_cache: Vec<Option<SmallVec<[VReg; 4]>>>`
- `local_map: BTreeMap<Handle<LocalVariable>, SmallVec<[VReg; 4]>>`
- `param_aliases: BTreeMap<Handle<LocalVariable>, SmallVec<[VReg; 4]>>`
- `naga_type_to_ir_types(inner) → SmallVec<[IrType; 4]>` — returns N
  types for vectors/matrices
- `naga_type_width(inner) → usize` — component count (1, 2, 3, 4, or
  N*M for matrices)
- `ensure_expr_vec()` / `ensure_expr()` as described above
- `resolve_local()` returns `SmallVec<[VReg; 4]>`
- Parameter setup: vector params add N params, local vars allocate N VRegs

### `expr_scalar.rs` — type query updates

`expr_scalar_kind` must handle vector/matrix expressions — extract the
scalar element kind from `TypeInner::Vector { scalar, .. }` and
`TypeInner::Matrix { scalar, .. }`. Also handle `Compose`, `Splat`,
`Swizzle`, and `AccessIndex` by recursing through base.

New helper: `expr_type_inner(module, func, expr) → &TypeInner` to get
the full type (needed to determine vector width for broadcasts).

### `lower_expr.rs` — vector expression lowering

New function `lower_expr_vec()` that dispatches by expression variant
and type. For scalar expressions, delegates to existing scalar lowering
and wraps in 1-element SmallVec. For vector/matrix expressions:

- **Compose**: Naga `Compose { ty, components }` — each component may be
  scalar or vector. Collect all component VRegs into a flat SmallVec.
- **Splat**: `Splat { size, value }` — lower scalar, replicate VReg N
  times (no extra ops — reuse same VReg).
- **Swizzle**: `Swizzle { size, vector, pattern }` — lower base vector,
  pick VRegs by pattern indices.
- **AccessIndex**: `AccessIndex { base, index }` on vector → return
  single VReg. On matrix → return column (vector width VRegs).
- **Binary/Unary/Select/As**: component-wise with scalar broadcast.
- **ZeroValue**: N zero constants.
- **Constant**: lower each component from global expression arena.
- **FunctionArgument**: consecutive VRegs starting at parameter offset.
- **Load**: N local VRegs.
- **CallResult**: N pre-allocated VRegs.

### `lower_math.rs` — component-wise math

Detect vector arguments via type query. For per-component math functions
(abs, sqrt, floor, sin, mix, etc.), loop over component count and call
the existing scalar math lowering per component.

Handle scalar broadcast in multi-arg functions: `mix(vec3, vec3, float)`
broadcasts the float to each lane. Use `expr_type_inner` to detect width
mismatch.

Vector-specific math functions dispatch to `lower_matrix.rs`:
- `Dot` → inline dot product (multiply + add chain)
- `Cross` → inline cross product formula
- `Length` → dot(v,v) → sqrt
- `Distance` → length(a-b)
- `Normalize` → v / length(v)
- `FaceForward`, `Reflect`, `Refract` → inline formulas

### `lower_matrix.rs` — matrix decomposition

All matrix operations decompose to scalar LPIR ops:

- **mat × vec**: for each row, emit dot product with the vector.
- **mat × mat**: treat right matrix column-by-column, each is mat×vec.
- **transpose**: rearrange VRegs (no ops emitted, just index shuffle).
- **determinant**: cofactor expansion (2×2 inline, 3×3 inline, 4×4
  recursive via 3×3 minors).
- **inverse**: adjugate matrix / determinant.

Matrix storage: column-major (matching Naga/GLSL convention). A mat3 has
columns [c0, c1, c2], each a vec3. Flattened: [c0.x, c0.y, c0.z, c1.x,
c1.y, c1.z, c2.x, c2.y, c2.z] — 9 VRegs.

### `lower_stmt.rs` — vector statement changes

- **Store**: N `Copy` ops (one per component).
- **Return**: push all N component VRegs.
- **Call (user)**: flatten vector arguments, allocate N result VRegs.

### `lower_lpfx.rs` — vector LPFX changes

- Vector value arguments: flatten to N scalar args.
- Vector out-parameters: allocate slot of N×4 bytes, pass slot address,
  load N components at offsets [0, 4, 8, ...] after call.
- Both `build_lpfx_import_decl` and `lpfx_arg_kinds` updated for vectors.
- New `LpfxArgKind::OutVector(IrType, u8)` variant (type + component count).

### `lower.rs` — entry point changes

- `func_return_ir_types` handles vector/matrix returns (N types).
- Parameter setup passes through to updated `LowerCtx::new`.
