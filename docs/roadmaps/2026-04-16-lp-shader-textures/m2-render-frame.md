# M2 — `render_frame` via Synthetic `__render_texture`

## Goal

Move the per-pixel render loop into LPIR itself: synthesize a
`__render_texture` function that contains the nested y/x loops, calls
`render(vec2 pos)` for each pixel, converts Q32 → unorm16, and writes the
result to the texture buffer via `Store16`.

`LpsPxShader::render_frame` becomes one direct trait call:
`instance.call_render_texture(name, tex_buffer, width, height)` —
served by a dedicated, fast-path method on `LpvmInstance` that
resolves the entry on first call and caches it internally (no
per-frame string lookup after warmup, no arg marshalling, no
return-vec allocation).

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

## Why a dedicated `LpvmInstance` method (not generic `call_q32`)

Originally we considered invoking `__render_texture` through the existing
`call_q32(name: &str, args: &[i32])` generic path. Two problems killed
that:

- **Pointer-width asymmetry.** The texture pointer is `IrType::Pointer`
  (host-width on Cranelift JIT = 64-bit; 32-bit on RV32 / emu / WASM).
  The flat `&[i32]` ABI cannot carry that polymorphism without either
  pair-of-i32 packing or a typed-value refactor — both of which add
  significant churn for what is fundamentally a single hot-path call.
  With **Wasmtime as the host execution backend** for `lp-shader`, the
  production host path uses **32-bit guest offsets** like the other
  non–`lpvm-cranelift` backends, so this asymmetry **no longer applies in
  practice** on the path we ship tests and future `lp-cli` against; the
  dedicated method remains the right trait shape for any backend that
  still lowers `Pointer` differently (including the deprecated JIT).
- **Hot-path overhead.** Per-frame: HashMap symbol lookup + `Vec<i32>`
  allocation for args + per-arg marshal + `Vec<i32>` allocation for
  return + per-return unmarshal. We previously paid this cost per *pixel*
  in `DirectCall`-less paths and measured a **~10× slowdown**. Per-frame
  it is much smaller, but this is also exactly the case where a typed
  fast path is straightforward to provide.

Both problems vanish if we acknowledge that texture rendering is the
*main* execution mode of `LpsPxShader` — not a generic call — and bake
its shape into the trait:

```rust
pub trait LpvmInstance {
    /// Hot path: invoke the synthesised `__render_texture[_<format>]`
    /// entry by name. The instance is responsible for resolving the
    /// entry on first call and caching it internally; subsequent
    /// calls with the same name should hit the cache.
    ///
    /// `texture` carries both host pointer (for JIT) and 32-bit guest
    /// offset (for RV32 / emu / WASM); each backend extracts what its
    /// calling convention needs. Validates signature shape
    /// `(Pointer, I32, I32) -> ()` on the first lookup.
    fn call_render_texture(
        &mut self,
        fn_name: &str,
        texture: &mut LpvmBuffer,
        width: u32,
        height: u32,
    ) -> Result<(), Self::Error>;

    // existing `call`, `call_q32`, `set_uniform*` unchanged.
}
```

A single method, with caching as an implementation detail of each
backend (typically a small `Option<(String, ResolvedEntry)>` field).
No public `RenderTextureHandle` type, no separate `lookup` method —
the first call pays the resolve cost, every subsequent call reuses
the cached entry. Same hot-path throughput as a separate lookup-and-
invoke pair, with a smaller trait surface and no handle-safety
concerns.

This is purely additive — `call_q32` keeps its `&[i32]` shape and serves
init / parameter updates / one-off calls. Only `__render_texture` gets
the dedicated method.

## Deliverables

### `LpvmInstance` trait extension

Add `call_render_texture` to the `LpvmInstance` trait in
`lp-shader/lpvm/src/instance.rs`. Each backend owns its own internal
cache for the resolved entry. Implement on every backend:

- `lp-shader/lpvm-cranelift` (host JIT): cache stores a function
  pointer; invocation passes `texture.host_ptr()` directly to the JIT'd
  function (real 64-bit host pointer, no translation).
- `lp-shader/lpvm-native` (RV32fa JIT + emu): cache stores the resolved
  guest entry; invocation passes `texture.guest_base() as i32` per the
  RV32 ABI.
- `lp-shader/lpvm-wasm` (wasmtime + browser): cache stores a
  `wasmtime::Func` (or browser equivalent); invocation passes the
  32-bit linear-memory offset.
- `lp-shader/lpvm-emu` (standalone interpreter): same shape as RV32 —
  guest offset in i32.

Each impl is small (cache-or-resolve entry + extract pointer-form from
`LpvmBuffer` + invoke), but each has its own optimised call path.

Warmup-time presence/signature validation lives in `LpsPxShader::new`
(inspecting `meta()`), not in the trait. That keeps the trait surface
minimal and the hot path uncluttered.

