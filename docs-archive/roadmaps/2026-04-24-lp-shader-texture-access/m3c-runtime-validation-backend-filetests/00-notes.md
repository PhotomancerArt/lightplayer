# Scope Of Work

Plan Milestone 3c: runtime validation and backend filetests for texture reads.

The milestone should make the M3b `texelFetch` implementation merge-ready by:

- Validating public `lp-shader` runtime texture bindings before shader execution.
- Keeping the runtime binding surface typed and self-describing enough for validation, without returning to raw `UVec4` descriptor writes.
- Preserving the guest ABI descriptor as the existing four lanes: `ptr`, `width`, `height`, `row_stride`.
- Defining format-specific texture layout invariants now, while only implementing the currently supported 16-bit formats.
- Proving exact `texelFetch` behavior across the current Q32 backend matrix used by filetests: `rv32n.q32`, `rv32c.q32`, and `wasm.q32`.
- Covering runtime negative cases that are not already covered by compile-time or parser-level M2/M3a/M3b tests.
- Replacing the design-doc-only fixture smoke with a real `texelFetch` behavior test.

Out of scope:

- New sampling semantics, filtering, wrap behavior, mipmaps, or `texture()`.
- Product-level lpfx/lp-domain texture routing.
- wgpu execution parity.
- New texture formats.
- Changing the guest `LpsTexture2DDescriptor` ABI shape.
- Emulator ISA-profile gating; that remains a separate follow-up from the Load16 alignment report.

# Current State

- M3b has completed core `texelFetch(sampler2D, ivec2, 0)` lowering in `lp-shader/lps-frontend/src/lower_texture.rs`.
- Compile-time texture specs are retained in `LpsModuleSig::texture_specs` and are available through `LpsPxShader::meta()`.
- Public rendering flows through `LpsPxShader::render_frame` in `lp-shader/lp-shader/src/px_shader.rs`.
- `render_frame` currently applies uniforms and validates only the output texture format before calling the backend render loop.
- `LpsTextureBuf` in `lp-shader/lp-shader/src/texture_buf.rs` knows the runtime buffer width, height, format, row stride, and guest pointer, but `LpsValueF32::Texture2D` carries only the opaque descriptor lanes.
- Current texture allocation is safe for today's formats by construction: `LpsEngine::alloc_texture` uses 4-byte allocation alignment, every supported format has 16-bit channels, and tight row strides are even.
- The architecture should not bake in "all textures have even stride"; layout alignment should be format-specific so future `R8`-style formats can use byte loads and byte-aligned rows.
- The filetest harness already validates parsed texture fixtures against `texture-spec` directives in `lp-shader/lps-filetests/src/test_run/texture_fixture.rs`.
- Existing texture filetests cover positive `texelFetch` values for `R16Unorm`, `Rgb16Unorm`, `Rgba16Unorm`, clamp behavior, compile diagnostics, parse diagnostics, and fixture setup failures.
- `positive_minimal_fixture_design_doc.glsl` is still a design-doc fixture smoke and does not assert a real `texelFetch` result.
- The recent LPIR Load16 alignment interstitial plan removed the old Cranelift word-load decomposition and revalidated the texture filetests across default backends.

# Answered Questions

## Q1: How far should public runtime validation go with descriptor-only values?

Context: `LpsValueF32::Texture2D` currently stores only `LpsTexture2DDescriptor`, not the source `LpsTextureBuf` format. That means `render_frame` can validate value type, missing binding, descriptor dimensions, `HeightOne`, row-stride consistency for the expected spec format, and obvious malformed descriptors. It cannot prove the original buffer format when callers construct descriptors manually.

Answer: enrich the host texture value enough to validate format/layout while keeping the guest descriptor ABI unchanged. Runtime validation should not rely on descriptor-only values when the public `LpsTextureBuf` can provide storage facts.

## Q2: Should `row_stride` require tight packing for v0 runtime descriptors?

Context: `LpsTextureBuf::to_texture2d_descriptor()` produces tight rows (`width * bytes_per_pixel`), and filetest fixtures do the same. M3b lowering deliberately honors `row_stride` so future padded rows or subviews can work without an ABI break.

Answer: require `row_stride >= width * bytes_per_pixel` and alignment appropriate for the format/channel load width, not exact tight packing. For current 16-bit formats, base pointer and row stride should be 2-byte aligned. Future `R8` can use alignment 1; future 32-bit formats can require alignment 4.

## Q3: Where should public runtime validation live?

Context: `lp-shader` already owns `texture_interface.rs` for compile-time texture interface validation and `px_shader.rs` for render-time uniform application.

Answer: add a small `runtime_texture_validation` module inside `lp-shader/lp-shader/src/`, called by `LpsPxShader::apply_uniforms` before `inner.set_uniform`.

## Q4: Should M3c add public API helpers for binding textures?

Context: The milestone says public palette/height-one helper APIs are out of scope, but runtime validation has limited information when callers pass only raw descriptors. A helper such as `LpsTextureBuf::to_texture2d_value()` could reduce mistakes without changing the binding surface.

Answer: add a minimal typed texture binding/value helper from `LpsTextureBuf` and avoid a broader resource abstraction. The helper should carry the descriptor plus storage metadata needed by validation.

## Q5: How should backend matrix coverage be represented?

Context: The current filetest default targets exercise `rv32n.q32`, `rv32c.q32`, and `wasm.q32`, but this is implicit unless the file has comments or target directives.

Answer: keep the default-target model, add/retain comments where helpful, and ensure final validation explicitly runs `TEST_FILE=textures cargo test -p lps-filetests --test filetests filetests -- --ignored --nocapture`.

# Notes

- Host-side texture values should separate resource facts from guest ABI facts:
  - `LpsTexture2DDescriptor` remains the four-lane guest ABI token.
  - A host binding/value produced from `LpsTextureBuf` carries `descriptor`, `format`, and enough footprint/layout information to validate before writing descriptor lanes.
- Layout invariants should be stated in terms of each format's required load alignment:
  - `R16Unorm`, `Rgb16Unorm`, `Rgba16Unorm`: 2-byte aligned base, 2-byte aligned row stride, 2-byte aligned channel offsets.
  - Future 1-byte formats should not inherit a false even-stride requirement.
  - Future 32-bit formats should be able to require 4-byte base/row/channel alignment.
