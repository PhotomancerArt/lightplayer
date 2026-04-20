# lp-shader Textures Stage I — Design

Roadmap: `docs/roadmaps/2026-04-16-lp-shader-textures/m0-lp-shader-crate.md`

## Scope

Create `lp-shader/lp-shader` crate and add texture storage types to
`lps-shared`. The new crate is the high-level shader API — it owns the
GLSL-to-runnable-shader pipeline and texture allocation, hiding lpvm
internals from consumers.

## File structure

```
lp-shader/
├── lps-shared/
│   └── src/
│       ├── lib.rs                      # UPDATE: add pub mod texture
│       ├── texture_format.rs           # NEW: TextureStorageFormat enum
│       └── texture_buffer.rs           # NEW: TextureBuffer trait (no impls)
└── lp-shader/
    ├── Cargo.toml                      # NEW: deps on lps-shared, lpir, lpvm, lps-frontend
    └── src/
        ├── lib.rs                      # NEW: crate root, re-exports from lps-shared
        ├── engine.rs                   # NEW: LpsEngine<E>
        ├── frag_shader.rs              # NEW: LpsFragShader<M> (module+instance, render_frame)
        ├── texture_buf.rs              # NEW: LpsTextureBuf (wraps LpvmBuffer)
        └── error.rs                    # NEW: LpsError enum

Cargo.toml (workspace)                  # UPDATE: add lp-shader/lp-shader
```

## Architecture

```
   Consumer (lpfx-cpu, lp-engine, ...)
         |
         |  let engine = LpsEngine::new(cranelift_engine);
         |  let shader = engine.compile_frag(glsl, Rgba16Unorm)?;
         |  let mut tex = engine.alloc_texture(32, 32, Rgba16Unorm);
         |  shader.render_frame(&uniforms, &mut tex)?;
         |
         v
   ┌──────────────────────────────────────┐
   │            lp-shader                  │
   │                                       │
   │  LpsEngine<E: LpvmEngine>            │
   │    .compile_frag(glsl, fmt)           │
   │        -> LpsFragShader               │
   │    .alloc_texture(w, h, fmt)          │
   │        -> LpsTextureBuf               │
   │                                       │
   │  LpsFragShader<M: LpvmModule>        │
   │    .render_frame(&uniforms, &mut tex) │
   │    .meta() -> &LpsModuleSig           │
   │    (module + instance internal)       │
   │                                       │
   │  LpsTextureBuf                        │
   │    (wraps LpvmBuffer + dims + fmt)    │
   │    impl TextureBuffer                 │
   │                                       │
   │  re-exports from lps-shared:          │
   │    TextureStorageFormat, TextureBuffer │
   │    LpsModuleSig, LpsValueF32          │
   └───────────┬───────────────────────────┘
               │ uses internally
      ┌────────┼──────────────┐
      v        v              v
  lps-frontend  lpvm        lps-shared
  (GLSL→LPIR)  (traits,    (TextureStorageFormat,
                 memory)     TextureBuffer trait,
                             LpsModuleSig, ...)
```

## Key types

### `lps-shared`: `TextureStorageFormat`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureStorageFormat {
    /// RGBA 16-bit unorm, 8 bytes/pixel.
    /// Q32 fractional bits map to unorm16 via saturate: min(q32, 65535).
    Rgba16Unorm,
}

impl TextureStorageFormat {
    pub fn bytes_per_pixel(self) -> usize { 8 }
    pub fn channel_count(self) -> usize { 4 }
}
```

Single variant for now. Enum exists so format is explicit, not implicit.

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

Trait only, no implementors in lps-shared. Concrete impl is in lp-shader.

### `lp-shader`: `LpsTextureBuf`

Wraps `LpvmBuffer` + dimensions + format. Implements `TextureBuffer`.
Allocated via `LpsEngine::alloc_texture()` from the engine's shared memory,
so the texture is guest-addressable (shaders can read it in M3).

### `lp-shader`: `LpsEngine<E: LpvmEngine>`

```rust
pub struct LpsEngine<E: LpvmEngine> {
    engine: E,
}

impl<E: LpvmEngine> LpsEngine<E> {
    pub fn new(engine: E) -> Self;

    pub fn compile_frag(
        &self,
        glsl: &str,
        output_format: TextureStorageFormat,
    ) -> Result<LpsFragShader<E::Module>, LpsError>;

    pub fn alloc_texture(
        &self,
        width: u32,
        height: u32,
        format: TextureStorageFormat,
    ) -> Result<LpsTextureBuf, AllocError>;
}
```

`compile_frag` is the only compile method. Fragment is the only shader type.
Output format baked in at compile time for future optimization (inlined
render functions).

### `lp-shader`: `LpsFragShader<M: LpvmModule>`

```rust
use core::cell::RefCell;

pub struct LpsFragShader<M: LpvmModule> {
    module: M,
    instance: RefCell<M::Instance>,
    output_format: TextureStorageFormat,
    meta: LpsModuleSig,
}

impl<M: LpvmModule> LpsFragShader<M> {
    pub fn render_frame(
        &self,
        uniforms: &LpsValueF32,
        tex: &mut LpsTextureBuf,
    ) -> Result<(), LpsError>;

    pub fn meta(&self) -> &LpsModuleSig;
}
```

Module + instance combined internally. The instance is wrapped in
[`RefCell`] so `render_frame` takes `&self` while still mutating the VM
instance (runtime borrow checks; `LpsFragShader` is `!Sync`).
Stateless from consumer perspective: uniforms passed into `render_frame`,
not set as mutable state on the outer type.

### `lp-shader`: `LpsError`

```rust
pub enum LpsError {
    Parse(String),
    Lower(String),
    Compile(String),
    Render(String),
}
```

## Design decisions

- **`no_std`**: Yes, `#![no_std]` + `extern crate alloc`.
- **Generic over backend**: `<E: LpvmEngine>`, not trait objects.
- **No LPIR retention**: Backends that keep it expose via `LpvmModule::lpir_module()`.
- **Re-exports**: lps-shared types only (LpsModuleSig, LpsValueF32,
  TextureStorageFormat, TextureBuffer). lpvm types are impl details.
- **Q32 render variant**: Not in M0. Additive later if needed.

## Non-goals

- Fragment shader contract / `gl_FragCoord` injection (M1)
- Per-pixel render loop in lp-shader (M2)
- Texture reads / `texelFetch` (M3)
- Consumer migration (M4)
