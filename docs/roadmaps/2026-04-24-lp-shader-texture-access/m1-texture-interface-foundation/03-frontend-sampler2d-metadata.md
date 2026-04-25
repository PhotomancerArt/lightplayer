# Scope of Phase

Teach the frontend metadata path to recognize GLSL `sampler2D` uniforms as
logical `LpsType::Texture2D`.

Depends on phase 1 shared `LpsType::Texture2D`.

In scope:

- Map supported Naga representation for GLSL `sampler2D` uniforms to
  `LpsType::Texture2D`.
- Include texture uniforms in `LpsModuleSig::uniforms_type` metadata.
- Ensure uniform/global offset computation handles `Texture2D` layout.
- Reject unsupported texture/source shapes clearly.
- Add frontend tests for sampler metadata and unsupported shapes.

Out of scope:

- Compile descriptor texture spec validation.
- Lowering texture read expressions (`texelFetch`, `texture`).
- Runtime texture sampling or binding.
- Filetest fixture syntax.

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

- `lp-shader/lps-frontend/src/naga_types.rs`
- `lp-shader/lps-frontend/src/lower.rs`
- `lp-shader/lps-frontend/src/lib.rs`
- potentially `lp-shader/lps-frontend/src/lower_error.rs`

Current behavior:

- `naga_type_handle_to_lps` delegates most non-struct types to
  `naga_type_inner_to_glsl`.
- `naga_type_inner_to_glsl` supports scalar/vector/matrix/array and rejects
  other `TypeInner` values with `CompileError::UnsupportedType`.
- `lower.rs::compute_global_layout` walks global variables in
  `AddressSpace::Uniform` and `AddressSpace::Private`, maps each type to
  `LpsType`, and computes VM context offsets using `type_size` and
  `type_alignment`.

Implementation notes:

- Inspect how Naga represents GLSL `uniform sampler2D inputColor;` in this
  version. It may be a sampler/image related `TypeInner` plus global metadata,
  or a separate image/sampler pair depending on Naga's GLSL frontend.
- Support exactly GLSL `sampler2D` as `LpsType::Texture2D`.
- Reject texture arrays, 3D, cubemaps, depth/comparison samplers, storage
  images, and other non-2D sampler/image forms.
- Keep errors explicit enough for phase 4 compile validation tests to assert
  useful substrings.
- Do not lower texture read expressions in this phase. If a shader declares a
  sampler uniform but does not read it, lowering should succeed and metadata
  should show the uniform. If a shader calls `texelFetch`/`texture`, it may
  still fail until later milestones.
- Update `lps_scalar_component_count` in `lower.rs` for `Texture2D`; use `4`
  because the ABI descriptor is four 32-bit words.

Tests to add in `lps-frontend`:

- GLSL containing `uniform sampler2D inputColor; vec4 render(vec2 pos) { return
  vec4(pos, 0.0, 1.0); }` compiles/lowers, and `uniforms_type` contains
  `inputColor: LpsType::Texture2D`.
- A texture uniform plus a normal scalar/vector uniform produces stable metadata
  and layout.
- Unsupported texture shape examples, if Naga can parse them, return a clear
  unsupported-type/lower error. Keep these tests narrow and do not fight Naga if
  it rejects the source earlier than the type mapper.

# Validate

Run from the workspace root:

```bash
cargo test -p lps-frontend
cargo check -p lps-frontend
```

