//! GPU implementation of the `splat_leds` texture op (a member of the
//! GPU-resident texture-op family — see `lp-gfx/README.md`).
//!
//! # Shape
//!
//! One **instanced quad per LED** (triangle strip, four vertices — WGSL has
//! no point size, so point primitives cannot cover a splat): the vertex
//! stage projects the instance's world-space center through the caller's
//! view-projection matrix, expands the quad along camera-aligned billboard
//! axes recovered from that matrix, and fetches the instance's color from
//! the grid texture by row-major texel index. The fragment stage shades a
//! soft radial falloff and fixed-function **additive** blending accumulates
//! overlapping splats.
//!
//! # Accumulator + resolve
//!
//! Core WebGPU cannot blend into `Rgba32Float` targets (that needs the
//! optional `float32-blendable` feature, absent from default browser and
//! test devices), so splats accumulate in an internal `Rgba16Float` texture
//! — blendable in core — cleared to the caller's clear color, then a fixed
//! resolve pass copies it into the product target rounded to the unorm16
//! grid (the texture-op family's product convention, like `blend`).
//! `Rgba16Float`'s ~3-decimal-digit precision is ample for a preview splat
//! product that presents through 8-bit sRGB.

use std::sync::Mutex;

use lp_gfx::{GfxError, LedSplatInstance, LedSplatParams};
use lps_shared::TextureStorageFormat;

use crate::gpu_graphics::GpuShared;
use crate::texture_backing::{GpuTexture, gpu_format};

/// Instanced splat pass: quad expansion + grid fetch in the vertex stage,
/// soft radial falloff in the fragment stage (additive blend is
/// fixed-function pipeline state).
const SPLAT_WGSL: &str = "
struct SplatUniforms {
    view_proj: mat4x4<f32>,
    radius_scale: f32,
    color_scale: f32,
}

@group(0) @binding(0) var grid_tex: texture_2d<f32>;
@group(0) @binding(1) var<uniform> splat: SplatUniforms;

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) @interpolate(flat) color: vec4<f32>,
}

@vertex
fn vs_main(
    @builtin(vertex_index) vi: u32,
    @location(0) center: vec3<f32>,
    @location(1) radius: f32,
    @location(2) grid_index: u32,
) -> VsOut {
    var corners = array<vec2<f32>, 4>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(1.0, -1.0),
        vec2<f32>(-1.0, 1.0),
        vec2<f32>(1.0, 1.0),
    );
    let corner = corners[vi];
    // Camera-aligned billboard axes recovered from the view-projection
    // rows: clip x/y are dot(row, world), so the world directions that
    // move clip x/y fastest are the projected right/up — exact for
    // orthographic projections (the planar card fit), the standard
    // approximation for a future perspective camera.
    let right = normalize(vec3<f32>(
        splat.view_proj[0].x, splat.view_proj[1].x, splat.view_proj[2].x));
    let up = normalize(vec3<f32>(
        splat.view_proj[0].y, splat.view_proj[1].y, splat.view_proj[2].y));
    let offset = (corner.x * right + corner.y * up) * radius * splat.radius_scale;
    let dims = textureDimensions(grid_tex);
    let texel = vec2<u32>(grid_index % dims.x, grid_index / dims.x);
    var out: VsOut;
    out.position = splat.view_proj * vec4<f32>(center + offset, 1.0);
    out.local = corner;
    // Display-brightness uniform: scales the fetched RGB, never alpha.
    let fetched = textureLoad(grid_tex, texel, 0);
    out.color = vec4<f32>(fetched.rgb * splat.color_scale, fetched.a);
    return out;
}

// Soft radial falloff: solid core out to half the radius, smoothstep edge
// to the quad rim.
@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let falloff = 1.0 - smoothstep(0.5, 1.0, length(in.local));
    return in.color * falloff;
}
";

/// Fullscreen resolve: accumulated `Rgba16Float` splats → the product
/// target, rounded to the unorm16 grid (values are stored as `v/65536`
/// floats, matching the `blend` op's product convention).
const RESOLVE_WGSL: &str = "
@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    var pos = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    return vec4<f32>(pos[vi], 0.0, 1.0);
}

@group(0) @binding(0) var accum_tex: texture_2d<f32>;

