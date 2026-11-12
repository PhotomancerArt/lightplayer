# Scope of Phase

Lower GLSL `texture(sampler2D, vec2)` calls to the format/shape-specialized
texture sampler builtins from earlier phases.

This phase should:

- recognize Naga's representation of GLSL `texture(sampler2D, vec2)`;
- resolve the sampler operand to a direct `Texture2D` uniform;
- validate there is a matching `TextureBindingSpec`;
- select a builtin by `TextureStorageFormat` and `TextureShapeHint`;
- pass runtime filter and wrap ABI values to the builtin;
- for `TextureShapeHint::HeightOne`, lower to the 1D builtin and intentionally
  drop `uv.y` and `wrap_y`.

Out of scope:

- Implementing sampler builtin internals.
- New texture formats.
- Mipmaps, derivatives, explicit LOD, `textureGrad`, or `textureLod`.
- Product-level texture routing.

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

- `lp-shader/lps-frontend/src/lower.rs`
- `lp-shader/lps-frontend/src/lower_expr.rs`
- `lp-shader/lps-frontend/src/lower_texture.rs`
- `lp-shader/lps-frontend/src/lower_ctx.rs`
- `lp-shader/lps-shared/src/texture_format.rs`
- `lp-shader/lpir/src/lpir_module.rs`

Start by confirming Naga's IR shape for GLSL `texture(sampler2D, vec2)`. Add a
small unit test near the existing `texelFetch` Naga-shape tests in
`lower_texture.rs`.

Expected lowering contract:

- Operand must resolve to a direct `Texture2D` sampler uniform, following the
  same strict M3a/M3b model as `texelFetch`.
- The sampler uniform name must have a matching `TextureBindingSpec`.
- Coordinate must be a `vec2`.
- Unsupported forms should fail with clear diagnostics:
  - explicit LOD;
  - dynamic or non-base LOD forms;
  - gradients;
  - array/layered/multisampled textures;
  - texture operand that cannot be resolved to a direct uniform;
  - no matching texture spec.

Import registration:

- Register texture sampler imports in `lower.rs` so `lower_texture.rs` can look
  up a `CalleeRef`.
- Keep the import namespace explicit, for example `texture::texture2d_rgba16_unorm`.
- The logical LPIR return type should be four `IrType::F32` lanes, even if the
  native extern uses a result pointer underneath.
- If the import uses `needs_vmctx`, pass `VMCTX_VREG` according to the
  established import convention. Prefer direct descriptor lanes if the builtin
  only needs texture memory and result pointer; use `needs_vmctx` only if the
  final ABI requires it.

Descriptor passing:

M3b already has helper logic in `lower_texture.rs` for loading descriptor lanes
from VMContext:

- ptr
- width
- height
- row_stride

Reuse/refactor those helpers rather than duplicating descriptor offset logic.
The 2D builtin call should receive all descriptor lanes. The 1D builtin call
should receive only the lanes it needs, but it is also acceptable to pass height
if that keeps ABI and implementation simpler. Do not use height in the 1D
sampler behavior.

Builtin selection:

```text
match (spec.format, spec.shape_hint) {
  (Rgba16Unorm, General2D) => texture2d_rgba16_unorm
  (Rgba16Unorm, HeightOne) => texture1d_rgba16_unorm
  (R16Unorm, General2D) => texture2d_r16_unorm
  (R16Unorm, HeightOne) => texture1d_r16_unorm
  (Rgb16Unorm, ...) => supported if phase 3 implemented it, otherwise diagnostic
}
```

For `General2D`:

- lower both `uv.x` and `uv.y`;
- pass `filter`, `wrap_x`, and `wrap_y`.

For `HeightOne`:

- lower only `uv.x` or lower the vec2 then use only lane 0 if that is simpler;
- pass `filter` and `wrap_x`;
- intentionally ignore/drop `uv.y` and `wrap_y`;
- rely on runtime validation from M3c for `height == 1`.

Tests to add:

- Naga shape test for `texture(sampler2D, vec2)`.
- Lowering test or printed-LPIR assertion proving a `General2D` RGBA binding
  calls the `texture2d` import.
- Lowering test proving a `HeightOne` binding calls the `texture1d` import and
  does not depend on `uv.y`.
- Negative tests for missing texture spec and unsupported operand shape if not
  already covered by filetests in later phases.

# Validate

Run:

```bash
cargo test -p lps-frontend texture
cargo check -p lps-frontend
```

If texture imports affect backend compile paths, also run:

```bash
cargo check -p lpvm-cranelift
cargo check -p lpvm-wasm
```
