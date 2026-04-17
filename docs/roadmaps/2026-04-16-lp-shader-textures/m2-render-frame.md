# M2 — `render_frame` via Synthetic `__render_texture`

## Goal

Move the per-pixel render loop into LPIR itself: synthesize a
`__render_texture` function that contains the nested y/x loops, calls
`render(vec2 pos)` for each pixel, converts Q32 → unorm16, and writes the
result to the texture buffer via `Store16`.

`LpsPxShader::render_frame` becomes one trait-level call:
`instance.call_q32("__render_texture", &[tex_ptr, width, height])`.

## Why synthetic LPIR (not host-driven loop)

We considered a host-driven pixel loop (mirroring `lpfx-cpu`'s
`DirectCall::call_i32_buf` pattern). Synthetic LPIR is both faster *and*
simpler:

- **Performance**: one compiled function, `render()` inlined into the loop
  body via the LPIR inliner. Backend optimizes across the loop (hoist
  invariants, register-allocate across pixels). Eliminates per-pixel
  function call + globals reset overhead from the host side.
- **Simplicity**: no backend-specific `render_frame_fast` impls, no
  `DirectCall` plumbing in `lp-shader`, no generic leakage of
  `LpsPxShader<M>` to callers.
- **Portability**: works identically on Cranelift, native JIT, WASM, and
  emulator. No special emulator path needed.
- **Cost**: small one-time codegen overhead (more LPIR ops to compile)
  and a few extra functions in the module. Negligible.

## Deliverables

### Synthetic `__render_texture(tex_ptr: i32, width: i32, height: i32)`

Built programmatically in `lp-shader` after `lps_frontend::lower()`,
before backend `compile()`. Parameterized over `TextureStorageFormat`.

LPIR shape (pseudo-code):

```
fn __render_texture(tex_ptr, width, height):
    y = 0
    loop {
        if y >= height: break
        x = 0
        loop {
            if x >= width: break
            // globals reset (Memcpy snapshot -> globals)
            Memcpy(globals_addr, snapshot_addr, globals_size)

            // pixel center coords in Q32
            pos_x = (x << 16) + 32768
            pos_y = (y << 16) + 32768

            // call render -- inliner fuses this in
            color = render(pos_x, pos_y)

            // pixel byte offset
            row_off = y * (width * bytes_per_pixel)
            px_off  = row_off + x * bytes_per_pixel

            // per-channel Q32 -> unorm16 + Store16
            // (channel count varies by format)
            for ch in 0..channels:
                u = clamp_q32_to_unorm16(color[ch])
                Store16 base=tex_ptr offset=(px_off + ch*2) value=u

            x += 1
        }
        y += 1
    }
```

Format-driven parameters:
- `Rgba16Unorm`: 4 channels, 8 bpp, return type `Vec4`
- `Rgb16Unorm`: 3 channels, 6 bpp, return type `Vec3`
- `R16Unorm`: 1 channel, 2 bpp, return type `Float`

### Q32 → unorm16 conversion

Inlined LPIR sequence (no helper function):

```
v_clamped = max(0, min(value, 65536))  // via Imin/Imax or Select
u16_val   = (v_clamped * 65535) >> 16  // safe in i32: max product = 65535 * 65536
```

Same math as the existing host-side conversion in
`lpfx-cpu/render_cranelift.rs` and `lp-engine/gfx/native_jit.rs`.

### `LpsPxShader` refactor

Drop the `<M: LpvmModule>` generic. Hold the instance type-erased so the
struct doesn't leak the backend type to callers:

```rust
pub struct LpsPxShader {
    inner: Box<dyn PxShaderBackend>,
    output_format: TextureStorageFormat,
    meta: LpsModuleSig,
    render_fn_index: usize,
}

trait PxShaderBackend {
    fn call_render_texture(&mut self, tex_ptr: u32, w: u32, h: u32)
        -> Result<(), LpsError>;
    fn set_uniform(&mut self, name: &str, value: &LpsValueF32)
        -> Result<(), LpsError>;
}
```

`LpsEngine::compile_px` returns `LpsPxShader` (no generic in the public
type).

### `render_frame` implementation

```rust
pub fn render_frame(
    &self,
    uniforms: &LpsValueF32,
    tex: &mut LpsTextureBuf,
) -> Result<(), LpsError> {
    self.apply_uniforms(uniforms)?;
    let w = tex.width();
    let h = tex.height();
    let ptr = tex.guest_ptr().raw();
    self.inner.call_render_texture(ptr, w, h)?;
    Ok(())
}
```

One `call_q32` through the trait. No backend-specific code in
`render_frame`.

### Inliner regression test

Compile a simple shader, dump LPIR for `__render_texture`, assert that
`render()` was inlined:
- No `Call` op targeting the `render` function in `__render_texture`'s op
  stream
- Ops from the body of `render` are present inline

Codify as a test that fails if the inliner stops fusing.

### Migrate consumers

Replace duplicated host-loop pixel rendering:
- `lpfx/lpfx-cpu/src/render_cranelift.rs` → use `LpsPxShader::render_frame`
- `lp-core/lp-engine/src/gfx/cranelift.rs` → use `LpsPxShader::render_frame`
- `lp-core/lp-engine/src/gfx/native_jit.rs` → use `LpsPxShader::render_frame`

Removes the duplicated Q32 → unorm16 host code in those files.

## Validation

```bash
cargo test -p lp-shader --features cranelift
cargo test -p lpfx-cpu
cargo test -p lp-engine
```

End-to-end correctness tests (per format):
- Constant color shader produces uniform texture
- Gradient shader (`return vec4(pos / outputSize, 0.0, 1.0)`) produces
  expected gradient
- Uniform-driven shader respects `set_uniform`

Inliner regression test as described above.

## Dependencies

- M1 (pixel shader contract) — done
- M1.1 (Store16 op + new format variants) — must land first
- LPIR inliner / stable function IDs from `feature/inline` — assumed landed

## Out of Scope (future work)

- Texture reads (`sampler2D`, `texelFetch`) — separate milestone
- Multi-target rendering (multiple output textures)
- Compute-shader-style dispatch
- Additional pixel formats beyond the three in M1.1
