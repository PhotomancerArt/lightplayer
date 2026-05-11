//! CPU-backed texture result materialized from a [`super::VisualProduct`].

use core::fmt;

use lps_shared::TextureStorageFormat;

use super::{VisualSample, VisualSampleBatch, VisualSampleBatchResult};

/// Invalid [`TextureRenderProduct`] construction input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextureRenderProductError {
    ZeroDimension { width: u32, height: u32 },
    ByteLenMismatch { expected: usize, actual: usize },
}

impl fmt::Display for TextureRenderProductError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroDimension { width, height } => {
                write!(
                    f,
                    "texture dimensions must be non-zero (got {width}x{height})"
                )
            }
            Self::ByteLenMismatch { expected, actual } => write!(
                f,
                "texture pixel byte length mismatch: expected {expected} bytes, got {actual}"
            ),
        }
    }
}

impl core::error::Error for TextureRenderProductError {}

/// Texture-backed visual product with private byte storage (no `LpsTextureBuf` in the public API).
///
/// Sample coordinates in [`VisualSampleBatch`] are interpreted as integer texels with
/// clamp-to-edge behavior.
#[derive(Debug, Clone)]
pub struct TextureRenderProduct {
    width: u32,
    height: u32,
    format: TextureStorageFormat,
    pixels: alloc::vec::Vec<u8>,
}

impl TextureRenderProduct {
    #[must_use]
    pub fn width(&self) -> u32 {
        self.width
    }

    #[must_use]
    pub fn height(&self) -> u32 {
        self.height
    }

    #[must_use]
    pub fn storage_format(&self) -> TextureStorageFormat {
        self.format
    }

    /// Raw tightly packed pixel bytes when resident in host memory.
    #[must_use]
    pub fn try_raw_bytes(&self) -> Option<&[u8]> {
        Some(self.pixels.as_slice())
    }

    /// Builds a product after validating dimensions and byte length.
    pub fn new(
        width: u32,
        height: u32,
        format: TextureStorageFormat,
        pixels: alloc::vec::Vec<u8>,
    ) -> Result<Self, TextureRenderProductError> {
        if width == 0 || height == 0 {
            return Err(TextureRenderProductError::ZeroDimension { width, height });
        }
        let expected = usize::try_from(width)
            .ok()
            .and_then(|w| usize::try_from(height).ok().map(|h| w.saturating_mul(h)))
            .and_then(|wh| wh.checked_mul(format.bytes_per_pixel()))
            .unwrap_or(usize::MAX);
        if pixels.len() != expected {
            return Err(TextureRenderProductError::ByteLenMismatch {
                expected,
                actual: pixels.len(),
            });
        }
        Ok(Self {
            width,
            height,
            format,
            pixels,
        })
    }

    /// Convenience for tests and deterministic RGBA16 textures (`width` × `height` × 8 bytes).
    pub fn rgba16_unorm(
        width: u32,
        height: u32,
        pixels: alloc::vec::Vec<u8>,
    ) -> Result<Self, TextureRenderProductError> {
        Self::new(width, height, TextureStorageFormat::Rgba16Unorm, pixels)
    }
}

impl TextureRenderProduct {
    pub fn sample_batch(&self, request: &VisualSampleBatch) -> VisualSampleBatchResult {
        let mut samples = alloc::vec::Vec::with_capacity(request.points.len());
        for p in &request.points {
            let (tx, ty) = clamp_texel(p.x, p.y, self.width, self.height);
            let color = sample_texel(&self.pixels, self.width, self.format, tx, ty);
            samples.push(VisualSample {
                rgba_unorm16: color,
            });
        }
        VisualSampleBatchResult { samples }
    }
}

fn clamp_texel(x: u32, y: u32, width: u32, height: u32) -> (u32, u32) {
    (
        x.min(width.saturating_sub(1)),
        y.min(height.saturating_sub(1)),
    )
}