@fragment
fn fs_main(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let v = textureLoad(accum_tex, vec2<i32>(floor(pos.xy)), 0);
    let rounded = clamp(floor(v * 65536.0 + 0.5), vec4<f32>(0.0), vec4<f32>(65535.0));
    return rounded / 65536.0;
}
";

/// Bytes per splat instance: `center: vec3<f32>` + `radius: f32` +
/// `grid_index: u32`.
const INSTANCE_STRIDE: u64 = 20;

/// Byte size of the WGSL `SplatUniforms` struct (`mat4x4<f32>` + 2 × `f32`,
/// rounded up to the struct's 16-byte alignment).
const UNIFORM_SIZE: u64 = 80;

/// The additive accumulator's texture format: blendable in core WebGPU
/// (`Rgba32Float` is not — see the module docs).
const ACCUMULATOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// Splat pipeline pieces cached on the backend (fixed pipelines, built once
/// per device — this is not a shader cache) plus the reusable accumulator
/// texture (rebuilt only when the target size changes).
pub(crate) struct LedSplatPipeline {
    splat_pipeline: wgpu::RenderPipeline,
    splat_layout: wgpu::BindGroupLayout,
    resolve_pipeline: wgpu::RenderPipeline,
    resolve_layout: wgpu::BindGroupLayout,
    accumulator: Mutex<Option<Accumulator>>,
}

/// Cached accumulator target (the view keeps the texture alive).
struct Accumulator {
    width: u32,
    height: u32,
    view: wgpu::TextureView,
}

