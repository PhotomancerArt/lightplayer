# lp-shader Texture System — Design Notes

## Decisions / history

### Host execution: Wasmtime instead of Cranelift JIT (2026)

For **host** CPU execution of `lp-shader` / LPIR (tests, future `lp-cli`,
post-M4 consumers), we standardize on **Wasmtime** (`lpvm-wasm`) rather
than the in-process **`lpvm-cranelift` JIT**. The JIT crate is **not
deleted** and remains for legacy callers and a **single-instance Phase 2
smoke test** (`render_texture_smoke.rs`) that guards the JIT trait
implementation.

**Reasons:** (1) Multi-`JITModule` / multi-instance use in one process has
exhibited **non-deterministic state leakage** in Cranelift’s JIT backend,
reproducing as flaky or order-dependent failures (including panics such as
“function must be compiled before it can be finalized” under
`--test-threads=1`); we have prior art disabling JIT in `lps-filetests` for
similar issues. (2) Wasmtime uses Cranelift internally with **proper
per-instance isolation**. (3) **32-bit guest pointers** on the host match
RV32, emulator, and browser — removing the 64-bit-host-pointer ABI corner
that complicated `call_render_texture` for the old default host path.

`lp-engine` and `lpfx-cpu` still use the JIT until **M4 (consumer
migration)** lands; M4 explicitly includes switching those stacks to
Wasmtime through `lp-shader`.

## Q32 / unorm16 relationship

Q16.16 represents 1.0 as 0x0001_0000 (65536).
unorm16 represents 1.0 as 0xFFFF (65535).

These are NOT the same representation. You cannot just mask the low 16 bits
of a Q32 value to get a unorm16 -- that gives 0 for the value 1.0.

### Conversion: saturate, not multiply

The simplest and best conversion is: `min(clamped_q32, 65535) as u16`.

This maps:

- 0 -> 0
- 1 -> 1
- ...
- 65535 -> 65535
- 65536 -> 65535 (saturation at pure white)

The single value discontinuity (two inputs mapping to one output) is at the
top of the range, between 0.999985 and 1.0 -- completely invisible.

The alternative multiply approach `(q32 * 65535) >> 16` puts the
discontinuity at the bottom (inputs 0 and 1 both map to output 0), which is
worse because human vision is most sensitive to differences in dark values.

RV32 has a `min` instruction (Zbb extension, already used in the emulator
and available in LPIR as `Imin`/`Fmin`), so this is a single instruction --
cheaper than the multiply + shift alternative.

The pedantic unorm16 interpretation says output 32768 means 32768/65535 =
0.500008, while Q32 input 32768 means 32768/65536 = 0.500000. That's an
8-parts-per-million difference -- negligible for color data.

## GPU texture format constraints

wgpu/WebGPU baseline formats (no feature flags required):

- Rgba16Float (f16, 8 bytes/pixel) -- universally supported
- Rgba8Unorm (u8, 4 bytes/pixel) -- universally supported

Feature-gated (TEXTURE_FORMAT_16BIT_NORM):

- Rgba16Unorm (u16, 8 bytes/pixel) -- NOT baseline

No 3-channel (RGB) formats in WebGPU at all.

Implication: GPU render target is Rgba16Float, but that's a wgpu-level
concern -- lpfx-gpu uses wgpu::TextureFormat::Rgba16Float directly. The
lp-shader enum only needs formats the CPU path can produce. Since LPIR/Q32
has no f16 type, Rgba16Float doesn't belong in lp-shader's enum.

The enum starts with a single variant: Rgba16Unorm (8 bytes/pixel). Future
variants (Rgb16Unorm for embedded memory, R16Unorm for data textures) are
added when there's a concrete consumer.

## Why fragment shader contract, not function arguments

The current `render(fragCoord, outputSize, time) -> vec4` was a workaround
for not having uniforms/globals. Now that we have those:

- `gl_FragCoord` becomes a built-in (uniform on CPU, native on GPU)
- `outputSize`, `time` become regular uniforms
- Output is via `out vec4 fragColor` (output global)

Benefits:

- Standard GLSL -- portable to GPU without transformation
- Output globals enable the runtime to read results without return values
- The compiler can see the full data flow (uniforms in, globals out)
- Enables future synthetic \_\_render_frame inlined function

The bootstrap wrapper approach means existing `render()` style shaders
keep working -- lpfx generates the wrapper GLSL automatically.

## Why output format at compile time

If the output format is known at compile time, the compiler can:

1. Emit format-specific Q32 -> unorm16 conversion inline
2. Emit direct stores to texture memory with known stride
3. In the future, fuse the pixel loop into a synthetic LPIR function
   that the inliner can optimize into a single flat function

The alternative (runtime format dispatch) adds a branch per pixel per
channel. On ESP32 that's measurable.

## Crate structure decision

Considered merging lps-shared + lpvm. Rejected because:

- lps-frontend depends only on lps-shared, not lpvm
- lpir depends on neither
- Merging would pull runtime concepts into the frontend's dep tree

Instead: new lp-shader crate on top that depends on everything and provides
the high-level API. Lower crates stay unchanged.

## Texture buffer ownership

FragInstance::render_frame takes `&mut dyn TextureBuffer` -- it borrows the
buffer for the duration of the render. The caller owns the buffer. This
matches the GPU model (render to a texture you created) and the lp-engine
model (engine owns textures, passes them to shaders).

## gl_FragCoord on CPU

On GPU, gl_FragCoord is a hardware built-in. On CPU, it's injected as a
uniform. The compile_frag path prepends it to the uniform block
automatically. The render loop sets it per-pixel before calling main().

For the fast path (DirectCall), the uniform write is a direct store to a
known vmctx offset -- same cost as the old function-argument approach.

## Texture reads: format-aware loads

When a shader reads from a texture (texelFetch), the load must convert from
storage format to the shader's float representation:

- Rgb16Unorm -> vec4(r/65535, g/65535, b/65535, 1.0) in Q32
- Rgba16Unorm -> vec4(r/65535, g/65535, b/65535, a/65535) in Q32

The texture format is known at compile time (bound when the texture is
declared). The compiler can emit format-specific load sequences.

For Q32: `(u16_value << 16) / 65535` converts unorm16 to Q16.16. Or
approximately: `u16_value << 16 | u16_value` (accurate to ~1 LSB, avoids
division).
