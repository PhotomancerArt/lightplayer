# Scope of Work

Milestone 3b replaces the M3a `texelFetch` placeholder with the core LPIR data
path for supported GLSL `texelFetch(sampler2D, ivec2, 0)`.

The implementation lowers texture descriptor lane loads, integer coordinate
clamping, byte address calculation, format-specialized unorm16 channel loads,
`Unorm16toF` conversion, and GLSL-compatible `vec4` channel fill.

Out of scope:

- Runtime validation of host-provided `LpsTextureBuf` / descriptor values.
- Public texture binding API helpers.
- Backend implementation for new LPIR ops; M3b should use existing LPIR ops.
- Normalized-coordinate `texture()` sampling, filtering, wrap modes, mipmaps, or
  any `lod != 0` behavior.
- Changing the default safety policy: M3b adds a compiler option to disable
  generated bounds clamps for performance measurement, but the default remains
  memory-safe clamp-to-edge behavior.

# File Structure

```text
lp-shader/
├── lpir/
│   └── src/
│       └── compiler_config.rs            # UPDATE: texture texelFetch bounds option + compile-opt key
├── lps-frontend/
│   └── src/
│       ├── lower.rs                       # UPDATE: LowerOptions carries texture safety option
│       ├── lower_ctx.rs                   # UPDATE: pass texture safety option into function lowering
│       ├── lower_expr.rs                  # UPDATE: pass ImageLoad coordinate into texture lowering
│       └── lower_texture.rs               # UPDATE: replace M3a placeholder with texelFetch data path
├── lp-shader/
│   └── src/
│       └── engine.rs                      # UPDATE: copy CompilerConfig texture option into LowerOptions
├── lpvm-cranelift/
│   └── src/
│       ├── emit/memory.rs                 # UPDATE: support RV32 Load16U for texture reads
│       ├── emit/mod.rs                    # UPDATE: carry RV32 Load16U decomposition flag
│       └── module_lower.rs                # UPDATE: enable RV32 Load16U decomposition
└── lps-filetests/
    └── src/
        └── test_run/
            └── filetest_lpvm.rs           # UPDATE: copy CompilerConfig texture option into LowerOptions
    └── filetests/
        └── textures/
            ├── texelfetch_rgba16_unorm.glsl   # NEW/RENAMED: exact values, wasm+rv32n+rv32c
            ├── texelfetch_rgb16_unorm.glsl    # NEW: RGB + alpha fill
            ├── texelfetch_r16_unorm.glsl      # NEW: R + missing channel fill
            └── texelfetch_clamp_bounds.glsl   # NEW: out-of-range clamp behavior
```

# Conceptual Architecture Summary

```text
Naga ImageLoad(texelFetch)
    │
    ▼
lower_expr.rs
    └─ checks no array layer / multisample / missing lod
       passes image + coordinate + lod to lower_texture
          │
          ▼
lower_texture.rs
    ├─ resolve direct Texture2D uniform name
    ├─ validate matching TextureBindingSpec
    ├─ validate lod == literal 0
    ├─ load descriptor lanes from VMContext
    │     ptr, width, height, row_stride
    ├─ lower ivec2 coordinate expression
    ├─ clamp x/y to descriptor bounds unless compiler option is unchecked
    ├─ compute byte address:
    │     ptr + y * row_stride + x * bytes_per_pixel
    ├─ emit format-specialized Load16U per stored channel
    ├─ emit Unorm16toF per loaded channel
    └─ fill missing vec4 lanes with 0.0 / 1.0
```

# Main Components

## `lower_expr.rs`

`Expression::ImageLoad` handling should continue to reject multisampled,
array-layered, and non-LOD image loads before calling texture-specific lowering.

M3b changes the call from passing only `image` and `level` to passing
`image`, `coordinate`, and `level`, so `lower_texture.rs` owns the complete
`texelFetch` data path.

## Texture Safety Compiler Option

Add a texture lowering option to `lpir::CompilerConfig` and expose it through
the existing `compile-opt` parser.

Suggested public shape:

