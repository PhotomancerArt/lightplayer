# Scope Of Phase

Implement the typed host texture value and format-specific layout helpers from
the M3c design.

This is a supervised mechanical API migration. It should keep the existing
guest ABI unchanged while making `LpsValueF32::Texture2D` /
`LpsValueQ32::Texture2D` carry enough host metadata for later runtime
validation.

In scope:

- Add a host-side texture value type in `lp-shader/lps-shared`.
- Preserve `LpsTexture2DDescriptor { ptr, width, height, row_stride }` exactly
  as the four-lane guest ABI token.
- Add format-specific layout helper(s), including required load alignment and
  checked footprint math.
- Change `LpsValueF32::Texture2D` and `LpsValueQ32::Texture2D` to carry the new
  typed host value.
- Update LPVM ABI flattening and uniform writes to write only descriptor lanes.
- Update existing call sites/tests enough for the repository to compile.
- Add focused unit tests for layout helper behavior and ABI flattening.

Out of scope:

- Public `render_frame` validation against `TextureBindingSpec`; that is phase 2.
- New texture formats.
- Changing descriptor ABI layout or descriptor lane order.
- New sampling semantics or filetest behavior changes beyond mechanical compile
  fixes required by the new value type.

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

- `docs/roadmaps/2026-04-24-lp-shader-texture-access/m3c-runtime-validation-backend-filetests/00-notes.md`
- `docs/roadmaps/2026-04-24-lp-shader-texture-access/m3c-runtime-validation-backend-filetests/00-design.md`
- `lp-shader/lps-shared/src/texture_format.rs`
- `lp-shader/lps-shared/src/lps_value_f32.rs`
- `lp-shader/lps-shared/src/lps_value_q32.rs`
- `lp-shader/lp-shader/src/texture_buf.rs`
- `lp-shader/lpvm/src/lpvm_abi.rs`
- `lp-shader/lpvm/src/set_uniform.rs`
- `lp-shader/lpvm/src/lpvm_data_q32.rs`

Add a host-side texture value near `LpsTexture2DDescriptor`, for example:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LpsTexture2DValue {
    pub descriptor: LpsTexture2DDescriptor,
    pub format: TextureStorageFormat,
    pub byte_len: u32,
}
```

Use field names that make the design obvious. If `usize` is materially easier
for `byte_len` inside this codebase, that is acceptable, but avoid platform-
dependent ABI assumptions: only the descriptor is guest ABI.

Add helpers:

- `TextureStorageFormat::required_load_alignment(self) -> usize`
  - Return `2` for `R16Unorm`, `Rgb16Unorm`, and `Rgba16Unorm`.
  - Document that future `R8`-style formats should be able to return `1`.
- A checked footprint helper, either on `LpsTexture2DValue` or as a shared
  helper. It should compute the bytes needed for padded rows:
  `row_stride * (height - 1) + width * bytes_per_pixel`.
  It should return `None` / `Result` on overflow.

Update values:

- `LpsValueF32::Texture2D` should carry `LpsTexture2DValue`.
- `LpsValueQ32::Texture2D` should carry `LpsTexture2DValue`.
- `lps_value_f32_to_q32` should preserve the value for `LpsType::Texture2D`.
- Any Q32-to-F32 conversion should preserve the value as well.
- Error text should still make clear that raw `UVec4` is not a texture stand-in.

Update `LpsTextureBuf`:

- Keep `to_texture2d_descriptor()` if needed for low-level ABI/debug use.
- Add `to_texture2d_value()` returning the new host value with:
  - descriptor from `to_texture2d_descriptor()`
  - format from `self.format()`
  - byte_len from backing buffer size
- Keep allocation unchanged for now: current `alloc_texture` uses 4-byte
  alignment and tight rows, which satisfies current 16-bit formats.

Update LPVM ABI flattening:

- Wherever `LpsValueF32::Texture2D(desc)` / `LpsValueQ32::Texture2D(desc)` is
  flattened, write `value.descriptor.ptr`, `width`, `height`, `row_stride`.
- Do not write `format` or `byte_len` into VM memory.
- Do not alter `LpsTexture2DDescriptor` size or field order.

Update call sites enough to compile:

- Filetest fixture binding may temporarily construct `LpsTexture2DValue`
  directly from the encoded fixture descriptor, format, and byte length.
- Existing unit tests that construct descriptors should either call
  `to_texture2d_value()` or explicitly construct the new value when testing ABI
  flattening.

Tests to add/update:

- `TextureStorageFormat::required_load_alignment()` returns `2` for all current
  formats.
- Checked footprint math accepts padded rows and rejects overflow.
- `LpsTextureBuf::to_texture2d_value()` preserves descriptor lanes, format, and
  byte length.
- LPVM uniform write tests prove only descriptor lanes are written, and raw
  `UVec4` remains rejected for `Texture2D`.

# Validate

Run:

```bash
cargo test -p lps-shared
cargo test -p lpvm
cargo test -p lp-shader
```

If one of these package names differs, stop and report the exact package-name
issue rather than guessing broadly.
