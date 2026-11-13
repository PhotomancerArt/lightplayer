//! GPU presentation of a render product to a surface (the second member of
//! the GPU-resident texture-op family — see `lp-gfx/README.md`).
//!
//! A small fixed pipeline: one `textureLoad` per output pixel that quantizes
//! the float product texel with the CPU tier's exact packing rule
//! (`floor(v · 65536)` saturated to `[0, 65535]`), then sRGB-encodes it the
//! same way the CPU tier's `Rgba16Unorm` → sRGB8 preview conversion does.
//! Zero readback: the product texture goes straight to the swapchain image.
//!
//! The pipeline targets a **non-sRGB** surface format and performs the sRGB
//! encode in the shader, so GPU-tier presentation matches the CPU tier's
//! byte-path conversion (`fw-browser` `texture_convert`, engine probe edge)
//! rather than the hardware encoder. [`present_to_view`] rejects sRGB target
//! formats to keep that single-encode invariant explicit.

use lp_gfx::GfxError;

use crate::gpu_graphics::GpuShared;
use crate::texture_backing::GpuTexture;

/// Fullscreen present: product float texel → unorm16 grid → sRGB.
///
/// The product texture stores unorm16 lanes as `v / 65536` floats; quantize
/// with the CPU packing rule, normalize on the u16 grid, and sRGB-encode.
/// Non-finite lanes are indeterminate under WGSL `clamp`/`floor` (the CPU
/// rule maps NaN to 0); presentation does not promise NaN parity.
const SURFACE_BLIT_WGSL: &str = "
@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    var pos = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    return vec4<f32>(pos[vi], 0.0, 1.0);
}

@group(0) @binding(0) var product_tex: texture_2d<f32>;

fn srgb_encode(c: f32) -> f32 {
    if c <= 0.0031308 {
        return c * 12.92;
    }
    return 1.055 * pow(c, 1.0 / 2.4) - 0.055;
}

@fragment
fn fs_main(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    // Identity pixel map (cards render the product at the surface size);
    // clamp to the product extent so a transient size mismatch reads edge
    // texels instead of trapping.
    let size = textureDimensions(product_tex, 0);
    let out_coord = vec2<u32>(floor(pos.xy));
    let coord = vec2<i32>(min(out_coord, size - vec2<u32>(1u, 1u)));
    let v = textureLoad(product_tex, coord, 0);
    // CPU packing rule: floor(v * 65536) saturated to the u16 grid, then
    // normalized as u16 / 65535 for the transfer curve.
    let q = clamp(floor(v * 65536.0), vec4<f32>(0.0), vec4<f32>(65535.0));
    let linear = q / 65535.0;
    return vec4<f32>(
        srgb_encode(linear.r),
        srgb_encode(linear.g),
        srgb_encode(linear.b),
        1.0,
    );
}
";

/// Present pipeline pieces cached on the backend (fixed pipeline, built once
/// per device and target format — this is not a shader cache).
pub(crate) struct SurfaceBlitPipeline {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    target_format: wgpu::TextureFormat,
}

impl SurfaceBlitPipeline {
    fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("lp-gfx-wgpu surface blit"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(SURFACE_BLIT_WGSL)),
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("lp-gfx-wgpu surface blit"),
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
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("lp-gfx-wgpu surface blit"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("lp-gfx-wgpu surface blit"),
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
                    format: target_format,
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
            target_format,
        }
    }
}

/// Blit a product texture into a presentable target view.
///
/// `target_format` must be a non-sRGB color format (the shader performs the
/// sRGB encode; an sRGB target would double-encode) and must stay the same
/// for the lifetime of the backend (the fixed pipeline is built once per
/// device — one surface format per worker device, which is how browsers
/// behave in practice).
pub(crate) fn present_to_view(
    shared: &GpuShared,
    source: &GpuTexture,
    target_view: &wgpu::TextureView,
    target_format: wgpu::TextureFormat,
) -> Result<(), GfxError> {
    if target_format.is_srgb() {
        return Err(GfxError::Backend(format!(
            "present_to_view: target format {target_format:?} is sRGB; configure the surface \
             with a non-sRGB format (the blit shader performs the sRGB encode)"
        )));
    }
    let blit = shared
        .surface_blit_pipeline
        .get_or_init(|| SurfaceBlitPipeline::new(&shared.device, target_format));
    if blit.target_format != target_format {
        return Err(GfxError::Backend(format!(
            "present_to_view: surface format changed from {:?} to {target_format:?}; the \
             present pipeline is fixed per device",
            blit.target_format
        )));
    }

    let bind_group = shared.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("lp-gfx-wgpu surface blit"),
        layout: &blit.bind_group_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(&source.view),
        }],
    });

    let mut encoder = shared
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("lp-gfx-wgpu surface blit"),
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
        pass.set_pipeline(&blit.pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
    shared.queue.submit([encoder.finish()]);
    Ok(())
}

