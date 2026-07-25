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
//! # Two exits
//!
//! - [`SamplePass::render_grid`] renders the grid into a caller-provided
//!   target view and **leaves it on the GPU** — wasm-capable, the resident
//!   input to the LED splat op (`LpShader::sample_to_grid`).
//! - [`SamplePass::run`] renders into an internal grid target and reads it
//!   back through the blocking buffer map in [`crate::read_back`], then
//!   quantizes with the CPU packing rule into the caller's RGBA16 buffer.
//!   Native only: the browser tier cannot block on a map (see
//!   `LpShader::sample_rgba16` in [`crate::render`]).

#[cfg(not(target_arch = "wasm32"))]
use lp_gfx::GfxError;
use lps_shared::TextureStorageFormat;

use crate::gpu_graphics::GpuShared;
#[cfg(not(target_arch = "wasm32"))]
use crate::read_back::read_back_f32;
use crate::texture_backing::gpu_format;
#[cfg(not(target_arch = "wasm32"))]
use crate::texture_backing::{GpuTexture, quantize_unorm16};

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

/// Row-major grid dimensions for `count` sample points under a device's
/// `max_texture_dimension_2d`: `W = min(count, max_dim)`,
/// `H = ceil(count / W)`. `count` must be nonzero and at most
/// `max_dim × max_dim` (callers validate and error above this).
pub(crate) fn grid_dims(count: u32, max_dim: u32) -> (u32, u32) {
    let width = count.min(max_dim);
    (width, count.div_ceil(width))
}

/// The compiled sample pipeline plus its per-count resources. Built lazily
/// on the first sampling call (render-only consumers — the gallery — never
/// pay for it); resources are rebuilt only when the point count changes.
pub(crate) struct SamplePass {
    pipeline: wgpu::RenderPipeline,
    /// Point vertex buffer, rebuilt when the point count changes.
    vertices: Option<VertexBuffer>,
    /// Internal grid target for the readback path ([`Self::run`], native
    /// LED output); the resident path renders into caller targets instead.
    #[cfg(not(target_arch = "wasm32"))]
    readback_target: Option<ReadbackTarget>,
}

/// Vertex buffer for one point count.
struct VertexBuffer {
    count: u32,
    buffer: wgpu::Buffer,
}

/// Row-major `W × H` grid target for the readback path.
#[cfg(not(target_arch = "wasm32"))]
struct ReadbackTarget {
    width: u32,
    height: u32,
    target: GpuTexture,
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
            vertices: None,
            #[cfg(not(target_arch = "wasm32"))]
            readback_target: None,
        }
    }

    /// Evaluate the shader at `points_q16` (`count × 2` Q16.16 coordinates)
    /// into the row-major `width × height` grid behind `target_view`,
    /// leaving the result on the GPU (the resident path — wasm-capable).
    /// The caller has already validated `0 < count ≤ width × height`,
    /// written the uniform buffer behind `bind_group`, and passed a view
    /// over a `gpu_format(Rgba16Unorm)` render attachment.
    pub(crate) fn render_grid(
        &mut self,
        shared: &GpuShared,
        points_q16: &[i32],
        bind_group: Option<&wgpu::BindGroup>,
        target_view: &wgpu::TextureView,
        width: u32,
        height: u32,
    ) {
        debug_assert_eq!(points_q16.len() % 2, 0);
        let count = (points_q16.len() / 2) as u32;
        debug_assert!(count > 0);
        debug_assert!(u64::from(count) <= u64::from(width) * u64::from(height));

        self.ensure_vertices(shared, count);
        let vertices = self
            .vertices
            .as_ref()
            .expect("sample vertex buffer was just ensured");

        // Vertex i: clip-space center of grid texel (i % W, i / W), then the
        // Q16.16 point as f32 pixel coordinates (exact for |coord| < 2^24
        // texels). Texel row 0 is the top of the target, which is clip-space
        // +y, so rows map top-down.
        let mut vertex_floats = Vec::with_capacity(points_q16.len() / 2 * 4);
        for (i, point) in points_q16.chunks_exact(2).enumerate() {
            let col = i as u32 % width;
            let row = i as u32 / width;
            let clip_x = (col as f32 + 0.5) / width as f32 * 2.0 - 1.0;
            let clip_y = 1.0 - (row as f32 + 0.5) / height as f32 * 2.0;
            vertex_floats.push(clip_x);
            vertex_floats.push(clip_y);
            vertex_floats.push((f64::from(point[0]) / 65536.0) as f32);
            vertex_floats.push((f64::from(point[1]) / 65536.0) as f32);
        }
        let vertex_bytes: Vec<u8> = vertex_floats.iter().flat_map(|v| v.to_le_bytes()).collect();
        shared
            .queue
            .write_buffer(&vertices.buffer, 0, &vertex_bytes);

        let mut encoder = shared
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("lp-gfx-wgpu sample"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target_view,
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
            pass.set_vertex_buffer(0, vertices.buffer.slice(..));
            pass.draw(0..count, 0..1);
        }
        shared.queue.submit([encoder.finish()]);
    }

    /// Evaluate the shader at `points_q16` (`count × 2` Q16.16 coordinates)
    /// and quantize the results into `out` (`count × 4` RGBA16 channels)
    /// through the blocking readback (native LED output). The caller has
    /// already written the uniform buffer behind `bind_group`.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn run(
        &mut self,
        shared: &GpuShared,
        points_q16: &[i32],
        bind_group: Option<&wgpu::BindGroup>,
        out: &mut [u16],
    ) -> Result<(), GfxError> {
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
        let (width, height) = grid_dims(count, max_dim);

        self.ensure_readback_target(shared, width, height);
        // Split-borrow: render_grid needs `&mut self` for the vertex
        // buffer, so the target view is cloned out first (wgpu views are
        // internally refcounted).
        let target_view = self
            .readback_target
            .as_ref()
            .expect("sample readback target was just ensured")
            .target
            .view
            .clone();
        self.render_grid(shared, points_q16, bind_group, &target_view, width, height);

        // Row-major readback: texel (col, row) is index row * W + col, so
        // channels line up with `out` directly; the final row's unused tail
        // (count not divisible by W) falls off the zip.
        let target = &self
            .readback_target
            .as_ref()
            .expect("sample readback target was just ensured")
            .target;
        let pixels = read_back_f32(
            &shared.device,
            &shared.queue,
            target,
            width,
            height,
            TextureStorageFormat::Rgba16Unorm,
            None,
        )?;
        for (dst, &v) in out.iter_mut().zip(&pixels) {
            *dst = quantize_unorm16(v);
        }
        Ok(())
    }

    fn ensure_vertices(&mut self, shared: &GpuShared, count: u32) {
        if self
            .vertices
            .as_ref()
            .is_none_or(|vertices| vertices.count != count)
        {
            let buffer = shared.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("lp-gfx-wgpu sample points"),
                size: u64::from(count) * VERTEX_STRIDE,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.vertices = Some(VertexBuffer { count, buffer });
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn ensure_readback_target(&mut self, shared: &GpuShared, width: u32, height: u32) {
        if self
            .readback_target
            .as_ref()
            .is_none_or(|readback| (readback.width, readback.height) != (width, height))
        {
            let target = GpuTexture::new(
                &shared.device,
                width,
                height,
                TextureStorageFormat::Rgba16Unorm,
                "lp-gfx-wgpu sample target",
            );
            self.readback_target = Some(ReadbackTarget {
                width,
                height,
                target,
            });
        }
    }
}
