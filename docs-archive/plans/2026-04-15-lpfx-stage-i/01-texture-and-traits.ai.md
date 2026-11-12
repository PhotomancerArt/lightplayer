# Phase 1: Texture and trait types in `lpfx`

## Scope

Add `TextureId`, `TextureFormat`, `CpuTexture`, `FxEngine` trait, and
`FxInstance` trait to the `lpfx` core crate. No new dependencies -- these
are pure `no_std + alloc` data types and traits.

## Code organization reminders

- One concept per file: `texture.rs`, `engine.rs`, `instance.rs`.
- Place traits and public types first, helper methods at the bottom.
- Keep related functionality grouped together.

## Implementation

### 1.1 `lpfx/lpfx/src/texture.rs`

```rust
use alloc::vec::Vec;

/// Opaque texture handle issued by `FxEngine::create_texture`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureId(pub(crate) u32);

/// Pixel format for effect textures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureFormat {
    /// 16-bit RGBA (4 x u16 per pixel, 8 bytes). Default for LightPlayer.
    Rgba16,
}

impl TextureFormat {
    pub fn bytes_per_pixel(self) -> usize {
        match self {
            Self::Rgba16 => 8,
        }
    }
}

/// CPU-side pixel buffer. Used by any CPU backend (cranelift, native, wasm).
pub struct CpuTexture {
    width: u32,
    height: u32,
    format: TextureFormat,
    data: Vec<u8>,
}

impl CpuTexture {
    pub fn new(width: u32, height: u32, format: TextureFormat) -> Self {
        let size = (width as usize) * (height as usize) * format.bytes_per_pixel();
        let mut data = Vec::with_capacity(size);
        data.resize(size, 0);
        Self { width, height, format, data }
    }

    pub fn width(&self) -> u32 { self.width }
    pub fn height(&self) -> u32 { self.height }
    pub fn format(&self) -> TextureFormat { self.format }
    pub fn data(&self) -> &[u8] { &self.data }
    pub fn data_mut(&mut self) -> &mut [u8] { &mut self.data }

    pub fn set_pixel_u16(&mut self, x: u32, y: u32, rgba: [u16; 4]) {
        let bpp = self.format.bytes_per_pixel();
        let offset = ((y as usize) * (self.width as usize) + (x as usize)) * bpp;
        let bytes = &mut self.data[offset..offset + bpp];
        for (i, &val) in rgba.iter().enumerate() {
            let le = val.to_le_bytes();
            bytes[i * 2] = le[0];
            bytes[i * 2 + 1] = le[1];
        }
    }

    pub fn pixel_u16(&self, x: u32, y: u32) -> [u16; 4] {
        let bpp = self.format.bytes_per_pixel();
        let offset = ((y as usize) * (self.width as usize) + (x as usize)) * bpp;
        let bytes = &self.data[offset..offset + bpp];
        let mut out = [0u16; 4];
        for i in 0..4 {
            out[i] = u16::from_le_bytes([bytes[i * 2], bytes[i * 2 + 1]]);
        }
        out
    }
}
```

### 1.2 `lpfx/lpfx/src/engine.rs`

```rust
use crate::input::FxValue;
use crate::module::FxModule;
use crate::texture::{TextureFormat, TextureId};

pub trait FxEngine {
    type Instance: FxInstance;
    type Error: core::fmt::Display;

    fn create_texture(&mut self, width: u32, height: u32, format: TextureFormat) -> TextureId;
    fn instantiate(&mut self, module: &FxModule, output: TextureId) -> Result<Self::Instance, Self::Error>;
}

pub trait FxInstance {
    type Error: core::fmt::Display;

    fn set_input(&mut self, name: &str, value: FxValue) -> Result<(), Self::Error>;
    fn render(&mut self, time: f32) -> Result<(), Self::Error>;
}
```

### 1.3 Update `lpfx/lpfx/src/lib.rs`

Add modules and re-exports:

```rust
pub mod texture;
pub mod engine;

pub use texture::{TextureId, TextureFormat, CpuTexture};
pub use engine::{FxEngine, FxInstance};
```

## Tests

- `CpuTexture::new` allocates correct size (width * height * 8 for Rgba16).
- Round-trip: `set_pixel_u16` then `pixel_u16` returns same values.
- `TextureFormat::bytes_per_pixel` returns 8 for `Rgba16`.

## Validate

```bash
cargo check -p lpfx
cargo test -p lpfx
```

Existing M0 tests must still pass.
