//! GPU implementation of the `blend_textures` texture op (the first member
//! of the GPU-resident texture-op family — see `lp-gfx/README.md`).
//!
//! A small fixed pipeline: two texture bindings plus an alpha uniform,
//! `mix` in the fragment stage, output rounded to the unorm16 grid so the
//! result agrees with the CPU tier's integer blend
//! (`round(prev·(1−α) + active·α)` on u16 lanes) to ≤1 LSB.

use lp_gfx::GfxError;

use crate::gpu_graphics::GpuShared;
use crate::texture_backing::GpuTexture;

/// Fullscreen blend: `out = mix(previous, active, alpha)` rounded to the
/// unorm16 grid (values are stored as `v/65536` floats).
const BLEND_WGSL: &str = "
@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    var pos = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    return vec4<f32>(pos[vi], 0.0, 1.0);
}

@group(0) @binding(0) var previous_tex: texture_2d<f32>;
@group(0) @binding(1) var active_tex: texture_2d<f32>;
@group(0) @binding(2) var<uniform> blend_alpha: f32;

@fragment
fn fs_main(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let coord = vec2<i32>(floor(pos.xy));
    let p = textureLoad(previous_tex, coord, 0);
    let a = textureLoad(active_tex, coord, 0);
    // Scale to u16 lanes, blend with the CPU tier's +0.5 rounding, and
    // saturate — floor-quantization at read_back then reproduces the CPU
    // result exactly for on-grid inputs.
    let mixed = mix(p, a, blend_alpha) * 65536.0;
    let rounded = clamp(floor(mixed + 0.5), vec4<f32>(0.0), vec4<f32>(65535.0));
    return rounded / 65536.0;
}
";

/// Blend pipeline pieces cached on the backend (fixed pipeline, built once
/// per device — this is not a shader cache).
pub(crate) struct BlendPipeline {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
}

impl BlendPipeline {
    fn new(device: &wgpu::Device) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("lp-gfx-wgpu blend"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(BLEND_WGSL)),
        });
        let texture_entry = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("lp-gfx-wgpu blend"),
            entries: &[
                texture_entry(0),
                texture_entry(1),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(4),
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("lp-gfx-wgpu blend"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("lp-gfx-wgpu blend"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba32Float,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        Self {
            pipeline,
            bind_group_layout,
        }
    }
}

/// Run the GPU blend: `target = mix(previous, active, alpha)` (unorm16
/// rounding as documented on the pipeline). All three textures must already
/// be validated as same-size RGBA backings by the caller.
pub(crate) fn blend_textures_gpu(
    shared: &GpuShared,
    previous: &GpuTexture,
    active: &GpuTexture,
    alpha: f32,
    target: &GpuTexture,
) -> Result<(), GfxError> {
    let blend = shared
        .blend_pipeline
        .get_or_init(|| BlendPipeline::new(&shared.device));

    // Match the CPU tier's clamp before blending.
    let alpha = alpha.clamp(0.0, 1.0);
    let alpha_buffer = shared.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("lp-gfx-wgpu blend alpha"),
        size: 4,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    shared
        .queue
        .write_buffer(&alpha_buffer, 0, &alpha.to_le_bytes());

    let bind_group = shared.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("lp-gfx-wgpu blend"),
        layout: &blend.bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&previous.view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&active.view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: alpha_buffer.as_entire_binding(),
            },
        ],
    });

    let mut encoder = shared
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("lp-gfx-wgpu blend"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target.view,
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
        pass.set_pipeline(&blend.pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
    shared.queue.submit([encoder.finish()]);
    Ok(())
}
