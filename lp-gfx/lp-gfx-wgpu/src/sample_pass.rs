//! GPU sample-point pass: evaluate a compiled shader at caller-provided
//! Q16.16 pixel-space points (the LED-output path — thousands of points per
//! tick, not megapixels).
//!
//! # Shape
//!
//! One **point-list draw into a row-major `W × H` grid target** (`W =
//! min(count, max_texture_dimension_2d)`, `H = ceil(count / W)` — a single
//! row for small sets, wrapping into rows at device-limit scale, e.g. the
//! ~30k-LED radiance dome): vertex `i` carries its own clip-space position
//! (precomputed on the CPU as the center of grid texel `i`) plus the
//! pixel-space sample position as a second attribute, passed to the
//! fragment stage as a varying. The fragment `main` evaluates
//! `render(lp_gfx_sample_pos)` — see
//! [`crate::assembly::assemble_sample_fragment_glsl`]. Point primitives
//! rasterize exactly one fragment each and interpolate nothing, so every
//! target texel receives `render` at exactly the caller's point.
//!
//! Carrying the position as a vertex attribute (instead of a storage/uniform
//! buffer indexed by `gl_FragCoord`) keeps the authored shader's `@group(0)`
//! uniform interface untouched: no reserved binding slots, and the sample
//! pipeline reuses the render pipeline's bind group layout and uniform
//! buffer as-is.
//!
//! # Readback
//!
//! Results quantize with the CPU packing rule into the caller's RGBA16
//! buffer, but how the bytes leave the GPU differs by target:
//!
//! - **native**: the blocking buffer map in [`crate::read_back`] —
//!   synchronous, same-frame results (the LED-output path of the
//!   non-embedded lp-server).
//! - **wasm32**: the browser cannot block on a buffer map, so the pass
//!   keeps a persistent `MAP_READ` buffer per point count and runs it as a
//!   **one-frame-latency pipeline**: each call submits this frame's draw,
//!   harvests the previous call's `map_async` result if it has landed (the
//!   worker's event loop turns between ticks, resolving the map promise),
//!   issues a copy+map for the frame just drawn when the buffer is free,
//!   and serves the most recent completed frame (black until the first
//!   readback lands). This is the async-readback exit of
//!   `docs/debt/gpu-tier-cannot-sample-led-output.md`.

use lp_gfx::GfxError;
use lps_shared::TextureStorageFormat;

use crate::gpu_graphics::GpuShared;
#[cfg(not(target_arch = "wasm32"))]
use crate::read_back::read_back_f32;
#[cfg(target_arch = "wasm32")]
use crate::texture_backing::gpu_channels;
use crate::texture_backing::{GpuTexture, gpu_format, quantize_unorm16};

/// Hand-written point-list vertex stage. Attribute 0 is the precomputed
/// clip-space position of target grid texel `i`; attribute 1 is the
/// pixel-space sample position forwarded to the fragment stage.
const SAMPLE_VERTEX_WGSL: &str = "
struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) sample_pos: vec2<f32>,
}

@vertex
fn vs_main(@location(0) clip_pos: vec2<f32>, @location(1) point: vec2<f32>) -> VsOut {
    var out: VsOut;
    out.position = vec4<f32>(clip_pos, 0.0, 1.0);
    out.sample_pos = point;
    return out;
}
";

/// Bytes per sample vertex: `clip_pos: vec2<f32>` + `point: vec2<f32>`.
const VERTEX_STRIDE: u64 = 16;

/// The compiled sample pipeline plus its per-count resources. Built lazily
/// on the first `sample_rgba16` call (render-only consumers — the gallery —
/// never pay for it).
///
/// Resources are kept **per point count** (most-recently-used at the back,
/// capped at [`MAX_RESOURCE_SETS`]): one shader instance serves every
/// fixture sampling its product, so calls with different LED counts
/// interleave within a frame. A single rebuilt-on-change set would churn
/// buffers every call on native and, on wasm32, destroy the in-flight
/// async readback before it ever landed — permanently black.
pub(crate) struct SamplePass {
    pipeline: wgpu::RenderPipeline,
    resources: Vec<SampleResources>,
}

/// Cap on retained per-count resource sets. Distinct counts come from
/// distinct fixtures (few) and change only under editing; past the cap the
/// least-recently-used set is dropped.
const MAX_RESOURCE_SETS: usize = 8;

/// Vertex buffer and row-major `W × H` grid target for one point count.
struct SampleResources {
    count: u32,
    width: u32,
    height: u32,
    vertex_buffer: wgpu::Buffer,
    target: GpuTexture,
    /// Browser async readback state (see module docs); dropped with the
    /// rest of the set on LRU eviction, which also abandons any in-flight
    /// map (the dropped buffer's callback resolves into a state handle
    /// nobody reads).
    #[cfg(target_arch = "wasm32")]
    readback: AsyncReadback,
}

