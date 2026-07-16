//! Texture readback policy (GPU-residency doctrine).
//!
//! `read_back` exists for sinks that inherently need bytes (fixture
//! sampling, wire probes) — never for transforms, which belong behind
//! GPU-resident trait ops like `blend_textures`.
//!
//! - **native**: copy to a mapped buffer and block on
//!   `device.poll(wait)` — bounded and synchronous; the native server host
//!   can afford it (LED output path).
//! - **wasm32**: explicit `GfxError::Backend` — the browser cannot block on
//!   a map, the gallery never reads back, and probes/wire run on the CPU
//!   tier. A deferred/async readback API will be designed when a real
//!   browser consumer appears.

use lp_gfx::{GfxError, TextureData};
use lps_shared::TextureStorageFormat;

use crate::texture_backing::GpuTexture;

/// Read a GPU texture back as logical-format bytes (native).
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn read_back_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    backing: &GpuTexture,
    width: u32,
    height: u32,
    format: TextureStorageFormat,
) -> Result<TextureData, GfxError> {
    use crate::texture_backing::f32_to_texels;

    let pixels = read_back_f32(device, queue, backing, width, height, format, None)?;
    Ok(TextureData::new(
        width,
        height,
        format,
        f32_to_texels(format, &pixels),
    ))
}

/// Read the raw backing floats (pre-quantization) — the conformance/probe
/// path (e.g. non-finite-lane detection, which quantization would mask).
///
/// `timeout`: bound the device wait. The filetest probe passes a bound
/// because corpus shaders may not terminate (CPU targets rely on fuel
/// exhaustion; the GPU has none — an unbounded wait hangs the process).
/// Product paths pass `None` (wait indefinitely, matching `read_back`).
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn read_back_f32(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    backing: &GpuTexture,
    width: u32,
    height: u32,
    format: TextureStorageFormat,
    timeout: Option<core::time::Duration>,
) -> Result<Vec<f32>, GfxError> {
    use crate::texture_backing::gpu_channels;

    let bytes_per_pixel = gpu_channels(format) as u32 * 4;
    let unpadded_bytes_per_row = width * bytes_per_pixel;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let padded_bytes_per_row = unpadded_bytes_per_row.div_ceil(align) * align;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("lp-gfx-wgpu read_back"),
        size: u64::from(padded_bytes_per_row) * u64::from(height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &backing.texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_bytes_per_row),
                rows_per_image: None,
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    let submission = queue.submit([encoder.finish()]);

    let slice = buffer.slice(..);
    let (map_tx, map_rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = map_tx.send(result);
    });
    // Wait for THIS submission specifically. `submission_index: None` waits
    // for "the most recent submission at poll time", which starves under
    // concurrent submitters (parallel filetest workers keep moving the
    // goalpost) — the probe path deadlocked on exactly that.
    device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout,
        })
        .map_err(|e| GfxError::Backend(format!("read_back device poll: {e:?}")))?;
    // Map failures (e.g. device lost after an earlier hung submission)
    // surface as errors, never a panic inside the wgpu callback.
    match map_rx.try_recv() {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            return Err(GfxError::Backend(format!("read_back buffer map: {e:?}")));
        }
        Err(_) => {
            return Err(GfxError::Backend(String::from(
                "read_back buffer map did not complete after device poll",
            )));
        }
    }

    let mut pixels =
        Vec::with_capacity(width as usize * height as usize * bytes_per_pixel as usize / 4);
    {
        let data = slice.get_mapped_range();
        for row in 0..height {
            let start = (row * padded_bytes_per_row) as usize;
            let row_bytes = &data[start..start + unpadded_bytes_per_row as usize];
            for chunk in row_bytes.chunks_exact(4) {
                pixels.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
            }
        }
    }
    buffer.unmap();
    Ok(pixels)
}

/// Browser GPU tier: readback is unavailable by policy (see module docs).
#[cfg(target_arch = "wasm32")]
pub(crate) fn read_back_texture(
    _device: &wgpu::Device,
    _queue: &wgpu::Queue,
    _backing: &GpuTexture,
    _width: u32,
    _height: u32,
    _format: TextureStorageFormat,
) -> Result<TextureData, GfxError> {
    Err(GfxError::Backend(String::from(
        "read_back unavailable on the browser GPU tier; sinks needing bytes run on the CPU tier",
    )))
}
