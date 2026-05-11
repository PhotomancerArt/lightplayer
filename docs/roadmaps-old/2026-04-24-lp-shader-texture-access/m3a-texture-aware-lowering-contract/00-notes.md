# Scope of Work

Plan and implement Milestone 3a:
`docs/roadmaps/2026-04-24-lp-shader-texture-access/m3a-texture-aware-lowering-contract.md`.

This milestone is the frontend/control-plane slice of texture reads. It should
make `lps-frontend` aware of compile-time `TextureBindingSpec` metadata, verify
the Naga IR shape for GLSL `texelFetch(sampler2D, ivec2, lod)`, resolve texture
operands back to direct uniform sampler names, and produce intentional
diagnostics for unsupported forms.

In scope:

- Add a texture-spec-aware frontend lowering entry point.
- Keep the existing `lps_frontend::lower(&NagaModule)` API as a compatibility
  wrapper for texture-free callers.
- Confirm and document the Naga expression/statement shape emitted for
  `texelFetch`.
- Resolve supported texture operands back to direct uniform sampler names.
- Validate that the sampler uniform has a matching `TextureBindingSpec` at the
  point where `texelFetch` is lowered.
- Reject unsupported `texelFetch` forms with clear diagnostics:
  - nonzero literal LOD,
  - dynamic LOD,
  - non-`Texture2D` operand,
  - texture operand passed through a local/function parameter or otherwise not
    resolvable to a direct uniform.
- Add focused tests or filetests for the metadata and diagnostic contract.
- Decide whether compile-time texture specs should be retained in
  `LpsModuleSig` now for later runtime validation.

Out of scope:

- Full texel address calculation.
- Descriptor-field use beyond any minimal scaffolding needed to prove operand
  resolution.
- `Load16U`, `Unorm16toF`, row-stride math, and vec4 channel fill.
- Runtime validation of `LpsTextureBuf` values.
- Backend matrix filetests for exact sampled values.
- `texture(sampler2D, vec2)`, filtering, wrap modes, and mip/LOD support.

# Current State

Milestones 1 and 2 already provide the base vocabulary and filetest fixture
surface:

- `lps-shared` defines `TextureBindingSpec`, `TextureStorageFormat`,
  `TextureFilter`, `TextureWrap`, `TextureShapeHint`, `LpsType::Texture2D`,
  and `LpsTexture2DDescriptor`.
- `lps-shared::validate_texture_binding_specs_against_module` checks that
  lowered `Texture2D` uniforms match texture specs by name with no missing or
  extra specs.
- `lp-shader::CompilePxDesc` carries `textures: BTreeMap<String,
  TextureBindingSpec>` into `compile_px_desc`.
- `lps-filetests` parses `// texture-spec:` and `// texture-data:` directives,
  validates fixture/spec consistency, allocates shared memory, and binds
  `LpsValueF32::Texture2D` values via normal `set_uniform`.

Relevant frontend state:

- `lps-frontend` is `#![no_std]` plus `alloc`; this milestone must not add
  `std` to compile/lower paths.
- `lps_frontend::compile` rewrites supported top-level
  `uniform sampler2D name;` declarations into Naga-compatible texture globals.
- `sampler2d_metadata_tests.rs` documents the current metadata behavior:
  `uniform sampler2D` and explicit `texture2D` globals lower to
  `LpsType::Texture2D` in `LpsModuleSig::uniforms_type`.
- `lps_frontend::lower(&NagaModule)` currently has no parameter for texture
  specs. `lp-shader` and filetests validate texture specs only after lowering.
- `lps_frontend::lower::compute_global_layout` maps Naga
  `AddressSpace::Handle` globals to uniforms only when their type maps to
  `LpsType::Texture2D`.
- `lps_frontend::lower_ctx::LowerCtx` has `global_map` and
  `uniform_instance_locals`, but no texture-spec map or texture-specific helper.
- `lower_expr_vec_uncached` does not handle image load expressions today; it
  falls through to `UnsupportedExpression(format!("{:?}", expr))`.
- Existing M2 texture filetests avoid real `texelFetch` because it currently
  lowers to an unsupported expression.

Relevant metadata/runtime state:

- `LpsModuleSig` currently stores function metadata plus `uniforms_type` and
  `globals_type`; it does not retain texture specs.
- `LpsPxShader` also does not retain `CompilePxDesc::textures`, so later
  runtime validation cannot be implemented without storing specs somewhere.