impl LedSplatPipeline {
    fn new(device: &wgpu::Device) -> Self {
        let splat_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("lp-gfx-wgpu led splat"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(SPLAT_WGSL)),
        });
        // The grid fetch happens in the vertex stage (one fetch per
        // instance corner, flat-interpolated to the fragment stage).
        let splat_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("lp-gfx-wgpu led splat"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(UNIFORM_SIZE),
                    },
                    count: None,
                },
            ],
        });
        let splat_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("lp-gfx-wgpu led splat"),
                bind_group_layouts: &[Some(&splat_layout)],
                immediate_size: 0,
            });
        let splat_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("lp-gfx-wgpu led splat"),
            layout: Some(&splat_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &splat_module,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: INSTANCE_STRIDE,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &[
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x3,
                            offset: 0,
                            shader_location: 0,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32,
                            offset: 12,
                            shader_location: 1,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Uint32,
                            offset: 16,
                            shader_location: 2,
                        },
                    ],
                }],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &splat_module,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: ACCUMULATOR_FORMAT,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        let resolve_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("lp-gfx-wgpu led splat resolve"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(RESOLVE_WGSL)),
        });
        let resolve_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("lp-gfx-wgpu led splat resolve"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let resolve_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("lp-gfx-wgpu led splat resolve"),
                bind_group_layouts: &[Some(&resolve_layout)],
                immediate_size: 0,
            });
        let resolve_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("lp-gfx-wgpu led splat resolve"),
            layout: Some(&resolve_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &resolve_module,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &resolve_module,
                entry_point: Some("fs_main"),
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
            splat_pipeline,
            splat_layout,
            resolve_pipeline,
            resolve_layout,
            accumulator: Mutex::new(None),
        }
    }

    /// The accumulator view for a `width × height` target, reusing the
    /// cached texture when the size matches (a fresh view clone is returned
    /// so the cache lock is never held across encoding).
    fn accumulator_view(
        &self,
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> wgpu::TextureView {
        let mut cached = self
            .accumulator
            .lock()
            .expect("led splat accumulator lock is never poisoned");
        if cached
            .as_ref()
            .is_none_or(|accumulator| (accumulator.width, accumulator.height) != (width, height))
        {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("lp-gfx-wgpu led splat accumulator"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: ACCUMULATOR_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            *cached = Some(Accumulator {
                width,
                height,
                view: texture.create_view(&wgpu::TextureViewDescriptor::default()),
            });
        }
        cached
            .as_ref()
            .expect("led splat accumulator was just ensured")
            .view
            .clone()
    }
}

/// Run the GPU splat: clear the accumulator to `params.clear_color`,
/// additively draw one quad per instance colored from `grid`, and resolve
/// into `target` (`target_width × target_height`, unorm16-grid rounding as
/// documented on the resolve pipeline). The caller has already validated
/// formats and grid indices.
pub(crate) fn splat_leds_gpu(
    shared: &GpuShared,
    grid: &GpuTexture,
    instances: &[LedSplatInstance],
    params: &LedSplatParams,
    target: &GpuTexture,
    target_width: u32,
    target_height: u32,
) -> Result<(), GfxError> {
    let pipeline = shared
        .led_splat_pipeline
        .get_or_init(|| LedSplatPipeline::new(&shared.device));
    let device = &shared.device;

    let mut uniform_bytes = Vec::with_capacity(UNIFORM_SIZE as usize);
    for column in &params.view_proj {
        for &lane in column {
            uniform_bytes.extend_from_slice(&lane.to_le_bytes());
        }
    }
    uniform_bytes.extend_from_slice(&params.radius_scale.to_le_bytes());
    uniform_bytes.extend_from_slice(&params.color_scale.to_le_bytes());
    uniform_bytes.resize(UNIFORM_SIZE as usize, 0);
    let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("lp-gfx-wgpu led splat uniforms"),
        size: UNIFORM_SIZE,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    shared
        .queue
        .write_buffer(&uniform_buffer, 0, &uniform_bytes);

    // Empty layouts still clear + resolve (a splat of zero LEDs is the
    // caller's clear color); the instance buffer is skipped entirely.
    let instance_buffer = if instances.is_empty() {
        None
    } else {
        let mut instance_bytes = Vec::with_capacity(instances.len() * INSTANCE_STRIDE as usize);
        for instance in instances {
            for lane in instance.position {
                instance_bytes.extend_from_slice(&lane.to_le_bytes());
            }
            instance_bytes.extend_from_slice(&instance.radius.to_le_bytes());
            instance_bytes.extend_from_slice(&instance.grid_index.to_le_bytes());
        }
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("lp-gfx-wgpu led splat instances"),
            size: instance_bytes.len() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        shared.queue.write_buffer(&buffer, 0, &instance_bytes);
        Some(buffer)
    };

    let splat_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("lp-gfx-wgpu led splat"),
        layout: &pipeline.splat_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&grid.view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: uniform_buffer.as_entire_binding(),
            },
        ],
    });
    let accumulator_view = pipeline.accumulator_view(device, target_width, target_height);
    let resolve_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("lp-gfx-wgpu led splat resolve"),
        layout: &pipeline.resolve_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(&accumulator_view),
        }],
    });

    let clear_color = wgpu::Color {
        r: f64::from(params.clear_color[0]),
        g: f64::from(params.clear_color[1]),
        b: f64::from(params.clear_color[2]),
        a: f64::from(params.clear_color[3]),
    };
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("lp-gfx-wgpu led splat"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &accumulator_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(clear_color),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        if let Some(instance_buffer) = &instance_buffer {
            pass.set_pipeline(&pipeline.splat_pipeline);
            pass.set_bind_group(0, &splat_bind_group, &[]);
            pass.set_vertex_buffer(0, instance_buffer.slice(..));
            pass.draw(0..4, 0..instances.len() as u32);
        }
    }
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("lp-gfx-wgpu led splat resolve"),
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
        pass.set_pipeline(&pipeline.resolve_pipeline);
        pass.set_bind_group(0, &resolve_bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
    shared.queue.submit([encoder.finish()]);
    Ok(())
}

#[cfg(test)]
mod tests {
    use lp_gfx::led_splat::ortho_fit;
    use lp_gfx::{LpGraphics, ShaderCompileOptions, ShaderSemantics};
    use lps_shared::LpsValueF32;

    use crate::GpuGraphics;
    use crate::test_gpu::test_gpu;

    use super::*;

    /// `Rgba16Float` accumulation carries ~11 mantissa bits; allow ~0.2% of
    /// the unorm16 range on every conformance comparison.
    const TOLERANCE: u16 = 130;

