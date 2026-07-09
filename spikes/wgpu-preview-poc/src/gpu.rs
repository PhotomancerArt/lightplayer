//! Offscreen wgpu rendering: fullscreen triangle, fragment shader from WGSL,
//! rgba32float target, buffer readback.

use std::time::{Duration, Instant};

use crate::glsl_to_wgsl::{UniformSlot, UniformValue, wgsl_source};

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

/// wgpu pipeline creation timings for one shader.
#[derive(Debug, Clone, Copy)]
pub struct GpuTimings {
    pub create_shader_module: Duration,
    pub create_pipeline: Duration,
}

/// A wgpu device/queue pair for headless rendering.
pub struct GpuFrameRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pub adapter_info: wgpu::AdapterInfo,
}

/// One compiled fragment pipeline plus its uniform bind group layout.
pub struct GpuPipeline {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: Option<wgpu::BindGroupLayout>,
    pub timings: GpuTimings,
}

impl GpuFrameRenderer {
    /// Create a renderer on the first available adapter, or `None` when the
    /// host has no GPU adapter (e.g. CI) — callers skip gracefully.
    pub fn new() -> Option<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
        }))
        .ok()?;
        let adapter_info = adapter.get_info();
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("wgpu-preview-poc"),
            ..Default::default()
        }))
        .ok()?;
        Some(Self {
            device,
            queue,
            adapter_info,
        })
    }

    /// Build the render pipeline for a translated fragment shader, capturing
    /// module/pipeline creation timings.
    pub fn create_pipeline(&self, wgsl: &str, uniforms: &[UniformSlot]) -> GpuPipeline {
        let module_start = Instant::now();
        let fragment_module = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("fragment (authored GLSL via naga wgsl-out)"),
                source: wgsl_source(wgsl),
            });
        let create_shader_module = module_start.elapsed();

        let vertex_module = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("fullscreen triangle"),
                source: wgsl_source(FULLSCREEN_TRIANGLE_WGSL),
            });

        let bind_group_layout = if uniforms.is_empty() {
            None
        } else {
            let entries: Vec<wgpu::BindGroupLayoutEntry> = uniforms
                .iter()
                .map(|slot| wgpu::BindGroupLayoutEntry {
                    binding: slot.binding,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                })
                .collect();
            Some(
                self.device
                    .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                        label: Some("uniforms"),
                        entries: &entries,
                    }),
            )
        };

        let pipeline_start = Instant::now();
        let layouts: Vec<Option<&wgpu::BindGroupLayout>> =
            bind_group_layout.iter().map(Some).collect();
        let pipeline_layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("wgpu-preview-poc"),
                bind_group_layouts: &layouts,
                immediate_size: 0,
            });
        let pipeline = self
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("wgpu-preview-poc"),
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
                        format: wgpu::TextureFormat::Rgba32Float,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            });
        let create_pipeline = pipeline_start.elapsed();

        GpuPipeline {
            pipeline,
            bind_group_layout,
            timings: GpuTimings {
                create_shader_module,
                create_pipeline,
            },
        }
    }

    /// Render one frame and read it back as tightly packed rgba f32 pixels
    /// (row-major, `width * height * 4` floats).
    pub fn render(
        &self,
        pipeline: &GpuPipeline,
        uniforms: &[(UniformSlot, UniformValue)],
        width: u32,
        height: u32,
    ) -> Vec<f32> {
        let bind_group = pipeline.bind_group_layout.as_ref().map(|layout| {
            let buffers: Vec<wgpu::Buffer> = uniforms
                .iter()
                .map(|(_, value)| {
                    let bytes = value.bytes();
                    let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                        label: None,
                        size: bytes.len() as u64,
                        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    });
                    self.queue.write_buffer(&buffer, 0, &bytes);
                    buffer
                })
                .collect();
            let entries: Vec<wgpu::BindGroupEntry> = uniforms
                .iter()
                .zip(&buffers)
                .map(|((slot, _), buffer)| wgpu::BindGroupEntry {
                    binding: slot.binding,
                    resource: buffer.as_entire_binding(),
                })
                .collect();
            self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("uniforms"),
                layout,
                entries: &entries,
            })
        });

        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let bytes_per_pixel = 16u32; // rgba32float
        let unpadded_bytes_per_row = width * bytes_per_pixel;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded_bytes_per_row = unpadded_bytes_per_row.div_ceil(align) * align;
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: u64::from(padded_bytes_per_row) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("wgpu-preview-poc"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
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
            pass.set_pipeline(&pipeline.pipeline);
            if let Some(bind_group) = &bind_group {
                pass.set_bind_group(0, bind_group, &[]);
            }
            pass.draw(0..3, 0..1);
        }
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
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
        self.queue.submit([encoder.finish()]);

        let slice = readback.slice(..);
        slice.map_async(wgpu::MapMode::Read, |result| {
            result.expect("map readback buffer");
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("device poll");

        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
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
        readback.unmap();
        pixels
    }
}
