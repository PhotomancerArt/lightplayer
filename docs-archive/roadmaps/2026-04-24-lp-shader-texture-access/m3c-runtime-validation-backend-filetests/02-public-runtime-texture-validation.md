# Scope Of Phase

Add public `lp-shader` runtime validation for texture uniforms before shader
execution.

In scope:

- Add `lp-shader/lp-shader/src/runtime_texture_validation.rs`.
- Validate runtime `LpsValueF32::Texture2D` values against
  `LpsModuleSig::texture_specs` / `TextureBindingSpec` in
  `LpsPxShader::apply_uniforms`.
- Validate current M3c layout invariants for typed texture values:
  format, dimensions, `HeightOne`, pointer alignment, row-stride minimum,
  row-stride alignment, and footprint within byte length.
- Add public-path tests in `lp-shader/lp-shader/src/tests.rs`.

Out of scope:

- Changing the host texture value shape from phase 1.
- Filetest harness changes; those are phase 3.
- New formats or sampling behavior.
- Probing guest memory unsafely to prove pointer validity.

# Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

# Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within "Scope Of Phase".
- Do not suppress warnings or `#[allow(...)]` problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If something blocks completion, stop and report back rather than improvising.
- Report back: what changed, what was validated, and any deviations from the
  phase plan.

# Implementation Details

Read these first:

- `00-notes.md` and `00-design.md` in this plan directory.
- `lp-shader/lp-shader/src/px_shader.rs`
- `lp-shader/lp-shader/src/texture_buf.rs`
- `lp-shader/lp-shader/src/tests.rs`
- `lp-shader/lps-shared/src/texture_format.rs`
- `lp-shader/lps-shared/src/sig.rs`

Add a new module:

```rust
mod runtime_texture_validation;
```

in `lp-shader/lp-shader/src/lib.rs` or wherever crate-private modules are
declared.

The module should expose a crate-private function with a shape similar to:

```rust
pub(crate) fn validate_runtime_texture_binding(
    name: &str,
    value: &LpsTexture2DValue,
    spec: &TextureBindingSpec,
) -> Result<(), LpsError>
```

Validation rules:

- `value.format == spec.format`
- `descriptor.width > 0`
- `descriptor.height > 0`
- if `spec.shape_hint == TextureShapeHint::HeightOne`, then
  `descriptor.height == 1`
- `descriptor.ptr` aligned to `spec.format.required_load_alignment()`
- `descriptor.row_stride >= descriptor.width * spec.format.bytes_per_pixel()`
  using checked arithmetic
- `descriptor.row_stride` aligned to `required_load_alignment`
- padded-row footprint fits in `value.byte_len`
  - Use `row_stride * (height - 1) + width * bytes_per_pixel`
  - Use checked arithmetic

Error messages should be specific and include the texture uniform name. They
should be user-facing `LpsError::Render` errors because validation happens
during `render_frame`.

Wire into `LpsPxShader::apply_uniforms`:

- The existing code iterates declared uniform members and finds the matching
  field value.
- For members whose `ty` is `LpsType::Texture2D`, require the matching
  `LpsValueF32::Texture2D(value)` and look up `self.meta.texture_specs[name]`.
- If no spec is present for a texture uniform, return a clear render error; this
  should not normally happen after compile-time validation, but it protects the
  public runtime path.
- Call `validate_runtime_texture_binding` before `inner.set_uniform`.
- For non-texture uniforms, keep existing behavior.

Tests in `lp-shader/lp-shader/src/tests.rs`:

- Positive: compile a shader with a `sampler2D`, bind an `LpsTextureBuf` via
  the typed value helper, and render successfully.
- Missing runtime texture field: `render_frame` returns a render error naming
  the missing texture uniform.
- Wrong runtime value type for a texture uniform: `render_frame` returns a
  clear render error before backend execution.
- Format mismatch: compile spec expects one format, bind a typed value carrying
  a different format, and assert the error mentions format mismatch.
- `HeightOne` mismatch: spec uses `shape=height-one`, bind height > 1, assert
  the error mentions height-one / expected height 1.
- Bad row stride / alignment / footprint: manually construct a typed texture
  value with malformed descriptor metadata and assert targeted errors.

Use existing test helpers/patterns in `tests.rs` for `CompilePxDesc` and
`TextureBindingSpec`.

# Validate

Run:

```bash
cargo test -p lp-shader
```