/// One-frame-latency readback pipeline for the browser tier: a persistent
/// `MAP_READ` buffer plus the most recent completed frame.
#[cfg(target_arch = "wasm32")]
struct AsyncReadback {
    buffer: wgpu::Buffer,
    padded_bytes_per_row: u32,
    /// `Some` while a `map_async` is outstanding; the callback writes the
    /// map result into the shared cell (single-threaded wasm — the lock is
    /// never contended).
    pending: Option<std::sync::Arc<std::sync::Mutex<Option<Result<(), wgpu::BufferAsyncError>>>>>,
    /// Most recent completed frame, quantized (`count * 4` channels).
    /// Zeros — black — until the first readback lands.
    last: Vec<u16>,
}

impl SamplePass {
    /// Build the sample pipeline around the naga-translated sample fragment
    /// module (`entry_point = "main"`), reusing the shader's uniform bind
    /// group layout so the render path's bind group binds unchanged.
    pub(crate) fn new(
        shared: &GpuShared,
        sample_fragment_wgsl: &str,
        uniform_layout: Option<&wgpu::BindGroupLayout>,
    ) -> Self {
        let device = &shared.device;
        let fragment_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("lp-gfx-wgpu sample fragment (authored GLSL via naga wgsl-out)"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(sample_fragment_wgsl)),
        });
        let vertex_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("lp-gfx-wgpu sample points"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(SAMPLE_VERTEX_WGSL)),
        });

        let layouts: Vec<Option<&wgpu::BindGroupLayout>> =
            uniform_layout.iter().map(|layout| Some(*layout)).collect();
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("lp-gfx-wgpu sample"),
            bind_group_layouts: &layouts,
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("lp-gfx-wgpu sample"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &vertex_module,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: VERTEX_STRIDE,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 0,
                            shader_location: 0,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 8,
                            shader_location: 1,
                        },
                    ],
                }],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::PointList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &fragment_module,
                entry_point: Some("main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: gpu_format(TextureStorageFormat::Rgba16Unorm),
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        Self {
            pipeline,
            resources: Vec::new(),
        }
    }

    /// Evaluate the shader at `points_q16` (`count × 2` Q16.16 coordinates)
    /// and quantize the results into `out` (`count × 4` RGBA16 channels).
    /// The caller has already written the uniform buffer behind
    /// `bind_group`.
    pub(crate) fn run(
        &mut self,
        shared: &GpuShared,
        points_q16: &[i32],
        bind_group: Option<&wgpu::BindGroup>,
        out: &mut [u16],
    ) -> Result<(), GfxError> {
        debug_assert_eq!(points_q16.len() % 2, 0);
        debug_assert_eq!(out.len(), points_q16.len() * 2);
        let count = (points_q16.len() / 2) as u32;
        if count == 0 {
            return Ok(());
        }
        let max_dim = shared.device.limits().max_texture_dimension_2d;
        if u64::from(count) > u64::from(max_dim) * u64::from(max_dim) {
            return Err(GfxError::Render(format!(
                "sample_rgba16: {count} points exceed the device's maximum sample-target grid \
                 ({max_dim} x {max_dim})"
            )));
        }

        self.ensure_resources(shared, count);
        let resources = self
            .resources
            .last()
            .expect("sample resources were just ensured");
        let (width, height) = (resources.width, resources.height);

        // Vertex i: clip-space center of grid texel (i % W, i / W), then the
        // Q16.16 point as f32 pixel coordinates (exact for |coord| < 2^24
        // texels). Texel row 0 is the top of the target, which is clip-space
        // +y, so rows map top-down.
        let mut vertices = Vec::with_capacity(points_q16.len() / 2 * 4);
        for (i, point) in points_q16.chunks_exact(2).enumerate() {
            let col = i as u32 % width;
            let row = i as u32 / width;
            let clip_x = (col as f32 + 0.5) / width as f32 * 2.0 - 1.0;
            let clip_y = 1.0 - (row as f32 + 0.5) / height as f32 * 2.0;
            vertices.push(clip_x);
            vertices.push(clip_y);
            vertices.push((f64::from(point[0]) / 65536.0) as f32);
            vertices.push((f64::from(point[1]) / 65536.0) as f32);
        }
        let vertex_bytes: Vec<u8> = vertices.iter().flat_map(|v| v.to_le_bytes()).collect();
        shared
            .queue
            .write_buffer(&resources.vertex_buffer, 0, &vertex_bytes);

        let mut encoder = shared
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("lp-gfx-wgpu sample"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &resources.target.view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            if let Some(bind_group) = bind_group {
                pass.set_bind_group(0, bind_group, &[]);
            }
            pass.set_vertex_buffer(0, resources.vertex_buffer.slice(..));
            pass.draw(0..count, 0..1);
        }
        shared.queue.submit([encoder.finish()]);

        self.read_back_into(shared, out)
    }

    /// Native: blocking row-major readback of the frame just drawn. Texel
    /// (col, row) is index row * W + col, so channels line up with `out`
    /// directly; the final row's unused tail (count not divisible by W)
    /// falls off the zip.
    #[cfg(not(target_arch = "wasm32"))]
    fn read_back_into(&mut self, shared: &GpuShared, out: &mut [u16]) -> Result<(), GfxError> {
        let resources = self
            .resources
            .last()
            .expect("sample resources were just ensured");
        let pixels = read_back_f32(
            &shared.device,
            &shared.queue,
            &resources.target,
            resources.width,
            resources.height,
            TextureStorageFormat::Rgba16Unorm,
            None,
        )?;
        for (dst, &v) in out.iter_mut().zip(&pixels) {
            *dst = quantize_unorm16(v);
        }
        Ok(())
    }

    /// Browser: one-frame-latency pipeline (see module docs). Harvest the
    /// previous call's map if it has landed, issue a copy+map for the frame
    /// just drawn when the buffer is free, and serve the most recent
    /// completed frame.
    #[cfg(target_arch = "wasm32")]
    fn read_back_into(&mut self, shared: &GpuShared, out: &mut [u16]) -> Result<(), GfxError> {
        use std::sync::{Arc, Mutex};

        let resources = self
            .resources
            .last_mut()
            .expect("sample resources were just ensured");
        let readback = &mut resources.readback;

        // Harvest: did the previous call's map land?
        if let Some(state) = &readback.pending {
            let landed = state.lock().expect("map state (uncontended)").take();
            match landed {
                // Still in flight (slow frame) — serve the stale frame and
                // try again next call.
                None => {}
                Some(Err(e)) => {
                    readback.pending = None;
                    return Err(GfxError::Backend(format!(
                        "sample read_back buffer map: {e:?}"
                    )));
                }
                Some(Ok(())) => {
                    let slice = readback.buffer.slice(..);
                    let data = slice.get_mapped_range();
                    // Row-major grid readback, same layout as native; only
                    // the first `count * 4` channels are live (the final
                    // row's unused tail is dropped by the `last` bound).
                    let row_channels = resources.width as usize * 4;
                    let mut lane = 0usize;
                    'rows: for row in 0..resources.height {
                        let start = (row * readback.padded_bytes_per_row) as usize;
                        let row_bytes = &data[start..start + row_channels * 4];
                        for chunk in row_bytes.chunks_exact(4) {
                            if lane == readback.last.len() {
                                break 'rows;
                            }
                            let v = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                            readback.last[lane] = quantize_unorm16(v);
                            lane += 1;
                        }
                    }
                    drop(data);
                    readback.buffer.unmap();
                    readback.pending = None;
                }
            }
        }

        // Issue: copy the frame just drawn and map it, unless a previous
        // map is still holding the buffer.
        if readback.pending.is_none() {
            let mut encoder = shared
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: &resources.target.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &readback.buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(readback.padded_bytes_per_row),
                        rows_per_image: None,
                    },
                },
                wgpu::Extent3d {
                    width: resources.width,
                    height: resources.height,
                    depth_or_array_layers: 1,
                },
            );
            shared.queue.submit([encoder.finish()]);

            let state = Arc::new(Mutex::new(None));
            let callback_state = Arc::clone(&state);
            readback
                .buffer
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |result| {
                    *callback_state.lock().expect("map state (uncontended)") = Some(result);
                });
            readback.pending = Some(state);
        }

        out.copy_from_slice(&readback.last);
        Ok(())
    }

    /// Ensure a resource set for `count` exists and sits at the back of
    /// `self.resources` (the most-recently-used slot the callers read).
    fn ensure_resources(&mut self, shared: &GpuShared, count: u32) {
        if let Some(pos) = self
            .resources
            .iter()
            .position(|resources| resources.count == count)
        {
            let existing = self.resources.remove(pos);
            self.resources.push(existing);
            return;
        }
        if self.resources.len() >= MAX_RESOURCE_SETS {
            self.resources.remove(0);
        }
        let max_dim = shared.device.limits().max_texture_dimension_2d;
        let width = count.min(max_dim);
        let height = count.div_ceil(width);
        let vertex_buffer = shared.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("lp-gfx-wgpu sample points"),
            size: u64::from(count) * VERTEX_STRIDE,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let target = GpuTexture::new(
            &shared.device,
            width,
            height,
            TextureStorageFormat::Rgba16Unorm,
            "lp-gfx-wgpu sample target",
        );
        #[cfg(target_arch = "wasm32")]
        let readback = {
            let bytes_per_pixel = gpu_channels(TextureStorageFormat::Rgba16Unorm) as u32 * 4;
            let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
            let padded_bytes_per_row = (width * bytes_per_pixel).div_ceil(align) * align;
            AsyncReadback {
                buffer: shared.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("lp-gfx-wgpu sample read_back"),
                    size: u64::from(padded_bytes_per_row) * u64::from(height),
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                }),
                padded_bytes_per_row,
                pending: None,
                last: vec![0; count as usize * 4],
            }
        };
        self.resources.push(SampleResources {
            count,
            width,
            height,
            vertex_buffer,
            target,
            #[cfg(target_arch = "wasm32")]
            readback,
        });
    }
}