```rust
pub struct CompilerConfig {
    pub inline: InlineConfig,
    pub q32: lps_q32::q32_options::Q32Options,
    pub texture: TextureConfig,
}

pub struct TextureConfig {
    pub texel_fetch_bounds: TexelFetchBoundsMode,
}

pub enum TexelFetchBoundsMode {
    ClampToEdge,
    Unchecked,
}
```

Default must be `ClampToEdge`. The filetest/directive key should be explicit,
for example:

```text
// compile-opt(texture.texel_fetch_bounds, clamp-to-edge)
// compile-opt(texture.texel_fetch_bounds, unchecked)
```

`CompilerConfig` is not currently passed directly to `lps-frontend`; M3b should
copy the texture safety setting into `lps_frontend::LowerOptions` at the
existing lowering call sites:

- `lp-shader/lp-shader/src/engine.rs`
- `lp-shader/lps-filetests/src/test_run/filetest_lpvm.rs`
- error-test lowering paths if they construct `LowerOptions`

## `lower_texture.rs`

`lower_texture.rs` remains the texture-specific lowering module for M3b. It
should keep the texture entry point first and helper functions at the bottom.

Use named descriptor lane constants for the fixed `LpsTexture2DDescriptor` ABI:

```rust
const TEXTURE_DESC_PTR_OFFSET: u32 = 0;
const TEXTURE_DESC_WIDTH_OFFSET: u32 = 4;
const TEXTURE_DESC_HEIGHT_OFFSET: u32 = 8;
const TEXTURE_DESC_ROW_STRIDE_OFFSET: u32 = 12;
```

A small internal helper struct is preferred over passing a raw four-element
vector around:

```rust
struct TextureDescriptorVRegs {
    ptr: VReg,
    width: VReg,
    height: VReg,
    row_stride: VReg,
}
```

When `TexelFetchBoundsMode::ClampToEdge` is active, coordinate clamping should
be generated with existing LPIR integer comparisons and `Select`: clamp negative
coordinates to `0`, and clamp coordinates above the descriptor extent to
`extent - 1`. This keeps the default runtime behavior memory-safe.

When `TexelFetchBoundsMode::Unchecked` is active, generate address math from the
raw integer coordinates without bounds clamps. This mode exists only as a
performance measurement knob; it permits out-of-bounds reads if the shader
provides bad coordinates.

`TextureWrap` is intentionally ignored for `texelFetch`; wrap modes belong to
M4 `texture()` sampling.

Format dispatch is compile-time from `TextureBindingSpec::format`:

- `R16Unorm`: load channel 0, fill G/B with `0.0`, fill A with `1.0`.
- `Rgb16Unorm`: load channels 0..2, fill A with `1.0`.
- `Rgba16Unorm`: load channels 0..3.

Each stored channel should use `Load16U`, followed directly by `Unorm16toF`.
Missing channels should use `FconstF32`.

## Filetests

M3b should validate exact values on the three mainline Q32 targets:

- `wasm.q32`
- `rv32n.q32`
- `rv32c.q32`

The M3a placeholder expected-error test should be converted or renamed into a
positive M3b run test. Additional tests should cover `Rgb16Unorm`,
`R16Unorm`, and clamp-to-edge behavior for out-of-range coordinates.

Add at least one focused test for the compiler option, preferably using a
lowering/printed-LPIR assertion rather than relying on unsafe runtime behavior:
safe mode should include clamp-related `Select`/comparison ops, while unchecked
mode should omit those clamp ops for the same shader shape.

## Cranelift RV32 Backend Support

The required `rv32c.q32` filetests may expose backend gaps in existing LPIR ops.
M3b should fix those gaps when they are necessary for the agreed mainline target
set.

In particular, texture fixtures can produce texel addresses that are only
2-byte aligned. If the Cranelift RV32 path cannot lower `Load16U` for such
addresses, add a target-specific lowering path that avoids misaligned RV32 word
loads while preserving `Load16U` semantics. This is backend support for an
existing LPIR op, not a new texture opcode.

