//! GPU sample-point pass: evaluate a compiled shader at caller-provided
//! Q16.16 pixel-space points (the LED-output path — thousands of points per
//! tick, not megapixels).
//!
//! # Shape
//!
//! One **point-list draw into an `N × 1` target**: vertex `i` carries its
//! own clip-space x (precomputed on the CPU as the center of pixel `i`) plus
//! the pixel-space sample position as a second attribute, passed to the
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
//! Native only: results come back through the blocking buffer map in
//! [`crate::read_back`], then quantize with the CPU packing rule into the
//! caller's RGBA16 buffer. The browser tier cannot block on a map and LED
//! output is not a browser product path, so wasm32 keeps an explicit error
//! (see `LpShader::sample_rgba16` in [`crate::render`]).

use lp_gfx::GfxError;
use lps_shared::TextureStorageFormat;

use crate::gpu_graphics::GpuShared;
use crate::read_back::read_back_f32;
use crate::texture_backing::{GpuTexture, gpu_format, quantize_unorm16};

/// Hand-written point-list vertex stage. Attribute 0 is the precomputed
/// clip-space x of target pixel `i`; attribute 1 is the pixel-space sample
/// position forwarded to the fragment stage.
const SAMPLE_VERTEX_WGSL: &str = "
struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) sample_pos: vec2<f32>,
}

@vertex
fn vs_main(@location(0) clip_x: f32, @location(1) point: vec2<f32>) -> VsOut {
    var out: VsOut;
    out.position = vec4<f32>(clip_x, 0.0, 0.0, 1.0);
    out.sample_pos = point;
    return out;
}
";

/// Bytes per sample vertex: `clip_x: f32` + `point: vec2<f32>`.
const VERTEX_STRIDE: u64 = 12;

/// The compiled sample pipeline plus its per-count resources. Built lazily
/// on the first `sample_rgba16` call (render-only consumers — the gallery —
/// never pay for it); resources are rebuilt only when the point count
/// changes.
pub(crate) struct SamplePass {
    pipeline: wgpu::RenderPipeline,
    resources: Option<SampleResources>,
}

/// Vertex buffer and `N × 1` target for one point count.
struct SampleResources {
    count: u32,
    vertex_buffer: wgpu::Buffer,
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
                            format: wgpu::VertexFormat::Float32,
                            offset: 0,
                            shader_location: 0,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 4,
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
            resources: None,
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
        let max_width = shared.device.limits().max_texture_dimension_2d;
        if count > max_width {
            return Err(GfxError::Render(format!(
                "sample_rgba16: {count} points exceed the device's maximum sample-target width \
                 ({max_width})"
            )));
        }

        self.ensure_resources(shared, count);
        let resources = self
            .resources
            .as_ref()
            .expect("sample resources were just ensured");

        // Vertex i: clip-space center of target pixel i, then the Q16.16
        // point as f32 pixel coordinates (exact for |coord| < 2^24 texels).
        let mut vertices = Vec::with_capacity(points_q16.len() / 2 * 3);
        for (i, point) in points_q16.chunks_exact(2).enumerate() {
            let clip_x = (i as f32 + 0.5) / count as f32 * 2.0 - 1.0;
            vertices.push(clip_x);
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

        let pixels = read_back_f32(
            &shared.device,
            &shared.queue,
            &resources.target,
            count,
            1,
            TextureStorageFormat::Rgba16Unorm,
        )?;
        for (dst, &v) in out.iter_mut().zip(&pixels) {
            *dst = quantize_unorm16(v);
        }
        Ok(())
    }

    fn ensure_resources(&mut self, shared: &GpuShared, count: u32) {
        if self
            .resources
            .as_ref()
            .is_none_or(|resources| resources.count != count)
        {
            let vertex_buffer = shared.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("lp-gfx-wgpu sample points"),
                size: u64::from(count) * VERTEX_STRIDE,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let target = GpuTexture::new(
                &shared.device,
                count,
                1,
                TextureStorageFormat::Rgba16Unorm,
                "lp-gfx-wgpu sample target",
            );
            self.resources = Some(SampleResources {
                count,
                vertex_buffer,
                target,
            });
        }
    }
}
