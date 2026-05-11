# Scope of Work

Implement the texture interface foundation from
`docs/roadmaps/2026-04-24-lp-shader-texture-access/m1-texture-interface-foundation.md`.

This milestone establishes shared contracts and validation only. It makes
texture uniforms visible, typed, and validated, but does not lower or execute
`texelFetch`, `texture`, or runtime sampling behavior.

In scope:

- Add shared texture binding vocabulary in `lps-shared`, near
  `TextureStorageFormat`: `TextureBindingSpec`, `TextureFilter`, `TextureWrap`,
  and `TextureShapeHint`.
- Add a logical `Texture2D`/`sampler2D` type to `LpsType`.
- Define the texture uniform ABI descriptor shape:
  `{ ptr: u32, width: u32, height: u32, row_stride: u32 }`.
- Add a typed runtime value/helper representation for texture uniforms, built
  from `LpsTextureBuf`.
- Replace or augment `LpsEngine::compile_px` with a named compile descriptor
  carrying source, output format, compiler config, and texture binding specs.
- Extend frontend metadata extraction so GLSL `sampler2D` uniforms are reported
  as logical texture uniforms and matched against binding specs.
- Add strict compile-time validation for missing specs, extra specs,
  unsupported source type, and unsupported sampler shape.

Out of scope:

- `texelFetch` or `texture` lowering.
- Runtime sampling behavior.
- Runtime texture binding validation beyond typed descriptor construction needed
  for this foundation.
- Texture fixture syntax in filetests beyond minimal compile-time validation
  support, if needed.
- WGSL source input.
- lpfx/domain schema changes.

# Current State

The existing design in `docs/design/lp-shader-texture-access.md` already fixes
the major product decisions: GLSL remains the v0 source surface, `Texture2D` is
a logical shader type, binding policy is supplied outside shader source by
sampler uniform name, validation is strict, and the texture ABI descriptor
includes `row_stride`.

Relevant code today:

- `lp-shader/lps-shared/src/texture_format.rs` defines
  `TextureStorageFormat::{Rgba16Unorm, Rgb16Unorm, R16Unorm}` plus
  `bytes_per_pixel()` and `channel_count()`. There is no binding-spec
  vocabulary yet.
- `lp-shader/lps-shared/src/types.rs` defines scalar, vector, matrix, array,
  and struct `LpsType` variants. It has no logical texture/sampler type.
- `lp-shader/lps-shared/src/layout.rs` computes std430 size/alignment for
  existing `LpsType` values. Texture descriptors will need a fixed 16-byte
  layout, not a public fake struct in metadata.
- `lp-shader/lps-shared/src/path_resolve.rs` resolves paths through structs,
  arrays, and swizzles. `Texture2D` should behave as a named leaf type.
- `lp-shader/lp-shader/src/texture_buf.rs` defines `LpsTextureBuf` with
  `guest_ptr()`, `width`, `height`, `format`, and tight `row_stride()`.
- `lp-shader/lp-shader/src/engine.rs` exposes positional
  `LpsEngine::compile_px(&str, TextureStorageFormat, &CompilerConfig)`.
  Internally it parses, lowers, validates `render(vec2 pos)`, synthesizes the
  output writer, and compiles with `LpvmEngine::compile_with_config`.
- `lp-shader/lps-frontend/src/naga_types.rs` maps Naga scalar/vector/matrix,
  array, and struct types to `LpsType`. Naga sampler/image types currently fall
  into `UnsupportedType`.
- `lp-shader/lps-frontend/src/lower.rs` builds `uniforms_type` from
  Naga globals in `AddressSpace::Uniform`. It uses `type_size` and
  `type_alignment` while computing VM context offsets. It currently has no
  concept of texture uniforms or texture binding validation.
- `lp-shader/lpvm/src/set_uniform.rs` encodes normal scalar/vector/matrix/
  aggregate uniform writes by resolving `LpsType` paths and matching
  `type_size`. Texture values need a typed helper or value path so callers do
  not author raw pointer structs.
- Existing tests are concentrated in `lp-shader/lps-shared` unit tests,
  `lp-shader/lps-frontend/src/lib.rs` metadata tests, and
  `lp-shader/lp-shader/src/tests.rs` compile/render-signature tests.

Constraints:

- Preserve `no_std` support across the shader compile/execute path.
- Do not gate compiler or texture-interface logic behind `std`.
- Keep metadata and diagnostics logical: report texture/sampler uniforms as
  textures, not as descriptor fields like `ptr` and `width`.
- Keep validation strict and fail-fast.
- Keep this milestone compile-only for textures; later milestones own sampling
  lowering and filetest fixture execution.

# Questions That Need To Be Answered

## Confirmation-Style Questions

| # | Question | Context | Suggested answer |
| --- | --- | --- | --- |
| Q1 | Should the named compile descriptor be `CompilePxDesc<'a>` in `lp-shader`? | The design uses this sketch and `compile_px` is specific to pixel shaders. | Yes. |
| Q2 | Should `compile_px` remain as a compatibility wrapper around a new descriptor-based method? | Current call sites are positional across `lp-engine`, `lpfx-cpu`, and tests. A wrapper keeps this foundation smaller. | Yes, add a new descriptor method and have existing `compile_px` call it with no textures. |
| Q3 | Should texture binding specs use `BTreeMap<String, TextureBindingSpec>`? | The codebase is `no_std + alloc`; deterministic ordering helps validation/tests. | Yes. |
| Q4 | Should `LpsType::Texture2D` have std430 size/alignment `16`/`4`? | The ABI descriptor is four `u32`s and should stay logical in metadata. | Yes. |
| Q5 | Should the runtime helper be a `Texture2DUniform`/`LpsTexture2DUniform` descriptor type in `lp-shader`, constructed from `&LpsTextureBuf`? | `LpsTextureBuf` owns guest pointer/dimensions/stride; callers should not hand-build raw pointer structs. | Yes, expose a small typed descriptor/helper and leave broader runtime binding APIs to later milestones. |
| Q6 | Should this milestone reject all texture shapes except GLSL `sampler2D`? | The roadmap explicitly excludes arrays, 3D, cubemaps, depth/comparison samplers, and WGSL. | Yes. |

Resolved answers:

- Q1: Yes. Use `CompilePxDesc<'a>` in `lp-shader`.
- Q2: Yes. Add a new descriptor-based method and keep `compile_px` as a
  compatibility wrapper with no texture specs.
- Q3: Yes. Use `BTreeMap<String, TextureBindingSpec>`.
- Q4: Yes. `LpsType::Texture2D` has std430 size/alignment `16`/`4`.
- Q5: Yes. Expose a small typed texture uniform descriptor/helper constructed
  from `&LpsTextureBuf`.
- Q6: Yes. Reject all texture shapes except GLSL `sampler2D` in this
  milestone.

## Discussion-Style Questions

No discussion-style questions are currently open. The design and milestone have
already settled the major architectural choices; the remaining items above are
mainly API naming and compatibility confirmations.

