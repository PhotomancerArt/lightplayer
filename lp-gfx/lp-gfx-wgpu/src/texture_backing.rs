//! wgpu texture backing behind `lp_gfx::TextureHandle`, plus the CPU↔GPU
//! texel conversions.
//!
//! # Backing format
//!
//! Logical `TextureStorageFormat` values are unorm16 CPU layouts; the GPU
//! backing stores them as **32-bit float** textures:
//!
//! | logical | wgpu backing |
//! |---|---|
//! | `Rgba16Unorm` | `Rgba32Float` |
//! | `Rgb16Unorm` | `Rgba32Float` (alpha lane padded, dropped on readback) |
//! | `R16Unorm` | `R32Float` |
//!
//! Rationale: WebGPU (the first deployment target) has no renderable
//! 16-bit-unorm formats (`Rgba16Unorm` is a native-only wgpu feature), and
//! `Rgba16Float` would cost preview precision. Rendering at f32 and
//! quantizing with the CPU tier's exact packing rule (`trunc(v · 65536)`
//! saturated to 65535) on the readback/blend boundary is exactly the
//! spike-proven configuration behind the m3-report parity numbers.
//! Conversions round-trip bit-exactly: `v/65536` is exact in f32 for all
//! `v ≤ 65535`, and `floor(v/65536 · 65536) = v`.

use lp_gfx::{GfxError, TextureHandle};
use lps_shared::TextureStorageFormat;

/// GPU allocation behind a [`TextureHandle`].
pub(crate) struct GpuTexture {
    pub(crate) texture: wgpu::Texture,
    pub(crate) view: wgpu::TextureView,
    /// Registry id minted by [`crate::texture_registry::TextureRegistry`]
    /// (`0` until registered); carried in the `ptr` lane of the texture's
    /// uniform descriptor so shaders can resolve the view at render time.
    pub(crate) id: u32,
}

impl GpuTexture {
    /// Allocate a zero-initialized backing texture (wgpu zero-init).
    pub(crate) fn new(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        format: TextureStorageFormat,
        label: &str,
    ) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: gpu_format(format),
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            texture,
            view,
            id: 0,
        }
    }

    /// Upload logical-format texel bytes (little-endian unorm16 channels)
    /// into the float backing.
    pub(crate) fn upload(
        &self,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        format: TextureStorageFormat,
        texels: &[u8],
    ) -> Result<(), GfxError> {
        let expected = width as usize * height as usize * format.bytes_per_pixel();
        if texels.len() != expected {
            return Err(GfxError::Backend(format!(
                "texture texels write length mismatch: expected {expected}, got {}",
                texels.len()
            )));
        }
        let pixels = texels_to_f32(format, texels);
        self.upload_f32(queue, width, height, format, &pixels);
        Ok(())
    }

    /// Upload raw backing-format floats (`gpu_channels(format)` per pixel).
    pub(crate) fn upload_f32(
        &self,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        format: TextureStorageFormat,
        pixels: &[f32],
    ) {
        let bytes: Vec<u8> = pixels.iter().flat_map(|v| v.to_le_bytes()).collect();
        queue.write_texture(
            self.texture.as_image_copy(),
            &bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * gpu_channels(format) as u32 * 4),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
    }
}

/// wgpu backing format for a logical texture format.
pub(crate) fn gpu_format(format: TextureStorageFormat) -> wgpu::TextureFormat {
    match format {
        TextureStorageFormat::Rgba16Unorm | TextureStorageFormat::Rgb16Unorm => {
            wgpu::TextureFormat::Rgba32Float
        }
        TextureStorageFormat::R16Unorm => wgpu::TextureFormat::R32Float,
    }
}

/// Channels stored per pixel in the GPU backing (≥ the logical channel
/// count; `Rgb16Unorm` pads an alpha lane).
pub(crate) fn gpu_channels(format: TextureStorageFormat) -> usize {
    match format {
        TextureStorageFormat::Rgba16Unorm | TextureStorageFormat::Rgb16Unorm => 4,
        TextureStorageFormat::R16Unorm => 1,
    }
}

