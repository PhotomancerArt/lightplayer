//! [`GpuShader`]: a compiled fragment pipeline implementing
//! `lp_gfx::LpShader` (fullscreen-triangle pass into a render target).

use std::sync::Arc;

use lp_gfx::{GfxError, LpShader, SampleOutHandle, SamplePointsHandle, TextureHandle};
use lps_shared::{LpsValueF32, TextureStorageFormat};

use crate::gpu_graphics::GpuShared;
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
    /// Validated naga module (drives uniform encoding offsets).
    module: naga::Module,
    table: UniformTable,
    pipeline: wgpu::RenderPipeline,
    uniforms: Option<ShaderUniforms>,
}

/// The instance uniform buffer and its per-global slices.
struct ShaderUniforms {
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

        let (bind_group_layout, uniforms) = if table.globals.is_empty() {
            (None, None)
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
            (
                Some(layout),
                Some(ShaderUniforms {
                    buffer,
                    bind_group,
                    offsets,
                }),
            )
        };

        let layouts: Vec<Option<&wgpu::BindGroupLayout>> =
            bind_group_layout.iter().map(Some).collect();
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
            module: compiled.module,
            table,
            pipeline,
            uniforms,
        })
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

        let encoded = encode_uniforms(&self.module, &self.table, uniforms)?;
        if let Some(shader_uniforms) = &self.uniforms {
            for ((_, bytes), &offset) in encoded.iter().zip(&shader_uniforms.offsets) {
                self.shared
                    .queue
                    .write_buffer(&shader_uniforms.buffer, offset, bytes);
            }
        }

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

    fn sample_rgba16(
        &mut self,
        _points: &mut SamplePointsHandle,
        _out: &mut SampleOutHandle,
        _uniforms: &LpsValueF32,
    ) -> Result<(), GfxError> {
        Err(GfxError::Render(String::from(
            "sample_rgba16 is not implemented on the wgpu backend yet \
             (GPU sample-point pass milestone); sampling sinks run on the CPU tier",
        )))
    }
}
