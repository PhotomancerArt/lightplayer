//! [`GpuShader`]: a compiled fragment pipeline implementing
//! `lp_gfx::LpShader` (fullscreen-triangle pass into a render target).

use std::sync::Arc;

use lp_gfx::{GfxError, LpShader, SampleOutHandle, SamplePointsHandle, TextureHandle};
use lps_shared::{LpsValueF32, TextureStorageFormat};

use crate::gpu_graphics::GpuShared;
#[cfg(not(target_arch = "wasm32"))]
use crate::sample_pass::SamplePass;
use crate::texture_backing::{gpu_format, gpu_texture_mut};
use crate::uniform_layout::{UniformTable, reflect_uniforms};
use crate::uniform_writer::encode_uniforms;
use crate::wgsl_compile::compile_wgsl;

/// Hand-written fullscreen-triangle vertex stage (the fragment stage comes
/// from the authored GLSL via naga wgsl-out).
const FULLSCREEN_TRIANGLE_WGSL: &str = "
@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    var pos = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    return vec4<f32>(pos[vi], 0.0, 1.0);
}
";

/// A compiled visual shader on the wgpu backend.
///
/// Owns one render pipeline plus **one uniform buffer per shader instance**:
/// each reflected uniform global gets an alignment-padded slice of the
/// buffer, rewritten every frame from the engine's `LpsValueF32` tree.
pub struct GpuShader {
    shared: Arc<GpuShared>,
    /// Authored GLSL, kept for the lazily-built sample pipeline (the sample
    /// unit re-assembles from source with a different wrapper `main`).
    #[cfg(not(target_arch = "wasm32"))]
    authored: String,
    /// Validated naga module (drives uniform encoding offsets).
    module: naga::Module,
    table: UniformTable,
    pipeline: wgpu::RenderPipeline,
    uniforms: Option<ShaderUniforms>,
    /// Sample-point pass (native LED-output path), built on the first
    /// `sample_rgba16` call — render-only consumers never pay for it.
    #[cfg(not(target_arch = "wasm32"))]
    sample_pass: Option<SamplePass>,
}

/// The instance uniform buffer and its per-global slices.
struct ShaderUniforms {
    /// Bind group layout shared by the render and sample pipelines.
    layout: wgpu::BindGroupLayout,
    buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    /// Byte offset of each `table.globals` entry inside `buffer`.
    offsets: Vec<u64>,
}

impl GpuShader {
    /// Compile authored GLSL into a render pipeline on `shared`'s device.
    pub(crate) fn new(shared: Arc<GpuShared>, authored: &str) -> Result<Self, GfxError> {
        let compiled = compile_wgsl(authored)?;
        let table = reflect_uniforms(&compiled.module)?;
        let device = &shared.device;

        let fragment_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("lp-gfx-wgpu fragment (authored GLSL via naga wgsl-out)"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(&compiled.wgsl)),
        });
        let vertex_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("lp-gfx-wgpu fullscreen triangle"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(FULLSCREEN_TRIANGLE_WGSL)),
        });

        let uniforms = if table.globals.is_empty() {
            None
        } else {
            let entries: Vec<wgpu::BindGroupLayoutEntry> = table
                .globals
                .iter()
                .map(|global| wgpu::BindGroupLayoutEntry {
                    binding: global.binding,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(u64::from(global.size)),
                    },
                    count: None,
                })
                .collect();
            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("lp-gfx-wgpu uniforms"),
                entries: &entries,
            });

            // One buffer, one alignment-padded slice per uniform global.
            let alignment = u64::from(device.limits().min_uniform_buffer_offset_alignment.max(4));
            let mut offsets = Vec::with_capacity(table.globals.len());
            let mut cursor = 0u64;
            for global in &table.globals {
                offsets.push(cursor);
                cursor += u64::from(global.size).div_ceil(alignment) * alignment;
            }
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("lp-gfx-wgpu uniform buffer"),
                size: cursor.max(alignment),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let bind_entries: Vec<wgpu::BindGroupEntry> = table
                .globals
                .iter()
                .zip(&offsets)
                .map(|(global, &offset)| wgpu::BindGroupEntry {
                    binding: global.binding,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &buffer,
                        offset,
                        size: wgpu::BufferSize::new(u64::from(global.size)),
                    }),
                })
                .collect();
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("lp-gfx-wgpu uniforms"),
                layout: &layout,
                entries: &bind_entries,
            });
            Some(ShaderUniforms {
                layout,
                buffer,
                bind_group,
                offsets,
            })
        };

        let layouts: Vec<Option<&wgpu::BindGroupLayout>> = uniforms
            .as_ref()
            .map(|shader_uniforms| Some(&shader_uniforms.layout))
            .into_iter()
            .collect();
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("lp-gfx-wgpu shader"),
            bind_group_layouts: &layouts,
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("lp-gfx-wgpu shader"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &vertex_module,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
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

        Ok(Self {
            shared,
            #[cfg(not(target_arch = "wasm32"))]
            authored: String::from(authored),
            module: compiled.module,
            table,
            pipeline,
            uniforms,
            #[cfg(not(target_arch = "wasm32"))]
            sample_pass: None,
        })
    }

    /// Encode the engine uniform tree and write it into the instance uniform
    /// buffer (shared by the render and sample passes).
    fn write_uniforms(&self, uniforms: &LpsValueF32) -> Result<(), GfxError> {
        let encoded = encode_uniforms(&self.module, &self.table, uniforms)?;
        if let Some(shader_uniforms) = &self.uniforms {
            for ((_, bytes), &offset) in encoded.iter().zip(&shader_uniforms.offsets) {
                self.shared
                    .queue
                    .write_buffer(&shader_uniforms.buffer, offset, bytes);
            }
        }
        Ok(())
    }

    /// Build the sample-point pipeline on first use: translate the sample
    /// wrapper unit, check its uniform interface matches the render unit's
    /// (same authored declarations — a mismatch means the assembler broke),
    /// and build the point-list pipeline over the shared uniform layout.
    #[cfg(not(target_arch = "wasm32"))]
    fn ensure_sample_pass(&mut self) -> Result<&mut SamplePass, GfxError> {
        if self.sample_pass.is_none() {
            let compiled = crate::wgsl_compile::compile_sample_wgsl(&self.authored)?;
            let sample_table = reflect_uniforms(&compiled.module)?;
            let interface = |table: &UniformTable| -> Vec<(String, u32, u32)> {
                table
                    .globals
                    .iter()
                    .map(|global| (global.name.clone(), global.binding, global.size))
                    .collect()
            };
            if interface(&sample_table) != interface(&self.table) {
                return Err(GfxError::Compile(format!(
                    "sample unit uniform interface {:?} does not match the render unit's {:?}",
                    interface(&sample_table),
                    interface(&self.table)
                )));
            }
            self.sample_pass = Some(SamplePass::new(
                &self.shared,
                &compiled.wgsl,
                self.uniforms
                    .as_ref()
                    .map(|shader_uniforms| &shader_uniforms.layout),
            ));
        }
        Ok(self
            .sample_pass
            .as_mut()
            .expect("sample pass was just ensured"))
    }
}