    #[test]
    fn splats_land_at_display_positions_with_grid_colors() {
        let Some(graphics) = test_graphics() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        // 2×1 grid: LED 0 is half red, LED 1 is quarter green.
        let grid = grid_texture(&graphics, &[32768, 0, 0, 65535, 0, 16384, 0, 65535]);
        let mut target = graphics.create_render_target(32, 32).expect("target");
        let instances = [
            LedSplatInstance {
                position: [0.25, 0.25, 0.0],
                radius: 0.125,
                grid_index: 0,
            },
            // Nonzero z: positions are vec3; the planar ortho drops z.
            LedSplatInstance {
                position: [0.75, 0.75, 3.0],
                radius: 0.125,
                grid_index: 1,
            },
        ];
        let params = LedSplatParams {
            view_proj: ortho_fit([0.0, 0.0], [1.0, 1.0]),
            radius_scale: 1.0,
            color_scale: 1.0,
            clear_color: [0.0, 0.0, 0.0, 0.0],
        };
        graphics
            .splat_leds(&grid, &instances, &params, &mut target)
            .expect("splat");
        let channels = read_channels(&graphics, &target);

        // LED 0's center (world 0.25 → pixel 8) carries its full grid color
        // (inside the solid falloff core).
        let led0 = pixel(&channels, 32, 8, 8);
        assert!(led0[0].abs_diff(32768) <= TOLERANCE, "led0 red: {led0:?}");
        assert!(led0[1] <= TOLERANCE, "led0 green: {led0:?}");
        assert!(led0[3].abs_diff(65535) <= TOLERANCE, "led0 alpha: {led0:?}");
        // LED 1's center likewise — z did not disturb the planar position.
        let led1 = pixel(&channels, 32, 24, 24);
        assert!(led1[1].abs_diff(16384) <= TOLERANCE, "led1 green: {led1:?}");
        assert!(led1[0] <= TOLERANCE, "led1 red: {led1:?}");
        // Background between the splats stays at the (black) clear color.
        let background = pixel(&channels, 32, 16, 16);
        assert_eq!(background, [0, 0, 0, 0], "background: {background:?}");
    }

    #[test]
    fn overlapping_splats_accumulate_additively() {
        let Some(graphics) = test_graphics() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let grid = grid_texture(&graphics, &[16384, 0, 0, 65535]);
        let params = LedSplatParams {
            view_proj: ortho_fit([0.0, 0.0], [1.0, 1.0]),
            radius_scale: 1.0,
            color_scale: 1.0,
            clear_color: [0.0, 0.0, 0.0, 0.0],
        };
        let instance = LedSplatInstance {
            position: [0.5, 0.5, 0.0],
            radius: 0.25,
            grid_index: 0,
        };

        let center_red = |instances: &[LedSplatInstance]| -> u16 {
            let mut target = graphics.create_render_target(32, 32).expect("target");
            graphics
                .splat_leds(&grid, instances, &params, &mut target)
                .expect("splat");
            pixel(&read_channels(&graphics, &target), 32, 16, 16)[0]
        };

        let single = center_red(&[instance]);
        let double = center_red(&[instance, instance]);
        assert!(single.abs_diff(16384) <= TOLERANCE, "single: {single}");
        assert!(
            double.abs_diff(32768) <= TOLERANCE,
            "coincident splats sum: {double}"
        );
    }

    #[test]
    fn the_clear_color_is_the_callers() {
        let Some(graphics) = test_graphics() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let grid = grid_texture(&graphics, &[0, 0, 0, 0]);
        let mut target = graphics.create_render_target(8, 8).expect("target");
        let params = LedSplatParams {
            view_proj: ortho_fit([0.0, 0.0], [1.0, 1.0]),
            radius_scale: 1.0,
            color_scale: 1.0,
            clear_color: [0.1, 0.2, 0.3, 0.4],
        };
        graphics
            .splat_leds(&grid, &[], &params, &mut target)
            .expect("splat");
        let channels = read_channels(&graphics, &target);
        let expected = [6554u16, 13107, 19661, 26214];
        for (lane, (&got, &want)) in pixel(&channels, 8, 3, 5).iter().zip(&expected).enumerate() {
            assert!(
                got.abs_diff(want) <= TOLERANCE,
                "lane {lane}: got {got}, want {want}"
            );
        }
    }