/// Convert logical unorm16 texel bytes to backing floats (`v / 65536`,
/// exact; `Rgb16Unorm` pads alpha with 1.0).
pub(crate) fn texels_to_f32(format: TextureStorageFormat, texels: &[u8]) -> Vec<f32> {
    let logical = format.channel_count();
    let backing = gpu_channels(format);
    let mut out = Vec::with_capacity(texels.len() / 2 / logical * backing);
    for pixel in texels.chunks_exact(2 * logical) {
        for channel in pixel.chunks_exact(2) {
            let v = u16::from_le_bytes([channel[0], channel[1]]);
            out.push(f32::from(v) / 65536.0);
        }
        for _ in logical..backing {
            out.push(1.0);
        }
    }
    out
}

/// Quantize backing floats to logical unorm16 texel bytes with the CPU
/// path's exact packing rule: `trunc(v · 65536)` saturated to `[0, 65535]`
/// (1.0 maps to 65535; non-finite lanes map to 0, matching the spike).
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn f32_to_texels(format: TextureStorageFormat, pixels: &[f32]) -> Vec<u8> {
    let logical = format.channel_count();
    let backing = gpu_channels(format);
    let mut out = Vec::with_capacity(pixels.len() / backing * logical * 2);
    for pixel in pixels.chunks_exact(backing) {
        for &v in &pixel[..logical] {
            out.extend_from_slice(&quantize_unorm16(v).to_le_bytes());
        }
    }
    out
}

/// The CPU packing rule for one lane (see [`f32_to_texels`]).
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn quantize_unorm16(v: f32) -> u16 {
    let raw = (f64::from(v) * 65536.0).floor();
    raw.clamp(0.0, 65535.0) as u16
}

/// Downcast a texture handle to its GPU backing.
pub(crate) fn gpu_texture(handle: &TextureHandle) -> Result<&GpuTexture, GfxError> {
    handle
        .backing()
        .downcast_ref::<GpuTexture>()
        .ok_or_else(foreign_handle)
}

/// Mutable variant of [`gpu_texture`] (same backing; kept for parity with
/// `&mut TextureHandle` trait signatures).
pub(crate) fn gpu_texture_mut(handle: &mut TextureHandle) -> Result<&mut GpuTexture, GfxError> {
    handle
        .backing_mut()
        .downcast_mut::<GpuTexture>()
        .ok_or_else(foreign_handle)
}

pub(crate) fn foreign_handle() -> GfxError {
    GfxError::Backend(String::from(
        "handle does not belong to this wgpu graphics backend",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantize_matches_the_cpu_packing_rule() {
        assert_eq!(quantize_unorm16(0.25), 16384);
        assert_eq!(quantize_unorm16(1.0), 65535);
        assert_eq!(quantize_unorm16(-0.5), 0);
        assert_eq!(quantize_unorm16(0.5), 32768);
        assert_eq!(quantize_unorm16(f32::NAN), 0);
        assert_eq!(quantize_unorm16(f32::INFINITY), 65535);
    }

    #[test]
    fn texel_conversion_round_trips_exactly() {
        for format in [
            TextureStorageFormat::Rgba16Unorm,
            TextureStorageFormat::Rgb16Unorm,
            TextureStorageFormat::R16Unorm,
        ] {
            let channels = format.channel_count();
            let values: Vec<u16> = (0..channels * 3)
                .map(|i| [0u16, 1, 32767, 32768, 65534, 65535][i % 6])
                .collect();
            let bytes: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
            let floats = texels_to_f32(format, &bytes);
            assert_eq!(floats.len(), 3 * gpu_channels(format), "{format:?}");
            let back = f32_to_texels(format, &floats);
            assert_eq!(back, bytes, "{format:?} round trip");
        }
    }
}