impl LpShader for GpuShader {
    fn render(
        &mut self,
        target: &mut TextureHandle,
        uniforms: &LpsValueF32,
    ) -> Result<(), GfxError> {
        if target.format() != TextureStorageFormat::Rgba16Unorm {
            return Err(GfxError::Render(format!(
                "GPU shader renders RGBA16 targets; got {:?}",
                target.format()
            )));
        }
        let backing = gpu_texture_mut(target)?;

        self.write_uniforms(uniforms)?;

        let mut encoder = self
            .shared
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("lp-gfx-wgpu shader render"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &backing.view,
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
            if let Some(shader_uniforms) = &self.uniforms {
                pass.set_bind_group(0, &shader_uniforms.bind_group, &[]);
            }
            pass.draw(0..3, 0..1);
        }
        self.shared.queue.submit([encoder.finish()]);
        Ok(())
    }

    /// Evaluate the shader at the caller's Q16.16 points via the point-list
    /// sample pass (see [`crate::sample_pass`]) and quantize into `out` with
    /// the CPU packing rule. Native only — the LED-output path of the
    /// non-embedded lp-server.
    #[cfg(not(target_arch = "wasm32"))]
    fn sample_rgba16(
        &mut self,
        points: &mut SamplePointsHandle,
        out: &mut SampleOutHandle,
        uniforms: &LpsValueF32,
    ) -> Result<(), GfxError> {
        use crate::sample_backing::{sample_out_mut, sample_points};

        if points.count() != out.count() {
            return Err(GfxError::Render(format!(
                "sample_rgba16: point count {} does not match output count {}",
                points.count(),
                out.count()
            )));
        }
        self.ensure_sample_pass()?;
        self.write_uniforms(uniforms)?;

        let point_coords = &sample_points(points)?.0;
        let out_channels = &mut sample_out_mut(out)?.0;
        let Self {
            shared,
            uniforms: shader_uniforms,
            sample_pass,
            ..
        } = self;
        sample_pass
            .as_mut()
            .expect("sample pass was ensured above")
            .run(
                shared,
                point_coords,
                shader_uniforms
                    .as_ref()
                    .map(|shader_uniforms| &shader_uniforms.bind_group),
                out_channels,
            )
    }

    /// Browser GPU tier: LED output is not a browser product path and the
    /// blocking readback the sample pass needs is unavailable on wasm32
    /// (fidelity-tiers ADR) — explicit error, never a silent CPU substitute.
    #[cfg(target_arch = "wasm32")]
    fn sample_rgba16(
        &mut self,
        _points: &mut SamplePointsHandle,
        _out: &mut SampleOutHandle,
        _uniforms: &LpsValueF32,
    ) -> Result<(), GfxError> {
        Err(GfxError::Render(String::from(
            "sample_rgba16 is unavailable on the browser GPU tier (blocking readback is \
             native-only; LED-output sampling runs on native servers or the CPU tier)",
        )))
    }
}