    #[test]
    fn radius_scale_multiplies_every_radius() {
        let Some(graphics) = test_graphics() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let grid = grid_texture(&graphics, &[65535, 65535, 65535, 65535]);
        let instance = LedSplatInstance {
            position: [0.5, 0.5, 0.0],
            radius: 0.0625,
            grid_index: 0,
        };
        // Probe pixel (20, 16): ~0.14 world units from the splat center —
        // outside the raw radius, inside the ×4 scaled one.
        let probe_red = |radius_scale: f32| -> u16 {
            let mut target = graphics.create_render_target(32, 32).expect("target");
            let params = LedSplatParams {
                view_proj: ortho_fit([0.0, 0.0], [1.0, 1.0]),
                radius_scale,
                color_scale: 1.0,
                clear_color: [0.0, 0.0, 0.0, 0.0],
            };
            graphics
                .splat_leds(&grid, &[instance], &params, &mut target)
                .expect("splat");
            pixel(&read_channels(&graphics, &target), 32, 20, 16)[0]
        };
        assert!(probe_red(1.0) <= TOLERANCE, "unscaled probe must be dark");
        assert!(probe_red(4.0) > 30000, "scaled probe must be lit");
    }

    #[test]
    fn out_of_bounds_grid_indices_are_an_error() {
        let Some(graphics) = test_graphics() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let grid = grid_texture(&graphics, &[0, 0, 0, 0]);
        let mut target = graphics.create_render_target(4, 4).expect("target");
        let instances = [LedSplatInstance {
            position: [0.5, 0.5, 0.0],
            radius: 0.5,
            grid_index: 1,
        }];
        let params = LedSplatParams {
            view_proj: ortho_fit([0.0, 0.0], [1.0, 1.0]),
            radius_scale: 1.0,
            color_scale: 1.0,
            clear_color: [0.0; 4],
        };
        match graphics.splat_leds(&grid, &instances, &params, &mut target) {
            Err(GfxError::Backend(message)) => {
                assert!(message.contains("grid_index"), "{message}");
            }
            other => panic!("expected out-of-bounds error, got {other:?}"),
        }
    }

    #[test]
    fn a_resident_grid_feeds_the_splat_without_readback() {
        let Some(graphics) = test_graphics() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        // Sample a constant-color shader into a resident 1×1 grid...
        let options =
            ShaderCompileOptions::new(ShaderSemantics::F32Gpu, lp_shader::ShaderFrontend::Naga);
        let mut shader = graphics
            .compile_shader(
                "vec4 render(vec2 pos) { return vec4(0.25, 0.5, 0.75, 1.0); }",
                &options,
            )
            .expect("compiles");
        let (width, height) = graphics.sample_grid_dims(1).expect("dims");
        let mut grid = graphics
            .create_render_target(width, height)
            .expect("grid target");
        let mut points = graphics.create_sample_points(1).expect("points");
        let uniforms = LpsValueF32::Struct {
            name: None,
            fields: vec![],
        };
        shader
            .sample_to_grid(&mut points, &mut grid, &uniforms)
            .expect("sample to grid");

        // ...then splat it: the sampled color arrives at the display
        // position with no CPU round trip in between.
        let mut target = graphics.create_render_target(16, 16).expect("target");
        let instances = [LedSplatInstance {
            position: [0.5, 0.5, 0.0],
            radius: 0.5,
            grid_index: 0,
        }];
        let params = LedSplatParams {
            view_proj: ortho_fit([0.0, 0.0], [1.0, 1.0]),
            radius_scale: 1.0,
            color_scale: 1.0,
            clear_color: [0.0; 4],
        };
        graphics
            .splat_leds(&grid, &instances, &params, &mut target)
            .expect("splat");
        let center = pixel(&read_channels(&graphics, &target), 16, 8, 8);
        for (lane, (&got, want)) in center
            .iter()
            .zip([16384u16, 32768, 49152, 65535])
            .enumerate()
        {
            assert!(
                got.abs_diff(want) <= TOLERANCE,
                "lane {lane}: got {got}, want {want}"
            );
        }
    }

    #[test]
    fn color_scale_multiplies_rgb_but_not_alpha() {
        let Some(graphics) = test_graphics() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let grid = grid_texture(&graphics, &[40000, 20000, 10000, 60000]);
        let mut target = graphics.create_render_target(8, 8).expect("target");
        let instances = [LedSplatInstance {
            position: [0.5, 0.5, 0.0],
            radius: 0.5,
            grid_index: 0,
        }];
        let params = LedSplatParams {
            view_proj: ortho_fit([0.0, 0.0], [1.0, 1.0]),
            radius_scale: 1.0,
            color_scale: 0.5,
            clear_color: [0.0; 4],
        };
        graphics
            .splat_leds(&grid, &instances, &params, &mut target)
            .expect("splat");
        let center = pixel(&read_channels(&graphics, &target), 8, 4, 4);
        for (lane, (&got, want)) in center
            .iter()
            .zip([20000u16, 10000, 5000, 60000])
            .enumerate()
        {
            assert!(
                got.abs_diff(want) <= TOLERANCE,
                "lane {lane}: got {got}, want {want}"
            );
        }
    }

