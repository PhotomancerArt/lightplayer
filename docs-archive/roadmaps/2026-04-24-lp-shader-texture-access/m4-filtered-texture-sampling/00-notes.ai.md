# Scope of Work

Milestone 4 adds GLSL `texture(sampler2D, vec2)` support for normalized 2D
sampling on logical `Texture2D` uniforms.

In scope:

- Lower supported Naga/GLSL `texture(sampler2D, vec2)` calls using
  `TextureBindingSpec` metadata.
- Convert normalized `uv` coordinates to texture-space coordinates.
- Implement `TextureFilter::Nearest`.
- Implement `TextureFilter::Linear` as bilinear filtering.
- Implement per-axis `TextureWrap` policy:
  - `ClampToEdge`
  - `Repeat`
  - `MirrorRepeat`
- Reuse the M3 `texelFetch` descriptor, address, format load, unorm conversion,
  and vec4 fill machinery.
- Add a Rust reference implementation for sampler math and fixture expected
  values so tests do not duplicate subtle wrap/filter formulas by hand.
- Add filetests for nearest, linear, clamp, repeat, mirror-repeat, and mixed
  per-axis policy.

Out of scope:

- Mipmaps, implicit derivatives, `textureLod`, `textureGrad`, or nonzero LOD.
- `clamp_to_border`.
- New texture storage formats.
- Product-level texture routing or public palette helpers.
- wgpu parity execution.

# Current State

- `TextureBindingSpec` already carries `format`, `filter`, `wrap_x`, `wrap_y`,
  and `shape_hint` in `lp-shader/lps-shared/src/texture_format.rs`.
- Filetest texture directives already parse `filter=nearest|linear`,
  `wrap=...`, `wrap_x=...`, and `wrap_y=...` in
  `lp-shader/lps-filetests/src/parse/parse_texture.rs`.
- M3 `texelFetch` lowering is currently inline in
  `lp-shader/lps-frontend/src/lower_texture.rs`.
- That inline path already handles descriptor lane loads, integer coordinate
  clamp/unchecked mode, row-stride-aware address math, format-specialized
  `Load16U`, `Unorm16toF`, and GLSL vec4 missing-channel fill.
- `lpir::LpirOp` has enough basic scalar ops for sampler math: float arithmetic,
  min/max, floor/trunc/nearest, float-to-int casts, integer remainder, select,
  and loads.
- Some Q32 float operations are already lowered as builtin calls in the native
  backend (`Ffloor`, `Ftrunc`, `Fnearest`, `FtoiSatS`, `FtoiSatU`, etc.), while
  cheaper Q32 ops such as `Fmin`, `Fmax`, unorm conversion, and arithmetic are
  inline.
- GLSL/math builtins are wired through generated builtin IDs and ABI tables
  across native, Cranelift, WASM, and emulator paths. Adding texture sampling
  as a builtin would therefore touch generated builtin plumbing, signatures,
  runtime dispatch, and likely multi-result or sret import support.
- Current imported math builtin registration in `lps-frontend` is scalar-return
  oriented. `texture()` returns `vec4`, so a pure builtin path would need either
  four scalar-return imports, multi-return import support across all targets, or
  sret-style import support.

# Questions

## Confirmation-style Questions

| # | Question | Context | Suggested answer |
| --- | --- | --- | --- |
| Q1 | Add a Rust reference sampler for wrap/filter math? | M4 needs exact nearest cases and approximate bilinear cases across several wrap policies. | Yes. Put it in test-facing code first, with pure helpers that can later be promoted if useful. |
| Q2 | Keep M4 limited to implicit base LOD only? | M4 roadmap excludes mipmaps/LOD/derivatives. | Yes. Reject unsupported Naga sampled-image shapes with clear diagnostics. |
| Q3 | Reuse M3 texel load/channel-fill helpers instead of rewriting loads? | M3 already emits row-stride-aware descriptor loads and format-specialized unorm16 vec4 fill. | Yes. Refactor `lower_texture.rs` helpers only as needed to share them. |
| Q4 | Use tolerance-based filetest expectations for linear filtering? | Q32 and native-float paths may differ by a small fixed-point rounding amount. | Yes. Keep direct `texelFetch` exact, but use approximate expectations for bilinear samples. |
| Q5 | Include a mixed-axis wrap test? | `TextureBindingSpec` supports independent `wrap_x` and `wrap_y`. | Yes. Add at least one test where X and Y use different policies. |

## Discussion-style Questions

### Q6: Should `texture()` be inline LPIR, builtin-based, or hybrid?

Current M3 precedent is inline LPIR for `texelFetch`, which keeps the texture
policy compile-time-specialized and avoids runtime format/policy switches.
However, M4 sampling adds more math: normalized coordinate scaling, wrap
canonicalization, nearest rounding, four neighbor fetches for linear, and
bilinear interpolation. Fully inlining every path may generate large shader
bodies, especially for repeated texture calls inside loops.

