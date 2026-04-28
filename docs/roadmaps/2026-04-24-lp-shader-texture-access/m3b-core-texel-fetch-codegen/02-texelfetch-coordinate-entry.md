# Phase 2: Wire `texelFetch` Coordinate Data Path Entry

## Scope of phase

Wire Naga `Expression::ImageLoad` coordinate data into texture-specific
lowering so `lower_texture.rs` owns the complete `texelFetch` input shape:
image operand, coordinate operand, and LOD operand.

In scope:

- Change `lower_expr.rs` to pass the `coordinate` expression handle into
  `lower_image_load_texel_fetch`.
- Change the signature of `lower_image_load_texel_fetch` in
  `lower_texture.rs`.
- Add coordinate validation/lowering scaffolding for `ivec2` coordinates.
- Preserve all M3a diagnostics for unsupported multisample, array/layer,
  missing LOD, missing spec, nonzero LOD, and dynamic LOD.
- Keep the M3b placeholder diagnostic for otherwise valid `texelFetch` until
  later phases implement the data path.

Out of scope:

- Do not implement descriptor loads, clamp generation, address math, or channel
  loads.
- Do not convert the placeholder filetest yet.
- Do not alter backend code.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within "Scope of phase".
- Do not suppress warnings or `#[allow(...)]` problems away. Fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If something blocks completion, stop and report rather than improvising.
- Report back: what changed, what was validated, and any deviations.

## Implementation Details

Read:

- `lp-shader/lps-frontend/src/lower_expr.rs`
- `lp-shader/lps-frontend/src/lower_texture.rs`
- `lp-shader/lps-frontend/src/naga_util.rs` if you need type inspection helpers
- `lp-shader/lps-frontend/src/sampler2d_metadata_tests.rs`

In `lower_expr.rs`, the `Expression::ImageLoad` arm currently passes only image
and level:

```rust
crate::lower_texture::lower_image_load_texel_fetch(ctx, *image, *level_expr)
```

Change it to pass coordinate as well:

```rust
crate::lower_texture::lower_image_load_texel_fetch(
    ctx,
    *image,
    *coordinate,
    *level_expr,
)
```

Update `lower_texture.rs`:

```rust
pub(crate) fn lower_image_load_texel_fetch(
    ctx: &mut LowerCtx<'_>,
    image_expr: Handle<Expression>,
    coordinate_expr: Handle<Expression>,
    level_expr: Handle<Expression>,
) -> Result<VRegVec, LowerError>
```

Add a helper that lowers and validates the coordinate expression:

```rust
struct TexelFetchCoords {
    x: VReg,
    y: VReg,
}
```

Suggested behavior for the helper:

- Call `ctx.ensure_expr_vec(coordinate_expr)` to lower the coordinate.
- Require exactly two lanes; otherwise return
  `LowerError::UnsupportedExpression("texelFetch: coordinate must be ivec2")`
  or similarly clear text.
- If type inspection is straightforward, also verify the coordinate expression
  has signed integer scalar kind (`ivec2`), not `vec2`/`uvec2`. If type
  inspection is awkward, rely on Naga type checking and lane count for this
  phase, but do not overcomplicate it.
- Return the two coordinate `VReg`s.

Call the helper after sampler/spec/LOD validation so diagnostics stay focused:

1. Resolve direct sampler uniform.
2. Validate matching texture spec.
3. Validate literal `lod == 0`.
4. Validate/lower coordinates.
5. Return the existing M3b placeholder diagnostic.

Make sure all existing M3a tests still pass. If a test asserts the exact
placeholder text, keep that text unchanged:

```text
texelFetch for texture uniform `inputColor` recognized; data path is implemented in M3b
```

Add or update one narrow frontend test if useful to prove non-`ivec2`
coordinates produce a clear lowering error, but avoid broad new behavior tests
until phase 5.

## Validate

Run from workspace root:

```bash
cargo test -p lps-frontend sampler2d_metadata_tests
cargo check -p lps-frontend
```