fn sample_texel(
    pixels: &[u8],
    width: u32,
    format: TextureStorageFormat,
    x: u32,
    y: u32,
) -> [u16; 4] {
    let bpp = format.bytes_per_pixel();
    let stride = width as usize * bpp;
    let offset = y as usize * stride + x as usize * bpp;
    let slice = pixels.get(offset..offset + bpp).unwrap_or(&[]);
    match format {
        TextureStorageFormat::Rgba16Unorm => {
            if slice.len() < 8 {
                return [0; 4];
            }
            let r = u16::from_le_bytes([slice[0], slice[1]]);
            let g = u16::from_le_bytes([slice[2], slice[3]]);
            let b = u16::from_le_bytes([slice[4], slice[5]]);
            let a = u16::from_le_bytes([slice[6], slice[7]]);
            [r, g, b, a]
        }
        TextureStorageFormat::Rgb16Unorm => {
            if slice.len() < 6 {
                return [0; 4];
            }
            let r = u16::from_le_bytes([slice[0], slice[1]]);
            let g = u16::from_le_bytes([slice[2], slice[3]]);
            let b = u16::from_le_bytes([slice[4], slice[5]]);
            [r, g, b, u16::MAX]
        }
        TextureStorageFormat::R16Unorm => {
            if slice.len() < 2 {
                return [0; 4];
            }
            let r = u16::from_le_bytes([slice[0], slice[1]]);
            [r, r, r, u16::MAX]
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::{TextureRenderProduct, TextureRenderProductError};
    use crate::visual_product::{VisualSampleBatch, VisualSamplePoint};

    fn pixel_rgba16(r: u16, g: u16, b: u16, a: u16) -> [u8; 8] {
        let mut out = [0u8; 8];
        out[0..2].copy_from_slice(&r.to_le_bytes());
        out[2..4].copy_from_slice(&g.to_le_bytes());
        out[4..6].copy_from_slice(&b.to_le_bytes());
        out[6..8].copy_from_slice(&a.to_le_bytes());
        out
    }

    #[test]
    fn texture_product_insert_sample_metadata_raw_bytes() {
        // 2×2 RGBA16: TL red, TR green, BL blue, BR white
        let mut px = vec![0u8; 32];
        px[0..8].copy_from_slice(&pixel_rgba16(65535, 0, 0, 65535));
        px[8..16].copy_from_slice(&pixel_rgba16(0, 65535, 0, 65535));
        px[16..24].copy_from_slice(&pixel_rgba16(0, 0, 65535, 65535));
        px[24..32].copy_from_slice(&pixel_rgba16(65535, 65535, 65535, 65535));

        let tex = TextureRenderProduct::rgba16_unorm(2, 2, px).expect("valid texture");
        assert_eq!(tex.width(), 2);
        assert_eq!(tex.height(), 2);
        assert_eq!(
            tex.storage_format(),
            lps_shared::TextureStorageFormat::Rgba16Unorm
        );
        assert_eq!(tex.try_raw_bytes().map(<[_]>::len), Some(32));

        let batch = VisualSampleBatch {
            points: vec![
                VisualSamplePoint { x: 0, y: 0 },
                VisualSamplePoint { x: 1, y: 0 },
                VisualSamplePoint { x: 0, y: 1 },
                VisualSamplePoint { x: 1, y: 1 },
            ],
        };
        let out = tex.sample_batch(&batch);
        assert_eq!(out.samples.len(), 4);
        assert_eq!(out.samples[0].rgba_unorm16, [65535, 0, 0, 65535]);
        assert_eq!(out.samples[1].rgba_unorm16, [0, 65535, 0, 65535]);
        assert_eq!(out.samples[2].rgba_unorm16, [0, 0, 65535, 65535]);
        assert_eq!(out.samples[3].rgba_unorm16, [65535, 65535, 65535, 65535]);
    }

    #[test]
    fn rejects_bad_byte_length() {
        let err = TextureRenderProduct::rgba16_unorm(2, 2, vec![0u8; 31]).expect_err("short buf");
        assert!(matches!(
            err,
            TextureRenderProductError::ByteLenMismatch {
                expected: 32,
                actual: 31
            }
        ));
    }

    #[test]
    fn rejects_zero_dimension() {
        let err = TextureRenderProduct::rgba16_unorm(0, 4, vec![]).expect_err("zero width");
        assert!(matches!(
            err,
            TextureRenderProductError::ZeroDimension {
                width: 0,
                height: 4
            }
        ));
    }
}
