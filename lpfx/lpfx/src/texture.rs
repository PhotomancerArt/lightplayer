//! CPU-side textures and opaque handles for effect outputs.

use alloc::vec::Vec;

/// Opaque texture handle issued by [`crate::FxEngine::create_texture`](crate::engine::FxEngine::create_texture).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TextureId(u32);

impl TextureId {
    /// Wrap a raw id. CPU backends allocate unique ids when creating textures.
    #[must_use]
    pub const fn from_raw(id: u32) -> Self {
        Self(id)
    }

    /// Raw id for maps and logging.
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }
}

/// Pixel format for effect textures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureFormat {
    /// 16-bit RGBA (4 × u16 per pixel, 8 bytes). Default for LightPlayer.
    Rgba16,
}

impl TextureFormat {
    #[must_use]
    pub fn bytes_per_pixel(self) -> usize {
        match self {
            Self::Rgba16 => 8,
        }
    }
}

/// CPU-side pixel buffer. Used by any CPU backend (Cranelift, native, wasm).
pub struct CpuTexture {
    width: u32,
    height: u32,
    format: TextureFormat,
    data: Vec<u8>,
}

impl CpuTexture {
    #[must_use]
    pub fn new(width: u32, height: u32, format: TextureFormat) -> Self {
        let size = (width as usize) * (height as usize) * format.bytes_per_pixel();
        let mut data = Vec::with_capacity(size);
        data.resize(size, 0);
        Self {
            width,
            height,
            format,
            data,
        }
    }

    #[must_use]
    pub fn width(&self) -> u32 {
        self.width
    }

    #[must_use]
    pub fn height(&self) -> u32 {
        self.height
    }

    #[must_use]
    pub fn format(&self) -> TextureFormat {
        self.format
    }

    #[must_use]
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub fn data_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }

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

    #[must_use]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgba16_allocation_size() {
        let t = CpuTexture::new(4, 8, TextureFormat::Rgba16);
        assert_eq!(t.data().len(), 4 * 8 * 8);
    }

    #[test]
    fn bytes_per_pixel() {
        assert_eq!(TextureFormat::Rgba16.bytes_per_pixel(), 8);
    }

    #[test]
    fn pixel_round_trip_u16() {
        let mut t = CpuTexture::new(3, 3, TextureFormat::Rgba16);
        let px = [100, 200, 300, 400];
        t.set_pixel_u16(1, 2, px);
        assert_eq!(t.pixel_u16(1, 2), px);
    }
}
