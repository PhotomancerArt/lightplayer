# Scope of Phase

Add negative diagnostics and unsupported-combination coverage for M4
`texture(sampler2D, vec2)` sampling.

This phase should ensure unsupported forms fail at compile/lowering time with
clear, actionable messages rather than falling through to backend errors.

Out of scope:

- Positive sampling behavior tests; that is phase 5.
- Implementing new sampler builtins.
- Adding support for mipmaps, derivatives, explicit LOD, or new texture formats.

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
- Report back: what changed, what was validated, and any deviations from this phase.

# Implementation Details

Relevant files:

- `lp-shader/lps-frontend/src/lower_texture.rs`
- `lp-shader/lps-frontend/src/lower_expr.rs`
- `lp-shader/lps-filetests/filetests/textures/`
- `lp-shader/lps-filetests/src/parse/`

Add diagnostics for unsupported M4 texture sampling cases:

- `texture()` with missing `TextureBindingSpec`.
- `texture()` where operand is not a direct `Texture2D` sampler uniform.
- `texture()` coordinate is not `vec2`.
- Naga sampled-image forms with unsupported array/layer/multisample fields.
- Explicit LOD / gradient / derivative forms if Naga represents them in the
  same lowering path.
- Format/shape combinations deliberately deferred from M4 v0, for example
  `Rgb16Unorm` if phase 3 did not implement it.
- Any builtin import selection failure should mention sampler name, format, and
  shape hint if known.

Diagnostics should be specific enough that users can fix shader source or
texture specs. Prefer messages like:

```text
texture `inputColor`: no texture binding spec for sampler uniform `inputColor`
texture `palette`: unsupported format Rgb16Unorm for filtered sampling
texture `inputColor`: coordinate must be vec2
texture `inputColor`: explicit LOD/gradient sampling is not supported
```

Add negative filetests under `lp-shader/lps-filetests/filetests/textures/`.
Suggested files:

```text
error_texture_missing_spec.glsl
error_texture_bad_coordinate.glsl
error_texture_operand_not_direct_uniform.glsl
error_texture_explicit_lod.glsl
error_texture_unsupported_format.glsl       # only if a format is deferred
```

If some unsupported forms are easier to assert in `lps-frontend` unit tests than
filetests, add focused unit tests instead, but keep at least a few user-facing
filetests for diagnostics that matter to shader authors.

Do not weaken existing M3a/M3b `texelFetch` diagnostics while adding `texture()`
diagnostics. Keep function names in messages clear (`texture` vs `texelFetch`).

# Validate

Run:

```bash
cargo test -p lps-frontend texture
cargo test -p lps-filetests textures
```

If the filetest harness does not support the `textures` filter, run:

```bash
cargo test -p lps-filetests
```

Report exact commands and any target-specific skips.
