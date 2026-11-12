# Stage IV-B: Vector & Matrix Scalarization — Notes

## Scope

Extend the Naga → LPIR lowering (`lps-frontend`) to scalarize vector
and matrix operations. After this stage, GLSL programs using vec2/vec3/
vec4, mat2/mat3/mat4, swizzles, component access, and vector math all
lower to flat scalar LPIR ops. This is required for the web demo and the
majority of filetests.

## Current state

Stage IV implemented scalar-only lowering. The following all error on
vector/matrix types:

- `naga_type_to_ir_type()` — rejects non-scalar TypeInner
- `func_return_ir_types()` — scalar returns only
- `expr_scalar_kind()` — fails on vector expressions
- `lower_expr()` — no arms for Compose, Splat, Swizzle, AccessIndex
- `lower_stmt()` Store — single Copy, not N copies for vector locals
- `lower_math()` — scalar kind check fails on vector args
- `lower_lpfn()` — explicitly rejects vector out-params

## Key design challenge

In Naga, one `Handle<Expression>` can denote a `vec3` (3 scalar values).
In LPIR, there are no vector types — everything is scalar VRegs. So one
Naga expression may map to **N VRegs** (one per component).

The expression cache must change from `Vec<Option<VReg>>` to something
that can hold multiple VRegs per expression handle.

## What needs scalarization

### Naga Expression variants

| Expression                          | Scalarization                                                   |
|-------------------------------------|-----------------------------------------------------------------|
| `Compose { ty, components }`        | Collect component VRegs (recurse for vector components)         |
| `Splat { size, value }`             | Replicate scalar VReg N times                                   |
| `Swizzle { size, vector, pattern }` | Pick VRegs from vector's components by pattern                  |
| `AccessIndex { base, index }`       | Pick one VReg from base's component list                        |
| `Access { base, index }`            | Dynamic index — select via runtime comparison chain             |
| `Binary` on vectors                 | Component-wise: N binary ops                                    |
| `Unary` on vectors                  | Component-wise: N unary ops                                     |
| `Select` on vectors                 | Component-wise: N select ops                                    |
| `As` on vectors                     | Component-wise: N cast ops                                      |
| `Math` on vectors                   | Component-wise: N math ops (with scalar broadcast for mix args) |
| `ZeroValue` vector/matrix           | N zero constants                                                |
| `FunctionArgument` vector           | N consecutive param VRegs                                       |
| `Load(LocalVariable)` vector        | N local VRegs                                                   |
| `CallResult` vector                 | N result VRegs                                                  |
| `Constant` vector                   | Lower each component from global expression arena               |

### Naga Statement changes

| Statement                  | Change                                     |
|----------------------------|--------------------------------------------|
| `Store { pointer, value }` | N Copy ops (one per component)             |
| `Call { result }`          | Allocate N result VRegs for vector returns |
| `Return { value }`         | Push N VRegs                               |

### Type boundaries

| Boundary            | Change                                                      |
|---------------------|-------------------------------------------------------------|
| Function parameters | vec3 param → 3 F32 params in LPIR                           |
| Function returns    | vec3 return → 3 F32 returns in LPIR                         |
| User function calls | Flatten args and results to scalars                         |
| LPFX calls          | Vector out-params → slot with N*4 bytes, N loads after call |

## Questions

### Q1: Expression cache representation

The expression cache is currently `Vec<Option<VReg>>`. For vectors, one
Naga expression produces N VRegs.

Options:

- A) Change to `Vec<Option<SmallVec<[VReg; 4]>>>` — each entry holds
  1-16 VRegs (scalars have 1, vec4 has 4, mat4 has 16).
- B) Use a separate `Vec<Option<Vec<VReg>>>` (heap-allocated per entry).
- C) Keep the cache scalar-only and introduce a separate
  `vec_cache: Vec<Option<Vec<VReg>>>` for multi-component results.

Suggested: A. `SmallVec<[VReg; 4]>` avoids allocation for the common
case (vec4 or smaller). Scalars are 1-element SmallVecs. Uniform API.

**Answer:** A. SmallVec<[VReg; 4]>.

### Q2: Matrix support depth

Naga has matrices. The old emitter flattened them (mat4 → 16 f32 components).
GLSL matrix ops include:

- `mat * vec` → vector result (matrix-vector multiply)
- `mat * mat` → matrix result (matrix multiply)
- `mat * scalar` → matrix result (component-wise)
- `transpose`, `determinant`, `inverse`

Options:

- A) Full matrix support: flatten to scalars, inline multiply loops,
  support transpose/determinant/inverse as inline decomposition.
- B) Minimal matrix support: flatten to scalars, support component access
  and construction, but error on matrix multiply and matrix math.
- C) No matrix support in this stage. Vectors only.

Web demo doesn't use matrices, but matrix filetests exist and the old
compiler supported them. Easier to do now while we're in the scalarization
code.

**Answer:** A. Full matrix support. Flatten to scalars, inline multiply
patterns, support transpose/determinant/inverse as decomposition.

### Q3: Scalar broadcast in binary ops

GLSL allows `vec3 * float` (scalar broadcast). Naga represents this as
`Binary { left: vec3_expr, right: float_expr }`. The old emitter detected
width mismatch and broadcast the scalar.

Should the lowering:

- A) Detect width mismatch and replicate the scalar VReg for each
  component. No extra ops needed — just reuse the same VReg.
- B) Always require both operands to have the same width (error otherwise).

Suggested: A. It's simple — reuse the scalar VReg as-is for each
component lane. No Splat op needed.

**Answer:** A. Reuse the scalar VReg directly for each component lane.

### Q4: Dynamic vector access (`Access { base, index }`)

`v[i]` where `i` is a runtime value. Naga emits `Access { base, index }`.
The old emitter didn't support this (it errored). In LPIR, this would
require a comparison chain:

```
if i == 0: result = v.x
else if i == 1: result = v.y
else: result = v.z
```

Options:

- A) Implement via comparison + select chain.
- B) Error for now (same as old emitter). Most GLSL uses constant indices.

Suggested: B for now.

**Answer:** B. Error for now. Old WASM emitter didn't support it either
(filetests tagged `@unimplemented(backend=wasm)`). Add later if needed.

### Q5: LPFX vector out-parameters

Some LPFX functions return vectors via out-pointers (e.g. `lpfn_hsv2rgb`
writes to a vec3 out-param). The current lowering errors on these.

Options:

- A) Implement fully: allocate slot of N*4 bytes, pass slot address,
  load N components after call, map back to local VRegs.
- B) Error for now. Focus on scalar-returning LPFX first.

The web demo uses `lpfn_psrdnoise` which has an `out vec2 gradient`
parameter. So vector out-params are needed for the demo.

**Answer:** A. Implement fully. Slot of N*4 bytes, pass slot address,
load N components after call.
