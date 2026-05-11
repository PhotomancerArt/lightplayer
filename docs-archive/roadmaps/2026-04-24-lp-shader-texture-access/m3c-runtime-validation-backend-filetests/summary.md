### What was built

- `LpsTexture2DValue` in `lps-shared`: guest `LpsTexture2DDescriptor` plus `TextureStorageFormat` and `byte_len` (`usize`) for footprint and layout checks; `from_guest_descriptor` for lanes-only rehydration (`byte_len == 0`, placeholder format).
- Format helpers on `TextureStorageFormat`: `bytes_per_pixel`, `channel_count`, `required_load_alignment`, and checked required-footprint math on the value.
- `LpsValueF32` / `LpsValueQ32` carry `Texture2D(LpsTexture2DValue)`; conversions and LPVM reject using `UVec4` where a `Texture2D` uniform is expected.
- LPVM ABI flattening and uniform writes emit exactly four lanes from the descriptor: `ptr`, `width`, `height`, `row_stride` (no format or `byte_len` in guest memory).
- `lp-shader` `runtime_texture_validation`: validates each runtime `sampler2D` bind against `TextureBindingSpec` (format, height-one hint, dimensions, alignment, stride, footprint vs `byte_len`) before `set_uniform`; clear `LpsError::Render` messages.
- `LpsTextureBuf::to_texture2d_value()` as the supported buffer-backed binding path; `px_shader` / `apply_uniforms` integration.
- `lps-filetests`: texture fixtures bind via `LpsTexture2DValue`; additional negative runtime/parse cases; `positive_minimal_fixture_design_doc.glsl` asserts real `texelFetch` results; comments note default backend matrix (`rv32n.q32`, `rv32c.q32`, `wasm.q32`).
- Unit/integration tests in `lp-shader`, `lpvm`, and `lps-shared` for typed texture round-trips and rejection of uvec4 stand-ins.

### Decisions for future reference

#### Host Texture Value vs Guest Descriptor ABI

- **Decision:** Keep the guest `LpsTexture2DDescriptor` ABI as four `u32` lanes (`ptr`, `width`, `height`, `row_stride`). Host-only fields (`format`, `byte_len`) participate in validation and embedding APIs but are not written into LPVM uniform storage.
- **Why:** Preserves std430 / marshaling shape; runtime validation can use allocation facts when bindings come from `LpsTextureBuf`, without widening the guest token.
- **Rejected alternatives:** Treating `LpsType::Texture2D` uniforms as a raw `uvec4` write path; encoding format or size in guest uniforms without an ABI change.
- **Revisit when:** A future ABI revision explicitly adds descriptor fields (would be a separate versioned change).

#### Format-Specific Texture Layout Alignment

- **Decision:** Encode rules per `TextureStorageFormat` via `required_load_alignment` and footprint math: non-zero size, `row_stride >= width * bytes_per_pixel`, pointer and stride aligned to the format’s load alignment, last-row footprint within `byte_len`. Current 16-bit unorm formats use 2-byte alignment; no requirement that `row_stride` equals tight packing.
- **Why:** Matches the M3b `texelFetch` load path and avoids baking a false “all textures even-byte stride” rule that would block future 8-bit or 32-bit channel layouts.
- **Rejected alternatives:** A single global stride or alignment rule for all formats; validating layout from descriptor bits alone when `byte_len == 0`.
- **Revisit when:** Adding formats with different channel widths (e.g. R8 vs 32-bit channels), adjusting `required_load_alignment` and validation accordingly.
