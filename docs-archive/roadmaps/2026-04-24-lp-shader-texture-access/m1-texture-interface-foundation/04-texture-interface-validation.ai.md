# Scope of Phase

Validate descriptor texture binding specs against shader-declared texture
uniforms during descriptor-based pixel shader compilation.

Depends on phases 1, 2, and 3.

In scope:

- Extract declared `Texture2D` uniform names from `LpsModuleSig`.
- Validate `CompilePxDesc::textures` against those declarations.
- Error on shader sampler with no spec.
- Error on spec naming a nonexistent sampler.
- Error on unsupported texture/source shape if it reaches this layer.
- Add compile-level tests for positive and negative validation cases.

Out of scope:

- Runtime texture binding validation.
- Runtime sampling and texture read lowering.
- Filetest fixture parser changes.
- lpfx/domain integration.

# Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

# Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within "Scope of Phase".
- Do not suppress warnings or `#[allow(...)]` problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If something blocks completion, stop and report back rather than improvising.
- Report back: what changed, what was validated, and any deviations from this
  phase plan.

# Implementation Details

Relevant files:

- `lp-shader/lp-shader/src/engine.rs`
- `lp-shader/lp-shader/src/compile_px_desc.rs`
- `lp-shader/lp-shader/src/tests.rs`
- `lp-shader/lps-shared/src/types.rs`
- potentially a new helper module in `lp-shader/lp-shader/src/`

Add validation after frontend lowering and before render signature validation or
backend compile in the descriptor-based compile method. The exact order can be:

1. Parse GLSL.
2. Lower to `(ir, meta)`.
3. Validate texture interface using `meta.uniforms_type` and
   `CompilePxDesc::textures`.
4. Validate `render(vec2 pos)`.
5. Synthesize output writer.
6. Compile with LPVM.

This keeps texture contract errors close to metadata extraction and prevents the
backend from seeing intentionally unsupported texture calls in this milestone.

Validation rules:

- Walk top-level members of `meta.uniforms_type`.
- Declared texture uniforms are top-level members where `member.ty ==
  LpsType::Texture2D`.
- If a declared texture uniform name is missing from `desc.textures`, return
  `LpsError::Validation` with a message naming the sampler.
- If `desc.textures` contains a name that is not a declared texture uniform,
  return `LpsError::Validation` with a message naming the spec.
- If a top-level texture uniform has no name, return a validation error.
- Do not require specs when the shader declares no texture uniforms and the
  texture map is empty.
- Continue to accept texture-free shaders through both the descriptor path and
  the compatibility wrapper.

Use deterministic `BTreeMap` ordering for stable error messages. Do not silently
ignore extra specs.

Tests to add in `lp-shader/src/tests.rs`:

- Descriptor compile succeeds for a shader that declares `uniform sampler2D
  inputColor;` and includes an `inputColor` spec, provided the shader does not
  call texture reads.
- Descriptor compile fails when the shader declares `inputColor` but the spec
  map is empty.
- Descriptor compile fails when the spec map contains `inputColor` but the
  shader has no texture uniform by that name.
- Texture-free shader still compiles through old `compile_px` wrapper.
- Error messages mention the offending sampler/spec names.

Use a small helper in tests to build a default `TextureBindingSpec`, for example
`Rgba16Unorm`, `Nearest`, `ClampToEdge`, `ClampToEdge`, `General2D`.

# Validate

Run from the workspace root:

```bash
cargo test -p lp-shader
cargo check -p lp-shader
```

