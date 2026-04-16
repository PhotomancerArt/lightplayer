# M2 — `render_frame` and Pixel Loop

## Goal

Move the per-pixel render loop into lp-shader. `FragInstance::render_frame`
replaces the duplicated pixel loops in lpfx-cpu and lp-engine.

## Deliverables

### `FragInstance` with `render_frame`

```rust
pub struct FragInstance<I: LpvmInstance> {
    instance: I,
    meta: LpsModuleSig,
    output_desc: FragOutputDesc,
    frag_color_offset: u32,
    frag_color_components: u32,
}

impl<I: LpvmInstance> FragInstance<I> {
    /// Set a uniform by name (wraps LpvmInstance::set_uniform).
    pub fn set_uniform(&mut self, name: &str, value: &LpsValueF32)
        -> Result<(), Error>;

    /// Render all pixels into the given texture buffer.
    /// Sets gl_FragCoord per pixel, calls main(), reads fragColor,
    /// converts to storage format, writes to texture.
    pub fn render_frame(
        &mut self,
        texture: &mut dyn TextureBuffer,
        time: f32,
    ) -> Result<(), Error>;
}
```

### Generic (slow) pixel loop

The initial `render_frame` implementation is backend-agnostic:

```
for y in 0..height:
    for x in 0..width:
        reset_globals()
        set_uniform("gl_FragCoord", vec2(x, y))
        set_uniform("outputSize", vec2(width, height))
        set_uniform("time", time)
        call_q32("main", &[])
        read fragColor from vmctx at frag_color_offset
        convert Q32 -> storage format (e.g. * 65535 >> 16 for unorm16)
        write to texture buffer at (x, y)
```

This works through the `LpvmInstance` trait -- no backend-specific code.
It's slower than the current DirectCall path but correct and portable.

### Backend-specific fast path (Cranelift)

For backends that support it, `FragInstance` can downcast or use a
backend-specific method to get a fast path:

```rust
impl FragInstance<CraneliftInstance> {
    pub fn render_frame_fast(
        &mut self,
        texture: &mut dyn TextureBuffer,
        time: f32,
    ) -> Result<(), Error> {
        // Use DirectCall::call_i32_buf with instance.vmctx_ptr()
        // Same performance as today's lp-engine/lpfx-cpu pixel loops
    }
}
```

The fast path uses `DirectCall::call_i32_buf` with the real instance vmctx
(supporting uniforms/globals), matching the pattern already proven in
lpfx-cpu's `render_cranelift.rs`.

### Q32 -> unorm16 conversion

Centralized in lp-shader (used by all backends):

```rust
/// Convert a clamped Q16.16 value to unorm16.
/// Input: Q32 in [0, 65536] (i.e. [0.0, 1.0])
/// Output: u16 in [0, 65535]
fn q32_to_unorm16(q32: i32) -> u16 {
    let clamped = q32.max(0).min(65535);
    clamped as u16
}
```

This is a saturate-to-65535, not a multiply. The single value discontinuity
(65535 and 65536 both map to 65535) is at pure white where it's invisible.
On RV32, `min` is a single instruction (Zbb, available in LPIR as `Imin`).
See `notes.md` for the full rationale.

### Texture write helpers

Format-aware pixel write functions:

```rust
fn write_pixel_rgb16(buf: &mut [u8], offset: usize, r: u16, g: u16, b: u16);
fn write_pixel_rgba16(buf: &mut [u8], offset: usize, r: u16, g: u16, b: u16, a: u16);
```

These are inlineable and used by both the slow and fast paths.

## Validation

```bash
cargo test -p lp-shader
# Verify: compile_frag + render_frame produces non-trivial pixels
# Verify: fast path matches slow path output
```

## Future optimization: synthetic __render_frame

Not in this milestone, but the API is designed for it. A future pass could
emit a synthetic LPIR function that fuses the pixel loop, main() call,
format conversion, and texture writes. With the LPIR inliner, this could
produce a single flat function with no per-pixel call overhead.

## Dependencies

- M1 (fragment shader contract, output globals, compile_frag)
