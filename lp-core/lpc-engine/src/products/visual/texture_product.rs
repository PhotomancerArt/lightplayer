//! Texture result materialized from a [`super::VisualProduct`] — host bytes
//! or a GPU-resident handle.

use core::fmt;

use lp_gfx::TextureHandle;
use lps_shared::TextureStorageFormat;

use super::{TextureSampleBatch, VisualSample, VisualSampleBatchResult, texture_uv_q16_to_texel};

/// Invalid [`TextureRenderProduct`] construction input or an operation that
/// requires host-resident bytes on a GPU-resident product.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextureRenderProductError {
    ZeroDimension {
        width: u32,
        height: u32,
    },
    ByteLenMismatch {
        expected: usize,
        actual: usize,
    },
    /// The product is GPU-resident: no host bytes to operate on
    /// (fidelity-tiers ADR — byte-needing consumers run on the CPU tier).
    GpuResident,
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
            Self::GpuResident => write!(
                f,
                "texture product is GPU-resident (no host bytes on the GPU tier)"
            ),
        }
    }
}

impl core::error::Error for TextureRenderProductError {}

/// Where a texture product's texels live.
#[derive(Debug)]
enum TexturePixels {
    /// Host-resident tightly packed texel bytes (CPU tier).
    Host(alloc::vec::Vec<u8>),
    /// GPU-resident render target (GPU tier). The handle is RAII-owned by
    /// the product; presentation blits it to a surface without readback.
    Gpu(TextureHandle),
}

/// Texture-backed visual product: host bytes on CPU backends, an opaque
/// [`TextureHandle`] on GPU-resident backends (no `LpsTextureBuf`, no
/// backend pointers in the public API).
///
/// Byte consumers use [`Self::try_raw_bytes`] and must handle `None` for
/// GPU-resident products (the wire probe edge maps it to a structured
/// answer). Sample coordinates in [`TextureSampleBatch`] are interpreted as
/// normalized Q16.16 values and converted to integer texels with
/// clamp-to-edge behavior.
#[derive(Debug)]
pub struct TextureRenderProduct {
    width: u32,
    height: u32,
    format: TextureStorageFormat,
    pixels: TexturePixels,
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
    ///
    /// `None` for GPU-resident products.
    #[must_use]
    pub fn try_raw_bytes(&self) -> Option<&[u8]> {
        match &self.pixels {
            TexturePixels::Host(pixels) => Some(pixels.as_slice()),
            TexturePixels::Gpu(_) => None,
        }
    }

    /// The GPU render target behind a GPU-resident product.
    ///
    /// `None` for host-resident products. Presentation edges pass this to
    /// their backend's surface-present op; the handle is only valid with
    /// the backend that rendered the product.
    #[must_use]
    pub fn gpu_handle(&self) -> Option<&TextureHandle> {
        match &self.pixels {
            TexturePixels::Host(_) => None,
            TexturePixels::Gpu(handle) => Some(handle),
        }
    }

    /// Builds a host-resident product after validating dimensions and byte length.
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
            pixels: TexturePixels::Host(pixels),
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

    /// Builds a GPU-resident product around a rendered target handle.
    ///
    /// Used by render paths on backends where
    /// `LpGraphics::supports_read_back()` is `false` (browser GPU tier):
    /// the product keeps the texture on the GPU and presentation happens
    /// via surface blit, never bytes.
    pub fn gpu_resident(handle: TextureHandle) -> Result<Self, TextureRenderProductError> {
        let (width, height) = (handle.width(), handle.height());
        if width == 0 || height == 0 {
            return Err(TextureRenderProductError::ZeroDimension { width, height });
        }
        Ok(Self {
            width,
            height,
            format: handle.format(),
            pixels: TexturePixels::Gpu(handle),
        })
    }
}

impl TextureRenderProduct {
    /// Sample texels at normalized Q16.16 UV points.
    ///
    /// Errors with [`TextureRenderProductError::GpuResident`] on GPU-resident
    /// products — sampling sinks run on the CPU tier.
    pub fn sample_batch(
        &self,
        request: &TextureSampleBatch,
    ) -> Result<VisualSampleBatchResult, TextureRenderProductError> {
        let pixels = match &self.pixels {
            TexturePixels::Host(pixels) => pixels,
            TexturePixels::Gpu(_) => return Err(TextureRenderProductError::GpuResident),
        };
        let mut samples = alloc::vec::Vec::with_capacity(request.points.len());
        for p in &request.points {
            let tx = texture_uv_q16_to_texel(p.u_q16, self.width);
            let ty = texture_uv_q16_to_texel(p.v_q16, self.height);
            let (tx, ty) = clamp_texel(tx, ty, self.width, self.height);
            let color = sample_texel(pixels, self.width, self.format, tx, ty);
            samples.push(VisualSample {
                rgba_unorm16: color,
            });
        }
        Ok(VisualSampleBatchResult { samples })
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
    use crate::products::visual::{TextureSampleBatch, TextureUvSamplePoint};

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

        let batch = TextureSampleBatch {
            points: vec![
                TextureUvSamplePoint { u_q16: 0, v_q16: 0 },
                TextureUvSamplePoint {
                    u_q16: 32768,
                    v_q16: 0,
                },
                TextureUvSamplePoint {
                    u_q16: 0,
                    v_q16: 32768,
                },
                TextureUvSamplePoint {
                    u_q16: 32768,
                    v_q16: 32768,
                },
            ],
            time_seconds: 0.0,
        };
        let out = tex.sample_batch(&batch).expect("host product samples");
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

    #[test]
    fn gpu_resident_product_has_no_bytes_and_refuses_sampling() {
        let tex = TextureRenderProduct::gpu_resident(test_handle(3, 2)).expect("gpu product");
        assert_eq!(tex.width(), 3);
        assert_eq!(tex.height(), 2);
        assert_eq!(
            tex.storage_format(),
            lps_shared::TextureStorageFormat::Rgba16Unorm
        );
        assert!(tex.try_raw_bytes().is_none());
        assert!(tex.gpu_handle().is_some());

        let batch = TextureSampleBatch {
            points: vec![TextureUvSamplePoint { u_q16: 0, v_q16: 0 }],
            time_seconds: 0.0,
        };
        assert!(matches!(
            tex.sample_batch(&batch),
            Err(TextureRenderProductError::GpuResident)
        ));
    }

    #[test]
    fn host_product_exposes_no_gpu_handle() {
        let tex = TextureRenderProduct::rgba16_unorm(1, 1, vec![0u8; 8]).expect("host product");
        assert!(tex.gpu_handle().is_none());
    }

    fn test_handle(width: u32, height: u32) -> lp_gfx::TextureHandle {
        struct NoopAllocator;
        impl lp_gfx::HandleAllocator for NoopAllocator {
            fn free_texture(&self, backing: lp_gfx::HandleBacking) {
                drop(backing);
            }
            fn free_sample_points(&self, backing: lp_gfx::HandleBacking) {
                drop(backing);
            }
            fn free_sample_out(&self, backing: lp_gfx::HandleBacking) {
                drop(backing);
            }
        }
        lp_gfx::TextureHandle::from_backend_parts(
            width,
            height,
            lps_shared::TextureStorageFormat::Rgba16Unorm,
            alloc::boxed::Box::new(()),
            alloc::sync::Arc::new(NoopAllocator),
        )
    }
}