Further builtin research:

- `lpir::ImportDecl` already supports `needs_vmctx`, multi-word
  `return_types`, and `sret`.
- Existing LPFN builtins return vector-shaped data with a result-pointer ABI:
  the extern function takes `*mut i32` / `*mut f32` as its first argument and
  writes 3 or 4 lanes.
- `lpvm-wasm` detects result-pointer builtins by comparing the LPIR import's
  logical `return_types` with the generated WASM import signature, allocates a
  temporary result buffer in linear memory, calls the builtin, then loads the
  lanes back into result vregs.
- `lpvm-cranelift` has matching import result-pointer handling for LPFN
  builtins.
- `lpvm-native` call lowering already tracks whether the callee uses sret and
  whether the caller passes the sret pointer in LPIR args.
- The current result-pointer detection and overload resolution are mostly
  hardwired around the `lpfn` module. Texture sampling builtins would need a
  deliberate namespace/mapping strategy, or the result-pointer builtin handling
  should be generalized beyond LPFN.
- Texture sampling builtins also need guest texture memory reads, not just
  scalar math or result write-back. This is not a new conceptual ABI: VMContext
  is already passed as a guest pointer word, and native builtins such as
  `__lp_vm_get_fuel_q32` cast that word back to a `VmContext` pointer. Wasmtime
  builtin dispatch already receives the shared `env.memory` handle and uses it
  for result-pointer write-back. Texture reads would extend that existing
  design rather than invent a separate memory model.

Resolved answer: use a builtin-first implementation path, but do not specialize
on the full sampler-policy matrix. Specialize where it removes real data-path
complexity, especially storage format and dimensionality/shape. Keep small
policy choices such as wrap mode and, initially, filter mode as runtime branches
inside the sampler helper/builtin unless measurement shows they dominate.

The v0 design should still preserve a helper boundary for inline lowering:

- Inline or reuse M3 `texelFetch` for exact integer fetches.
- Prefer builtins for `texture()` sampling, especially linear filtering and
  non-clamp wrap modes.
- Allow nearest/clamp/R16-style cases to be inlined later once builtin semantics
  and filetests are stable.
- Avoid both extremes: do not use one fully generic format-dispatch builtin, but
  also do not generate a builtin for every format/filter/wrap combination.
- Specialize on `TextureShapeHint::HeightOne` as a 1D texture path. GPU-facing
  APIs still model textures as 2D, but palette/gradient lookups are common
  enough that removing Y-axis sampling/address math is worth a dedicated path.
- Treat WASM/builtin texture memory access as an explicit implementation detail
  if M4 uses texture builtins for mainline filetests, but do not treat it as a
  blocker to the builtin-first design.
- Limit the initial specialization axes so M4 does not explode into every
  format/filter/wrap combination at once.

### Q8: Which sampler policy combinations should M4 support initially?

Compile-time-selected builtins avoid runtime policy dispatch, but the full
matrix is large: 3 formats x 2 filters x 3 wrap modes on X x 3 wrap modes on Y,
before considering shape hints or future formats. Specializing on all axes is
not practical and creates too many symbols/tests for the value it provides.

Suggested answer: specialize on texture storage format and dimensionality
(`General2D` vs `HeightOne`). Keep filter selection runtime inside the
format/shape-specific sampler unless the implementation proves that splitting
nearest/linear materially improves generated code size or speed. The branch cost
is small, and keeping nearest/linear together reduces builtin symbol count and
generated ABI surface.

Concretely, prefer this initial shape:

- `texture2d_rgba16_unorm(out, desc, uv, filter, wrap_x, wrap_y)`
- `texture1d_rgba16_unorm(out, desc, u, filter, wrap_x)`
- `texture2d_r16_unorm(out, desc, uv, filter, wrap_x, wrap_y)`
- `texture1d_r16_unorm(out, desc, u, filter, wrap_x)`
- `texture2d_rgb16_unorm(...)` / `texture1d_rgb16_unorm(...)` if supporting
  RGB in M4 remains cheap.

Each format-specific builtin should dispatch to small inline internal helpers
for nearest and linear sampling. Wrap helpers stay shared and runtime-selected.
The 1D helpers should ignore Y and assume `height == 1`, relying on M3c runtime
validation to reject `HeightOne` bindings with runtime `height != 1`.

For M4 v0, support the formats that already exist in `TextureStorageFormat`
where the helper implementation is shared enough to avoid extra complexity.
If time or review size gets tight, prioritize `Rgba16Unorm` and `R16Unorm`,
and defer `Rgb16Unorm` with a compile-time diagnostic.

### Q9: How should `HeightOne` interact with `texture(sampler2D, vec2)` input?

`TextureShapeHint::HeightOne` is a compile-time binding hint, but GLSL still
uses `sampler2D` and `vec2` coordinates. The 1D specialized builtin can ignore
the `v` coordinate and `wrap_y`, or it can still validate/apply Y behavior.
Ignoring Y is faster and matches the palette/gradient motivation, but it should
be explicit so shaders do not accidentally depend on 2D Y semantics for
height-one textures.

