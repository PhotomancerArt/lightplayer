# lp-shader Texture Access — Decisions

#### GLSL v0 with WGSL-shaped internals

- **Decision:** Keep GLSL as the v0 source language, but model texture access as
  logical texture resources plus sampler policy.
- **Why:** GLSL matches existing examples and model familiarity; WGSL/wgpu has
  the better resource/sampler model and should shape future compatibility.
- **Rejected alternatives:** Switch wholesale to WGSL now (too much migration);
  encode sampler policy in GLSL layout qualifiers or LP-specific function names
  (creates a dialect).
- **Revisit when:** Real wgpu support lands and WGSL source input becomes part
  of the user-facing workflow.

#### Texture binding specs are outside shader source

- **Decision:** Compile texture policy from a `TextureBindingSpec` map keyed by
  sampler uniform name.
- **Why:** lpfx/domain own resource routing and context; `lp-shader` only needs
  shader-relevant policy for lowering and validation.
- **Rejected alternatives:** Infer everything from naming conventions; require
  policy to be written in shader source.

#### Texture2D is logical, descriptor is ABI

- **Decision:** Add logical `Texture2D`/`sampler2D` support in shared metadata;
  lower it to a uniform descriptor ABI.
- **Why:** Keeps diagnostics and future WGSL mapping clean while preserving a
  simple CPU/RV32 representation.
- **Rejected alternatives:** Treat sampler uniforms as user-visible structs with
  `ptr`, `width`, and `height` fields.

#### Strict validation

- **Decision:** Missing/extra specs, runtime binding mismatches, wrong formats,
  and broken shape hints are hard errors.
- **Why:** Fail-fast diagnostics are better while the contract is new; permissive
  fallbacks would hide bad domain/runtime wiring.
- **Rejected alternatives:** Ignore unknown specs; silently fall back when
  `HeightOne` or format promises are violated.

#### Foundation-first implementation

- **Decision:** Implement the interface and `texelFetch` before filtered
  sampling, wrap modes, and palette helpers.
- **Why:** `texelFetch` proves descriptor validation, uniform ABI, fixture
  allocation, format conversion, and backend execution with minimal semantics.
- **Rejected alternatives:** Start with palettes because they are product-visible.

#### Texture resources are 2D with height-one optimization

- **Decision:** V0 supports 2D texture resources only; palettes/gradients use
  width-by-one textures plus a `HeightOne` hint.
- **Why:** This matches GLSL ES/WebGL practice, keeps one resource type, and
  still enables cheaper palette lookup.
- **Rejected alternatives:** Add distinct 1D texture resources now; add 3D
  textures for future color LUTs before there is a concrete need.

#### Filetests are the validation backbone

- **Decision:** Extend `lps-filetests` with texture specs and inline
  pixel-grouped fixtures.
- **Why:** Filetests already validate the compiler/backends; texture fixtures can
  later be reused for wgpu comparison.
- **Rejected alternatives:** Only add `lp-shader` API tests; rely on large image
  sidecars from the start.

#### Filtered sampling format matrix (shipped)

- **Decision:** `texelFetch` supports `R16Unorm`, `Rgb16Unorm`, and `Rgba16Unorm`;
  filtered `texture()` supports `R16Unorm` and `Rgba16Unorm` only. `Rgb16Unorm`
  filtered sampling is rejected at lowering until a dedicated path exists.
- **Why:** Keeps filtered builtins aligned with implemented format lowering;
  RGB16 texel fetch remains useful without committing to filtered RGB16 yet.
- **Rejected alternatives:** Silently promote RGB16 to RGBA at sample time;
  claim parity with a wgpu backend that does not exist yet.