- Filetests currently compile with `lps_frontend::lower(&naga)` and then call
  `validate_texture_binding_specs_against_module(&meta, texture_specs)`.
  M3a likely needs to move this texture-spec input earlier for files that use
  `texelFetch`.

Constraints:

- Preserve existing texture-free frontend callers.
- Keep texture specs compile-time metadata; do not introduce per-sample dynamic
  format dispatch.
- Keep descriptors opaque: diagnostics should name sampler uniforms, not ABI
  fields such as `ptr` or `row_stride`.
- Avoid broad runtime/API work in this milestone; leave data-path behavior for
  M3b and runtime/backend validation for M3c.

# Questions That Need To Be Answered

## Confirmation-Style Questions

| # | Question | Context | Suggested answer |
| --- | --- | --- | --- |
| Q1 | Name the new frontend entry `lower_with_texture_specs`? | Existing `lower(&NagaModule)` should stay as the simple texture-free wrapper. | Yes: add `lower_with_texture_specs(&NagaModule, &BTreeMap<String, TextureBindingSpec>)`, and have `lower()` call it with an empty map. |
| Q2 | Should M3a store texture specs in `LpsModuleSig` now? | M3c needs runtime validation; keeping specs in module metadata avoids a second parallel metadata path. | Yes: add a `texture_specs`/`textures` field that defaults empty and is populated by the spec-aware lower path after validation. |
| Q3 | Should `texelFetch` use existing `LowerError::UnsupportedExpression(String)` rather than adding new error variants? | Current diagnostics are string-based and wrapped with `InFunction`; filetests already match substrings. | Yes for M3a; use clear strings such as `texelFetch: dynamic lod is not supported`. |
| Q4 | Should M3a reject unresolved texture operands instead of trying to track texture aliases through locals/parameters? | The milestone's contract says direct uniform sampler names only; texture parameters were rejected in M1/M2. | Yes: only accept operands that resolve directly to a uniform `Texture2D` global. |
| Q5 | Should M3a lower supported `texelFetch` to a placeholder unsupported diagnostic after validating sampler/spec/LOD, rather than emitting partial fetch data path code? | M3b owns descriptor loads, address math, and channel conversion. | Yes: prove recognition and validation, then return a clear `texelFetch lowering data path is implemented in M3b` style error or test-only marker unless there is a cleaner minimal scaffold. |

## Discussion-Style Questions

## Q1: What should the default frontend lowering API be?

The initial suggestion was to keep `lower(&NagaModule)` as the texture-free
default and add `lower_with_texture_specs(...)`. User accepted Q2-Q5 but asked
to discuss Q1: most future users may want textures, so should the default API be
the texture-aware one?

Current considerations:

- Rust has no function overloading or optional parameters, so adding required
  parameters to `lower` means changing every call site.
- Rust libraries commonly keep a small convenience function for the zero-config
  case and add either `*_with_*` variants or an options/config struct for richer
  behavior.
- This crate already has an options-like concept at higher levels
  (`CompilePxDesc`). Texture access may not be the last lowering option, so a
  dedicated `lower_with_texture_specs` function could age worse than
  `lower_with_options`.
- `lps-frontend` is not the final user-facing texture API. Most consumers should
  probably go through `lp-shader::compile_px_desc`, where textures are already
  part of the main descriptor.

Suggested answer:

- Keep `lower(&NagaModule)` as the convenience default for no texture specs.
- Add a `LowerOptions` / `LowerDesc` struct with `texture_specs` and `Default`.
- Add `lower_with_options(&NagaModule, &LowerOptions)` as the texture-aware
  canonical frontend API.
- Update `lp-shader` and `lps-filetests` texture-aware paths to call
  `lower_with_options`, while pure tests can keep calling `lower`.

# Answers

- Q1: Use `LowerOptions` plus `lower_with_options(&NagaModule, &LowerOptions)`
  as the canonical texture-aware frontend API. Keep `lower(&NagaModule)` as the
  zero-config convenience wrapper. Avoid a narrowly named
  `lower_with_texture_specs` function so future lowering options have a natural
  home.
- Q2: Yes. Store texture specs in module metadata now for later runtime
  validation.
- Q3: Yes. Use existing string-based `LowerError` variants for M3a diagnostics.
- Q4: Yes. Reject texture operands that do not resolve directly to a uniform
  sampler.
- Q5: Yes. M3a should stop after proving/diagnosing the contract and leave real
  fetch data-path codegen to M3b.