### Synthetic `__render_texture(tex_ptr: Pointer, width: I32, height: I32)`

Built programmatically in `lp-shader` after `lps_frontend::lower()`,
before backend `compile()`. Parameterized over `TextureStorageFormat`.
The first parameter is `IrType::Pointer` — already backend-polymorphic
in LPIR, so each backend lowers it appropriately (i64 on host JIT, i32
elsewhere) without any LPIR-level changes.

LPIR shape (pseudo-code):

```
fn __render_texture(tex_ptr: Pointer, width: I32, height: I32):
    y = 0
    loop {
        if y >= height: break
        x = 0
        loop {
            if x >= width: break
            // globals reset (only emitted when globals may be mutated)
            Memcpy(globals_addr, snapshot_addr, globals_size)

            // pixel center coords in Q32
            pos_x = (x << 16) + 32768
            pos_y = (y << 16) + 32768

            // call render -- inliner fuses this in (deferred)
            color = render(pos_x, pos_y)

            // pixel byte offset
            row_off = y * (width * bytes_per_pixel)
            px_off  = row_off + x * bytes_per_pixel

            // per-channel Q32 -> unorm16 + Store16
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

One specialised function per `compile_px` call (format known at compile
time). The synthesis routine is backend-agnostic — operates on
`LpirModule + LpsModuleSig + TextureStorageFormat` and returns an
`IrFunction` that any backend can compile.

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
    render_fn_name: String,  // format-specific symbol, e.g. "__render_texture_rgba16"
}

trait PxShaderBackend {
    fn call_render_texture(
        &mut self,
        name: &str,
        texture: &mut LpvmBuffer,
        width: u32,
        height: u32,
    ) -> Result<(), LpsError>;

    fn set_uniform(&mut self, name: &str, value: &LpsValueF32)
        -> Result<(), LpsError>;
}
```

The backend adapter holds `(M, M::Instance)` and forwards
`call_render_texture` straight into `LpvmInstance::call_render_texture`.
Hot-path entry caching lives inside the LPVM instance (Q8), not in
the adapter. `PxShaderBackend` itself stays LPVM-agnostic.

`LpsPxShader::new` validates that the synthesised render function
exists in `meta()` with the expected signature *before* the first
`render_frame` — that's the one-time check that would otherwise have
needed a separate `lookup` trait method.

`LpsEngine::compile_px` returns `LpsPxShader` (no generic in the public
type).

### `render_frame` implementation

```rust
pub fn render_frame(
    &mut self,
    uniforms: &LpsValueF32,
    tex: &mut LpsTextureBuf,
) -> Result<(), LpsError> {
    self.apply_uniforms(uniforms)?;
    let w = tex.width();
    let h = tex.height();
    self.inner.call_render_texture(&self.render_fn_name, tex.buffer_mut(), w, h)?;
    Ok(())
}
```

One direct v-call → one trait method on the instance → cache hit on
the resolved entry → one machine call into compiled guest code. After the
first frame: no string-table lookup, no allocations, no marshalling.

### Inliner regression test

Compile a simple shader, dump LPIR for `__render_texture`, assert that
exactly one `Call` to `render` is present today (inliner has not landed
on this branch). The test header documents how to invert the assertion
(zero calls + inlined body ops) when the inliner integration milestone
ships.

## Validation

```bash
cargo test -p lpvm
cargo test -p lpvm-cranelift
cargo test -p lpvm-native
cargo test -p lp-shader
cargo test -p lp-shader --features native
```

End-to-end correctness tests (per format):
- Constant color shader produces uniform texture
- Gradient shader (`return vec4(pos / outputSize, 0.0, 1.0)`) produces
  expected gradient
- Uniform-driven coverage stays via existing `render_frame_sets_uniforms`
  test.

Inliner regression test as described above.

## Implemented in

[`docs/plans/2026-04-17-lp-shader-textures-stage-v/`](../../plans/2026-04-17-lp-shader-textures-stage-v/) (stage plan; M2.0 closeout includes Phase 6 cleanup + validation).

## Dependencies

- M1 (pixel shader contract) — done
- M1.1 (six narrow memory ops + `R16Unorm` / `Rgb16Unorm` + `compile_px`
  validation) — done; see [`m1.1-lpir-format-prereqs.md`](./m1.1-lpir-format-prereqs.md)
- LPIR inliner / stable function IDs — prerequisite ops landed in M1.1;
  full inliner integration deferred to a follow-up perf milestone (M2.0
  ships with a real `Call render(...)` per pixel)

## Out of Scope (future work)

- Texture reads (`sampler2D`, `texelFetch`) — separate milestone (M3)
- Consumer migration (lpfx-cpu, lp-engine) — separate milestone (M4)
- Multi-target rendering (multiple output textures)
- Compute-shader-style dispatch
- Additional pixel formats beyond the three in M1.1
