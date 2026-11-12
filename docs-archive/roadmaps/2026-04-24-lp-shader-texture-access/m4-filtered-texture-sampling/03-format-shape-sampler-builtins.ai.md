# Scope of Phase

Implement the actual M4 texture sampler builtins, specialized by storage format
and dimensionality/shape.

The core target shape is:

- 2D builtins for `TextureShapeHint::General2D`;
- 1D builtins for `TextureShapeHint::HeightOne`;
- format specialization for at least `Rgba16Unorm` and `R16Unorm`;
- `Rgb16Unorm` support if it falls out cleanly from shared helpers;
- runtime `TextureFilter` selection inside each format/shape builtin;
- runtime wrap selection inside each format/shape builtin.

Out of scope:

- Frontend lowering from GLSL `texture()` to these builtins.
- Filetest GLSL coverage beyond unit tests or small direct builtin tests.
- New texture formats.
- Public palette/gradient APIs.

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

- `lp-shader/lps-builtins/src/builtins/texture/mod.rs`
- `lp-shader/lps-builtins/src/builtins/texture/sample_ref.rs`
- `lp-shader/lps-builtins/src/builtins/texture/rgba16_unorm_q32.rs`
- `lp-shader/lps-builtins/src/builtins/texture/r16_unorm_q32.rs`
- optional `lp-shader/lps-builtins/src/builtins/texture/rgb16_unorm_q32.rs`
- generated builtin files from phase 2

Implement builtins shaped like this, adjusting exact names/signatures to match
the generator conventions from phase 2:

```rust
#[unsafe(no_mangle)]
pub extern "C" fn __lp_texture2d_rgba16_unorm_q32(
    out: *mut i32,
    ptr: u32,
    width: u32,
    height: u32,
    row_stride: u32,
    u: i32,
    v: i32,
    filter: u32,
    wrap_x: u32,
    wrap_y: u32,
) {
    // write vec4 Q32 lanes to out
}

#[unsafe(no_mangle)]
pub extern "C" fn __lp_texture1d_rgba16_unorm_q32(
    out: *mut i32,
    ptr: u32,
    width: u32,
    row_stride: u32,
    u: i32,
    filter: u32,
    wrap_x: u32,
) {
    // write vec4 Q32 lanes to out; ignore height/Y by construction
}
```

The builtin receives descriptor lanes from `LpsTexture2DDescriptor`:

- `ptr`
- `width`
- `height` for 2D only
- `row_stride`

Use the existing runtime validation assumptions from M3c:

- width and height are nonzero;
- `HeightOne` bindings have runtime `height == 1`;
- pointer and row stride alignment are valid for current unorm16 formats;
- footprint fits the backing allocation when known.

Do not add broad defensive fallback behavior for invalid descriptors in the
builtins. Invalid bindings should be rejected before shader execution.

Sampler behavior:

- Use texel-center coordinates: `coord = uv * extent - 0.5`.
- For nearest, choose the closest texel center.
- For linear 2D, sample four neighboring texels and bilerp.
- For linear 1D, sample two neighboring texels and lerp.
- For `ClampToEdge`, clamp integer coordinates.
- For `Repeat`, use Euclidean modulo.
- For `MirrorRepeat`, mirror repeat periods.

Format behavior:

- `Rgba16Unorm`: load four u16 channels and convert each to Q32 float lanes.
- `R16Unorm`: load one u16 channel, fill G/B with `0.0`, fill A with `1.0`.
- `Rgb16Unorm`, if implemented: load three u16 channels, fill A with `1.0`.

Keep format-specific loads small and explicit. Shared generic helpers are fine
for coordinate/filter/wrap math, but avoid runtime format dispatch.

Use small internal inline helpers inside the builtin module for:

- filter ABI decode;
- wrap ABI decode;
- unorm16 load and conversion;
- nearest/linear sample bodies;
- 2D and 1D address calculation.

Safety:

- Minimize `unsafe` to the boundary where guest pointers are read/written.
- Keep pointer arithmetic localized in the format modules.
- Do not use `unwrap`/`expect` in production builtin paths.

Regenerate builtins after adding externs:

```bash
cargo run -p lps-builtins-gen-app
```

Tests to add:

- Unit tests comparing builtin helper output to `sample_ref` for small 1D and
  2D textures.
- R16 vec4 fill test.
- Height-one 1D test proving different Y values would not matter at the helper
  level, if the helper API exposes Y-free sampling.

# Validate

Run:

```bash
cargo run -p lps-builtins-gen-app
cargo test -p lps-builtins texture
cargo check -p lps-builtins
cargo check -p lps-builtin-ids
```

If the regenerated files affect backend crates, also run:

```bash
cargo check -p lpvm-cranelift
cargo check -p lpvm-wasm
```