    /// D2 tier parity: the CPU reference rasterizer
    /// (`lp_gfx::rasterize_led_splats`) and the GPU op must produce the same
    /// image for the same layout — overlaps, falloff edges, clear color,
    /// radius and color scaling included — within the f16-accumulation
    /// tolerance.
    #[test]
    fn cpu_rasterizer_matches_the_gpu_splat() {
        let Some(graphics) = test_graphics() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let grid_channels: [u16; 12] = [
            50000, 10000, 5000, 65535, // warm red-ish
            2000, 45000, 30000, 65535, // teal
            20000, 20000, 60000, 40000, // blue, partial alpha
        ];
        let grid = grid_texture(&graphics, &grid_channels);
        let instances = [
            LedSplatInstance {
                position: [0.3, 0.35, 0.0],
                radius: 0.2,
                grid_index: 0,
            },
            LedSplatInstance {
                position: [0.45, 0.5, 0.0],
                radius: 0.25,
                grid_index: 1,
            },
            LedSplatInstance {
                position: [0.7, 0.6, 1.5],
                radius: 0.15,
                grid_index: 2,
            },
        ];
        let world_min = [0.0, 0.0];
        let world_max = [1.0, 1.0];
        let radius_scale = 1.25;
        let color_scale = 0.75;
        let clear_color = [0.02, 0.03, 0.04, 0.0];
        let (width, height) = (48u32, 48u32);

        let mut target = graphics
            .create_render_target(width, height)
            .expect("target");
        let params = LedSplatParams {
            view_proj: ortho_fit(world_min, world_max),
            radius_scale,
            color_scale,
            clear_color,
        };
        graphics
            .splat_leds(&grid, &instances, &params, &mut target)
            .expect("splat");
        let gpu = read_channels(&graphics, &target);

        let cpu = lp_gfx::rasterize_led_splats(
            &grid_channels,
            &instances,
            &lp_gfx::LedSplatRasterParams {
                world_min,
                world_max,
                radius_scale,
                color_scale,
                clear_color,
            },
            width,
            height,
        )
        .expect("cpu raster");

        assert_eq!(gpu.len(), cpu.len());
        for (index, (&g, &c)) in gpu.iter().zip(&cpu).enumerate() {
            let diff = g.abs_diff(c);
            assert!(
                diff <= TOLERANCE,
                "channel {index}: gpu {g} vs cpu {c} (diff {diff})"
            );
        }
        // The comparison must be exercising real content, not two blank
        // images: the CPU image must contain lit pixels above the clear.
        assert!(cpu.iter().any(|&v| v > 20000), "cpu raster is blank");
    }

    fn test_graphics() -> Option<GpuGraphics> {
        let (device, queue) = test_gpu()?;
        Some(GpuGraphics::new(
            device,
            queue,
            Box::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
                lp_shader::ShaderFrontend::Naga,
            )),
        ))
    }

    /// Upload RGBA16 channels as an N×1 grid texture.
    fn grid_texture(graphics: &GpuGraphics, channels: &[u16]) -> lp_gfx::TextureHandle {
        let bytes: Vec<u8> = channels.iter().flat_map(|v| v.to_le_bytes()).collect();
        graphics
            .create_texture(
                (channels.len() / 4) as u32,
                1,
                TextureStorageFormat::Rgba16Unorm,
                &bytes,
            )
            .expect("grid texture")
    }

    fn read_channels(graphics: &GpuGraphics, target: &lp_gfx::TextureHandle) -> Vec<u16> {
        graphics
            .read_back(target)
            .expect("read back")
            .bytes()
            .chunks_exact(2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .collect()
    }

    fn pixel(channels: &[u16], width: u32, x: u32, y: u32) -> [u16; 4] {
        let base = ((y * width + x) * 4) as usize;
        channels[base..base + 4].try_into().expect("pixel channels")
    }
}
