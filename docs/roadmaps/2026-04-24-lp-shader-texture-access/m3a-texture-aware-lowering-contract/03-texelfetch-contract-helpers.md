# Phase 3: TexelFetch Contract Helpers

## Scope of phase

Recognize Naga's `texelFetch` expression shape and add contract validation helpers, but intentionally stop before M3b data-path codegen.

In scope:

- Add `lp-shader/lps-frontend/src/lower_texture.rs`.
- Register the module from `lps-frontend/src/lib.rs`.
- Extend `LowerCtx` so texture helpers can access texture specs and global uniform metadata.
- Add a `lower_expr` dispatch arm for Naga image-load expressions emitted by GLSL `texelFetch`.
- Resolve only direct uniform `Texture2D` operands to sampler names.
- Validate matching texture spec and `lod == 0` literal.
- Reject dynamic/nonzero LOD and unresolved/non-texture operands with clear diagnostics.
- For an otherwise valid `texelFetch`, return a clear placeholder diagnostic that says M3b owns the data path.

Out of scope:

- Do not emit descriptor loads, `Load16U`, `Unorm16toF`, row-stride math, or vec4 results.
- Do not implement bounds policy.
- Do not support texture aliases, texture function parameters, locals, arrays of textures, or non-uniform texture operands.
- Do not implement `texture(sampler2D, vec2)`.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within "Scope of phase".
- Do not suppress warnings or `#[allow(...)]` problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If Naga's expression shape differs from this plan and requires a design change, stop and report rather than improvising broad changes.
- Report back what changed, what was validated, and any deviations from this phase plan.

## Implementation Details

Read these files first:

- `lp-shader/lps-frontend/src/lower_expr.rs`
- `lp-shader/lps-frontend/src/lower_ctx.rs`
- `lp-shader/lps-frontend/src/lower.rs`
- `lp-shader/lps-frontend/src/sampler2d_metadata_tests.rs`
- `lp-shader/lps-frontend/src/naga_types.rs`
- `docs/roadmaps/2026-04-24-lp-shader-texture-access/m3a-texture-aware-lowering-contract/00-design.md`

This phase is supervised because the exact Naga IR shape is the main uncertainty.

### Confirm Naga shape

Add temporary local tests or a permanent small test that compiles a GLSL snippet like:

```glsl
uniform sampler2D inputColor;
vec4 render(vec2 pos) {
    return texelFetch(inputColor, ivec2(0, 0), 0);
}
```

Inspect the Naga expression shape. Based on earlier exploration, it is expected to be an image-load expression. Use the actual enum variant names from the installed Naga version.

Do not leave debug prints or scratch code behind.

### Extend `LowerCtx`

Add texture context fields to `LowerCtx`, likely:

```rust
pub(crate) texture_specs: BTreeMap<String, TextureBindingSpec>,
```

or a borrowed equivalent if the lifetime is straightforward. Keep this compatible with `no_std + alloc`.

Pass the texture specs from `lower_with_options` through `lower_function` into `LowerCtx::new`.

### `lower_texture.rs`

Create a new module with helper functions such as:

```rust
pub(crate) fn lower_image_load_texel_fetch(
    ctx: &mut LowerCtx<'_>,
    expr: Handle<Expression>,
) -> Result<VRegVec, LowerError> {
    ...
}
```

Use names that match the actual Naga shape.

The helper should:

1. Extract the image/texture operand, coordinate operand, and LOD/sample/level operand from the Naga expression.
2. Resolve the texture operand to a direct `Expression::GlobalVariable(gv)` or equivalent direct uniform form.
3. Confirm the global maps to `LpsType::Texture2D` and is uniform/handle-backed.
4. Get the sampler name from `ctx.module.global_variables[gv].name`.
5. Confirm `ctx.texture_specs` contains that sampler name.
6. Validate the LOD:
   - literal `0` is accepted,
   - literal nonzero is rejected with a message containing `texelFetch`, `lod`, and the value,
   - any dynamic/non-literal LOD is rejected with a message containing `texelFetch` and `dynamic lod`.
7. For otherwise valid calls, return:
   ```rust
   Err(LowerError::UnsupportedExpression(format!(
       "texelFetch for texture uniform `{name}` recognized; data path is implemented in M3b"
   )))
   ```

The exact placeholder text can differ, but it must be clear and testable.

Coordinate validation is limited in M3a. You may confirm coordinate expression shape if convenient, but do not implement coordinate math.

### `lower_expr.rs`

Add a dispatch arm before the final unsupported fallback for the actual Naga image-load expression. It should delegate to `lower_texture`.

Keep the final generic unsupported fallback for other image operations and expressions.

### Tests

Add tests in `lps-frontend` that call `lower_with_options` and assert error substrings for:

- Valid direct sampler + matching spec + literal zero LOD reaches the M3b placeholder diagnostic.
- Literal nonzero LOD is rejected.
- Dynamic LOD is rejected.
- Missing texture spec is rejected before/at texture lowering with the sampler name.

If Naga cannot parse some unsupported forms, test the parse error only if it is already covered elsewhere; do not force unnatural tests.

## Validate

Run:

```bash
cargo test -p lps-frontend
```

Also run:

```bash
cargo test -p lps-filetests
```

if this phase touches shared diagnostics that filetests depend on. Report exact commands and results.

