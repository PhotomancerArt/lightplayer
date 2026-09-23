//! The browser tier's [`lp_gfx::LpGraphics::read_back_latent`]: whole-texture
//! readback one frame late.
//!
//! The same shape as the sample pass's pipeline (`crate::sample_pass`, ADR
//! `2026-08-05-browser-sample-readback-is-async.md`), applied to a whole
//! texture for the wire probe's byte previews: a persistent `MAP_READ`
//! buffer per read site, held in the caller's [`LatentReadBack`]. Each call
//!
//! 1. harvests the previous call's `map_async` if it has landed (the
//!    worker's event loop turns between reads, resolving the map promise),
//! 2. issues a copy + map of *this* call's texture when the buffer is free,
//! 3. serves the most recent landed frame, tagged with the tag it was
//!    issued under — or nothing, until the first one lands.
//!
//! Native keeps the trait's synchronous default: it can block on a map.

use std::sync::{Arc, Mutex};

use lp_gfx::{GfxError, LatentReadBack};
use lps_shared::TextureStorageFormat;

use crate::texture_backing::{GpuTexture, f32_to_texels, gpu_channels};

/// Where a `map_async` callback writes its answer. Single-threaded wasm:
/// the lock is never contended.
type MapState = Arc<Mutex<Option<Result<(), wgpu::BufferAsyncError>>>>;

/// One read site's staging resources, keyed by the texture shape they were
/// built for.
struct Staging {
    width: u32,
    height: u32,
    format: TextureStorageFormat,
    buffer: wgpu::Buffer,
    padded_bytes_per_row: u32,
    /// The outstanding map and the tag of the frame it copies.
    pending: Option<(MapState, u64)>,
    /// The most recent landed frame (logical texel bytes) and its tag.
    last: Option<(Vec<u8>, u64)>,
}

/// Serve `backing`'s bytes one frame late (see module docs).
#[allow(
    clippy::too_many_arguments,
    reason = "mirrors read_back_texture's shape"
)]
pub(crate) fn read_back_latent(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    backing: &GpuTexture,
    width: u32,
    height: u32,
    format: TextureStorageFormat,
    state: &mut LatentReadBack,
    tag: u64,
    out: &mut [u8],
) -> Result<Option<u64>, GfxError> {
    let expected = width as usize * height as usize * format.bytes_per_pixel();
    if out.len() != expected {
        return Err(GfxError::Backend(format!(
            "latent read_back bytes: expected {expected}, got {}",
            out.len()
        )));
    }
    let reusable = state
        .backing_mut()
        .and_then(|staged| staged.downcast_mut::<Staging>())
        .is_some_and(|staging| {
            staging.width == width && staging.height == height && staging.format == format
        });
    if !reusable {
        state.set_backing(Box::new(Staging::new(device, width, height, format)));
    }
    let staging = state
        .backing_mut()
        .and_then(|staged| staged.downcast_mut::<Staging>())
        .expect("staging was just installed");

    staging.harvest()?;
    if staging.pending.is_none() {
        staging.issue(device, queue, backing, tag);
    }
    Ok(staging.last.as_ref().map(|(bytes, served)| {
        out.copy_from_slice(bytes);
        *served
    }))
}

impl Staging {
    fn new(device: &wgpu::Device, width: u32, height: u32, format: TextureStorageFormat) -> Self {
        let bytes_per_pixel = gpu_channels(format) as u32 * 4;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded_bytes_per_row = (width * bytes_per_pixel).div_ceil(align) * align;
        Self {
            width,
            height,
            format,
            buffer: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("lp-gfx-wgpu latent read_back"),
                size: u64::from(padded_bytes_per_row) * u64::from(height),
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            }),
            padded_bytes_per_row,
            pending: None,
            last: None,
        }
    }

    /// Take the outstanding map's answer if it has landed; a map still in
    /// flight (slow frame) leaves `last` as it is.
    fn harvest(&mut self) -> Result<(), GfxError> {
        let Some((map, tag)) = &self.pending else {
            return Ok(());
        };
        let tag = *tag;
        let landed = map.lock().expect("map state (uncontended)").take();
        match landed {
            None => Ok(()),
            Some(Err(error)) => {
                self.pending = None;
                Err(GfxError::Backend(format!(
                    "latent read_back buffer map: {error:?}"
                )))
            }
            Some(Ok(())) => {
                let unpadded = (self.width * gpu_channels(self.format) as u32 * 4) as usize;
                let mut pixels = Vec::with_capacity(unpadded / 4 * self.height as usize);
                {
                    let data = self.buffer.slice(..).get_mapped_range();
                    for row in 0..self.height {
                        let start = (row * self.padded_bytes_per_row) as usize;
                        for chunk in data[start..start + unpadded].chunks_exact(4) {
                            pixels
                                .push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
                        }
                    }
                }
                self.buffer.unmap();
                self.pending = None;
                self.last = Some((f32_to_texels(self.format, &pixels), tag));
                Ok(())
            }
        }
    }

    /// Copy `backing` into the staging buffer and start its map.
    fn issue(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        backing: &GpuTexture,
        tag: u64,
    ) {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &backing.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &self.buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(self.padded_bytes_per_row),
                    rows_per_image: None,
                },
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
        queue.submit([encoder.finish()]);

        let map: MapState = Arc::new(Mutex::new(None));
        let callback_map = Arc::clone(&map);
        self.buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                *callback_map.lock().expect("map state (uncontended)") = Some(result);
            });
        self.pending = Some((map, tag));
    }
}