Suggested answer: for `HeightOne`, lower `texture(sampler2D, vec2)` to a 1D
sampler builtin that uses only `uv.x`, `filter`, and `wrap_x`; ignores `uv.y`
and `wrap_y`; and relies on runtime validation that the bound texture really
has `height == 1`. Add docs/tests showing that Y has no effect for height-one
sampling.

## Resolved Answers

- Q1: Add a Rust reference sampler for wrap/filter math.
- Q2: Keep M4 limited to implicit base LOD only.
- Q3: Reuse/refactor M3 texel load helpers instead of rewriting loads.
- Q4: Use tolerance-based filetest expectations for linear filtering.
- Q5: Include a mixed-axis wrap test.
- Q6: Use a builtin-first implementation for `texture()` sampling. Specialize
  primarily by texture storage format, maybe also by nearest vs linear. Keep
  wrap policy runtime inside the sampler helper/builtin for now. Keep M3
  `texelFetch` inline, and leave a future inline optimization path for cheap
  `texture()` cases after builtin semantics are proven.
- Q7: Use the conventional texel-center model for normalized coordinates:
  `coord = uv * extent - 0.5`; nearest chooses the closest texel center, linear
  uses floor/fraction around that coordinate, and wrap policy applies to integer
  texel coordinates.
- Q8: Avoid full specialization across format/filter/wrap axes. Specialize by
  texture storage format and dimensionality/shape (`General2D` vs `HeightOne`).
  Keep filter and wrap policy runtime inside the format/shape-specific sampler
  for now, with inline internal helper functions for nearest, linear, and wrap
  math.
- Q9: For `TextureShapeHint::HeightOne`, keep the public GLSL surface as
  `texture(sampler2D, vec2)`, but have frontend lowering select the 1D
  format-specialized builtin, pass only `uv.x` and `wrap_x`, and intentionally
  drop `uv.y` / `wrap_y`. Runtime validation remains responsible for enforcing
  `height == 1`.

### Q7: What exact normalized-coordinate convention should M4 implement?

GLSL `texture()` samples normalized coordinates. For nearest and linear, the
important choice is the texel-center convention: commonly `u * width - 0.5`
for linear footprint selection and nearest rounding to the closest texel center.
This affects edge behavior and test expectations.

Suggested answer: follow the conventional texel-center model: convert each axis
to continuous texel space with `coord = uv * extent - 0.5`, use floor/fraction
for linear neighbors, and use nearest behavior equivalent to selecting the
closest texel center. Apply wrap policy to integer texel coordinates after
neighbor selection.

### Q8: Should Rust reference code be test-only or shared production code?

The reference implementation is useful for tests and documentation, but putting
it in production runtime code can blur the boundary between compiler-emitted
shader semantics and host utilities. It may also pull in APIs that are not
needed on-device.

Suggested answer: start test-facing and `no_std`-friendly where practical. Keep
the functions pure over slices/descriptors/specs, colocated with filetest or
texture test utilities. If a later builtin path is chosen, promote or copy the
math deliberately into `lps-builtins` rather than having production lowering
depend on a test helper.

# Notes

- The main performance risk is not a single `texture()` call; it is repeated
  bilinear calls inside shader loops or generated render loops. The design
  should leave an obvious future path to inline cheap builtin cases or introduce
  a dedicated LPIR op without changing texture semantics.
- User direction on Q6: do more builtin research; ABI work appears mostly in
  place; use simple inline paths where pragmatic, but a compile-time-selected
  builtin suite may be the clearest first implementation.
- User direction on Q6 follow-up: VMContext is already a guest pointer and
  builtins have access to the same linear memory by design, so texture pointer
  support should be treated as a modest extension rather than a major obstacle.
- User settled Q6 as builtin-first with a balanced specialization strategy:
  specialize on format, maybe filter; keep the rest runtime for now.
- User accepted Q7 texel-center convention.
- User noted that M4 should avoid combinatoric explosion and that runtime
  selection cost for wrap/filter policy is acceptable when it is only a few
  instructions.
- User accepted the format-specialized, maybe filter-specialized, runtime-wrap
  compromise for M4.
- User remains on the fence about splitting nearest/linear and prefers
  format-only builtins if filter dispatch is just a few extra instructions and
  keeps builtin count/code size down.
- User added that `HeightOne` / 1D palette-gradient texture paths should be
  specialized for performance even though the shader/GPU-facing model remains
  `sampler2D`. Instinct: specialize on format + dimensionality.
- User confirmed Q9: special-case `HeightOne` in frontend lowering and drop the
  Y coordinate. This is slightly messier than true 1D texture support, but keeps
  the shader model aligned with practical GLSL/GPU usage and future wgpu LPFX.
- User accepted Q1-Q5 as all yes.