#[cfg(test)]
mod tests {
    use lp_gfx::LpGraphics;
    use lps_shared::TextureStorageFormat;

    use super::*;
    use crate::GpuGraphics;
    use crate::test_gpu::test_gpu;
    use crate::texture_backing::gpu_texture;

    #[test]
    fn blit_matches_the_cpu_srgb_conversion() {
        let Some((device, queue)) = test_gpu() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let graphics = GpuGraphics::new(
            device.clone(),
            queue.clone(),
            Box::new(lp_gfx_lpvm::TargetLpvmGraphics::new()),
        );

        // 2×1 RGBA16 product with values across the transfer curve.
        let channels: [u16; 8] = [0, 65535, 32768, 65535, 205, 16384, 60000, 1234];
        let texels: Vec<u8> = channels.iter().flat_map(|v| v.to_le_bytes()).collect();
        let product = graphics
            .create_texture(2, 1, TextureStorageFormat::Rgba16Unorm, &texels)
            .expect("product texture");

        // Presentable stand-in for a surface frame (same usage bits).
        let target_format = wgpu::TextureFormat::Bgra8Unorm;
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("blit test target"),
            size: wgpu::Extent3d {
                width: 2,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: target_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        present_to_view(
            graphics.shared_for_tests(),
            gpu_texture(&product).expect("gpu backing"),
            &view,
            target_format,
        )
        .expect("blit");

        // Read the 8 output bytes back (256-byte row alignment).
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("blit test read"),
            size: 256,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        encoder.copy_texture_to_buffer(
            target.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(256),
                    rows_per_image: None,
                },
            },
            wgpu::Extent3d {
                width: 2,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        queue.submit([encoder.finish()]);
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, |result| result.expect("map"));
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("poll");
        let bytes: Vec<u8> = buffer.slice(..).get_mapped_range()[..8].to_vec();

        // CPU-tier reference conversion (fw-browser texture_convert /
        // engine probe curve), BGRA channel order.
        let srgb8 = |v: u16| -> u8 {
            let linear = f32::from(v) / 65535.0;
            let srgb = if linear <= 0.003_130_8 {
                linear * 12.92
            } else {
                1.055 * linear.powf(1.0 / 2.4) - 0.055
            };
            (srgb.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
        };
        for (pixel, expected_rgba) in [(0, &channels[0..4]), (1, &channels[4..8])] {
            let expected = [
                srgb8(expected_rgba[2]),
                srgb8(expected_rgba[1]),
                srgb8(expected_rgba[0]),
                255u8,
            ];
            let got = &bytes[pixel * 4..pixel * 4 + 4];
            for (lane, (g, e)) in got.iter().zip(&expected).enumerate() {
                assert!(
                    g.abs_diff(*e) <= 1,
                    "pixel {pixel} lane {lane}: gpu {g} vs cpu {e}"
                );
            }
        }
    }

    #[test]
    fn srgb_target_formats_are_rejected() {
        let Some((device, queue)) = test_gpu() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let graphics = GpuGraphics::new(
            device.clone(),
            queue,
            Box::new(lp_gfx_lpvm::TargetLpvmGraphics::new()),
        );
        let product = graphics.create_render_target(1, 1).expect("target");
        let stand_in = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("srgb stand-in"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = stand_in.create_view(&wgpu::TextureViewDescriptor::default());
        let result = present_to_view(
            graphics.shared_for_tests(),
            gpu_texture(&product).expect("gpu backing"),
            &view,
            wgpu::TextureFormat::Bgra8UnormSrgb,
        );
        match result {
            Err(GfxError::Backend(message)) => {
                assert!(message.contains("sRGB"), "{message}");
            }
            other => panic!("expected sRGB rejection, got {other:?}"),
        }
    }
}
