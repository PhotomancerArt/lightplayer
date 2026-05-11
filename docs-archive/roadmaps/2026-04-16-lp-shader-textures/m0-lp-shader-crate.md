# M0 — `lp-shader` Crate + Texture Storage Types

## Goal

Create the `lp-shader/lp-shader` crate as the high-level shader API and add
`TextureStorageFormat` to `lps-shared`. Wire up `LpsShaderEngine` as a
generic wrapper over `LpvmEngine` that compiles GLSL and produces modules.

This milestone does not change the shader contract or add fragment shader
concepts -- it establishes the crate and moves the "compile GLSL to runnable
module" logic out of consumers into a shared layer.

## Deliverables

### `lps-shared`: `TextureStorageFormat`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureStorageFormat {
    /// RGBA 16-bit unorm, 8 bytes/pixel. Default for CPU rendering.
    /// Q32 fractional bits map to unorm16 via saturate: min(q32, 65535).
    Rgba16Unorm,
}

impl TextureStorageFormat {
    pub fn bytes_per_pixel(self) -> usize { 8 }
    pub fn channel_count(self) -> usize { 4 }
}
```

Single variant for now. The enum exists so the format is explicit in the API
rather than implicit. Future variants (Rgb16Unorm for embedded memory
savings, R16Unorm for data textures, etc.) are added when there's a concrete
consumer. The GPU path (`lpfx-gpu`) uses `wgpu::TextureFormat::Rgba16Float`
directly at the wgpu API level -- it doesn't need an lp-shader enum entry
for a format the CPU path can't produce.

### `lps-shared`: `TextureBuffer` trait

```rust
pub trait TextureBuffer {
    fn width(&self) -> u32;
    fn height(&self) -> u32;
    fn format(&self) -> TextureStorageFormat;
    fn data(&self) -> &[u8];
    fn data_mut(&mut self) -> &mut [u8];
}
```

Concrete CPU implementation (`CpuTextureBuffer`) also in `lps-shared` or
`lpvm` (TBD during implementation -- wherever makes the dep graph cleanest).

### `lp-shader/lp-shader` crate

```toml
[dependencies]
lps-shared = { path = "../lps-shared" }
lpir = { path = "../lpir" }
lpvm = { path = "../lpvm" }
lps-frontend = { path = "../lps-frontend" }
```

Initial API -- wraps the existing compile pipeline:

```rust
pub struct LpsShaderEngine<E: LpvmEngine> {
    engine: E,
}

impl<E: LpvmEngine> LpsShaderEngine<E> {
    pub fn new(engine: E) -> Self;

    /// Compile GLSL to a runnable module (generic / compute style).
    /// This is the existing pipeline, formalized.
    pub fn compile(&self, glsl: &str) -> Result<LpsModule<E::Module>, Error>;
}

pub struct LpsModule<M: LpvmModule> {
    pub module: M,
    pub meta: LpsModuleSig,
}
```

This replaces the ad-hoc `lps_frontend::compile` + `lps_frontend::lower` +
`engine.compile` pattern that exists in lpfx-cpu and lp-engine.

### Workspace

Add `lp-shader/lp-shader` to workspace members and default-members.

## Validation

```bash
cargo check -p lp-shader
cargo test -p lp-shader
cargo check   # full default workspace
```

## Non-goals

- Fragment shader contract (M1)
- render_frame / pixel loop (M2)
- Texture reads (M3)
- Consumer migration (M4)
