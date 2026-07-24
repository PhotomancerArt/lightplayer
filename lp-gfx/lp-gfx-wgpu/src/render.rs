//! [`GpuShader`]: a compiled fragment pipeline implementing
//! `lp_gfx::LpShader` (fullscreen-triangle pass into a render target).

use std::sync::Arc;

use lp_gfx::{GfxError, LpShader, SampleOutHandle, SamplePointsHandle, TextureHandle};
use lp_shader::TextureBindingSpecs;
use lps_shared::{LpsValueF32, TextureShapeHint, TextureStorageFormat};

use crate::gpu_graphics::GpuShared;
#[cfg(not(target_arch = "wasm32"))]
use crate::sample_pass::SamplePass;
use crate::texture_backing::{gpu_format, gpu_texture_mut};
use crate::uniform_layout::{TextureGlobal, UniformTable, reflect_textures, reflect_uniforms};
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
/// Texture uniforms (`LpsValueF32::Texture2D` fields) resolve through the
/// backend's texture registry into bind-group entries per render call.
pub struct GpuShader {
    shared: Arc<GpuShared>,
    /// Authored GLSL, kept for the lazily-built sample pipeline (the sample
    /// unit re-assembles from source with a different wrapper `main`).
    #[cfg(not(target_arch = "wasm32"))]
    authored: String,
    /// Compile-time texture specs, kept so the sample unit lowers texture
    /// call sites identically to the render unit.
    #[cfg(not(target_arch = "wasm32"))]
    texture_specs: TextureBindingSpecs,
    /// Sample-point pass (native LED-output path), built on the first
    /// `sample_rgba16` call — render-only consumers never pay for it.
    #[cfg(not(target_arch = "wasm32"))]
    sample_pass: Option<SamplePass>,
    /// Validated naga module (drives uniform encoding offsets).
    module: naga::Module,
    table: UniformTable,
    /// Reflected `sampler2D` uniforms joined with their compile-time specs.
    textures: Vec<TextureGlobal>,
    pipeline: wgpu::RenderPipeline,
    /// `Some` when the shader has any `@group(0)` bindings.
    bindings: Option<ShaderBindings>,
}

/// The shader's `@group(0)` resources.
struct ShaderBindings {
    layout: wgpu::BindGroupLayout,
    /// Instance uniform buffer and its per-global slices (`None` when the
    /// shader has texture bindings only).
    uniforms: Option<ShaderUniforms>,
    /// Prebuilt bind group when every entry is static (no texture
    /// bindings); with textures the group is rebuilt per render from the
    /// uniform tree's texture values.
    static_bind_group: Option<wgpu::BindGroup>,
}

/// The instance uniform buffer and its per-global slices.
struct ShaderUniforms {
    buffer: wgpu::Buffer,
    /// Byte offset of each `table.globals` entry inside `buffer`.
    offsets: Vec<u64>,
}

impl GpuShader {
    /// Compile authored GLSL into a render pipeline on `shared`'s device.
    /// `textures` is the compile-time `TextureBindingSpec` map (shared
    /// contract with the CPU tier; mismatches fail compilation).
    pub(crate) fn new(
        shared: Arc<GpuShared>,
        authored: &str,
        textures: &TextureBindingSpecs,
    ) -> Result<Self, GfxError> {
        let compiled = compile_wgsl(authored, textures)?;
        let table = reflect_uniforms(&compiled.module)?;
        let texture_globals = reflect_textures(&compiled.module, textures)?;
        let device = &shared.device;

        let fragment_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("lp-gfx-wgpu fragment (authored GLSL via naga wgsl-out)"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(&compiled.wgsl)),
        });
        let vertex_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("lp-gfx-wgpu fullscreen triangle"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(FULLSCREEN_TRIANGLE_WGSL)),
        });

        let bindings = if table.globals.is_empty() && texture_globals.is_empty() {
            None
        } else {
            let mut entries: Vec<wgpu::BindGroupLayoutEntry> = table
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
            for texture in &texture_globals {
                entries.push(wgpu::BindGroupLayoutEntry {
                    binding: texture.binding,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    // The float backing formats (`Rgba32Float`/`R32Float`)
                    // are non-filterable; all filtering is generated code
                    // over `textureLoad` (see `crate::texture_lowering`),
                    // so no `float32-filterable` feature is required.
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                });
            }
            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("lp-gfx-wgpu shader bindings"),
                entries: &entries,
            });

            let uniforms = if table.globals.is_empty() {
                None
            } else {
                // One buffer, one alignment-padded slice per uniform global.
                let alignment =
                    u64::from(device.limits().min_uniform_buffer_offset_alignment.max(4));
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
                Some(ShaderUniforms { buffer, offsets })
            };

            let static_bind_group = if texture_globals.is_empty() {
                Some(build_bind_group(
                    device,
                    &layout,
                    &table,
                    uniforms.as_ref(),
                    &[],
                ))
            } else {
                None
            };

            Some(ShaderBindings {
                layout,
                uniforms,
                static_bind_group,
            })
        };

        let layouts: Vec<Option<&wgpu::BindGroupLayout>> = bindings
            .as_ref()
            .map(|b| &b.layout)
            .into_iter()
            .map(Some)
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
            textures: texture_globals,
            pipeline,
            bindings,
            #[cfg(not(target_arch = "wasm32"))]
            texture_specs: textures.clone(),
            #[cfg(not(target_arch = "wasm32"))]
            sample_pass: None,
        })
    }

    /// Build the sample-point pipeline on first use: translate the sample
    /// wrapper unit, check its uniform interface matches the render unit's
    /// (same authored declarations — a mismatch means the assembler broke),
    /// and build the point-list pipeline over the shared bind group layout.
    #[cfg(not(target_arch = "wasm32"))]
    fn ensure_sample_pass(&mut self) -> Result<(), GfxError> {
        if self.sample_pass.is_none() {
            let compiled =
                crate::wgsl_compile::compile_sample_wgsl(&self.authored, &self.texture_specs)?;
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
                self.bindings.as_ref().map(|bindings| &bindings.layout),
            ));
        }
        Ok(())
    }

    /// Filetest probe render: draw into a fresh `width`×1 target and return
    /// the **raw f32 backing texels** (4 floats per pixel, row order). The
    /// GPU backing for render targets is `Rgba32Float` (quantization to
    /// unorm16 only happens in `read_back`), so scalar bit patterns encoded
    /// by a probe wrapper survive readback exactly. Native-only, like
    /// `read_back`.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn probe_f32(&mut self, width: u32, uniforms: &LpsValueF32) -> Result<Vec<f32>, GfxError> {
        let scope = self
            .shared
            .device
            .push_error_scope(wgpu::ErrorFilter::Validation);
        let result = self.probe_f32_inner(width, uniforms);
        let scope_err = pollster::block_on(scope.pop());
        match (result, scope_err) {
            (_, Some(e)) => Err(GfxError::Render(format!("wgpu validation: {e}"))),
            (r, None) => r,
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn probe_f32_inner(
        &mut self,
        width: u32,
        uniforms: &LpsValueF32,
    ) -> Result<Vec<f32>, GfxError> {
        use crate::read_back::read_back_f32;
        use crate::texture_backing::GpuTexture;

        let backing = GpuTexture::new(
            &self.shared.device,
            width,
            1,
            TextureStorageFormat::Rgba16Unorm,
            "lp-gfx-wgpu probe target",
        );

        let encoded = encode_uniforms(&self.module, &self.table, uniforms)?;
        let texture_views = self.resolve_texture_views(uniforms)?;

        let mut per_render_bind_group = None;
        if let Some(bindings) = &self.bindings {
            if let Some(shader_uniforms) = &bindings.uniforms {
                for ((_, bytes), &offset) in encoded.iter().zip(&shader_uniforms.offsets) {
                    self.shared
                        .queue
                        .write_buffer(&shader_uniforms.buffer, offset, bytes);
                }
            }
            if bindings.static_bind_group.is_none() {
                per_render_bind_group = Some(build_bind_group(
                    &self.shared.device,
                    &bindings.layout,
                    &self.table,
                    bindings.uniforms.as_ref(),
                    &texture_views,
                ));
            }
        }

        let mut encoder = self
            .shared
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("lp-gfx-wgpu probe render"),
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
            if let Some(bindings) = &self.bindings {
                let bind_group = per_render_bind_group
                    .as_ref()
                    .or(bindings.static_bind_group.as_ref())
                    .expect("bindings imply a static or per-render bind group");
                pass.set_bind_group(0, bind_group, &[]);
            }
            pass.draw(0..3, 0..1);
        }
        self.shared.queue.submit([encoder.finish()]);

        // Bounded wait: corpus shaders may not terminate (CPU targets rely
        // on fuel exhaustion; the GPU has none). A hung submission surfaces
        // as PollError::Timeout instead of hanging the harness forever.
        read_back_f32(
            &self.shared.device,
            &self.shared.queue,
            &backing,
            width,
            1,
            TextureStorageFormat::Rgba16Unorm,
            Some(core::time::Duration::from_secs(20)),
        )
    }

    /// Resolve the shader's texture bindings from the uniform tree: each
    /// `LpsValueF32::Texture2D` value is validated against the compile-time
    /// spec (format, `HeightOne` promise) and its registry id resolved to
    /// the live wgpu view.
    fn resolve_texture_views(
        &self,
        uniforms: &LpsValueF32,
    ) -> Result<Vec<(u32, wgpu::TextureView)>, GfxError> {
        let mut views = Vec::with_capacity(self.textures.len());
        for texture in &self.textures {
            let value = lookup_uniform_path(uniforms, &texture.name)?;
            let LpsValueF32::Texture2D(tv) = value else {
                return Err(GfxError::Render(format!(
                    "texture uniform `{}` expects LpsValueF32::Texture2D, engine value is {value:?}",
                    texture.name
                )));
            };
            if tv.format != texture.spec.format {
                return Err(GfxError::Render(format!(
                    "texture uniform `{}`: runtime format {:?} does not match the \
                     compile-time spec format {:?}",
                    texture.name, tv.format, texture.spec.format
                )));
            }
            if texture.spec.shape_hint == TextureShapeHint::HeightOne && tv.descriptor.height != 1 {
                return Err(GfxError::Render(format!(
                    "texture uniform `{}`: TextureShapeHint::HeightOne promised but runtime \
                     height is {}",
                    texture.name, tv.descriptor.height
                )));
            }
            let entry = self.shared.textures.get(tv.descriptor.ptr).ok_or_else(|| {
                GfxError::Render(format!(
                    "texture uniform `{}` does not reference a live texture of this wgpu \
                     backend (use GpuGraphics::texture_uniform_value)",
                    texture.name
                ))
            })?;
            if entry.width != tv.descriptor.width
                || entry.height != tv.descriptor.height
                || entry.format != tv.format
            {
                return Err(GfxError::Render(format!(
                    "texture uniform `{}`: descriptor {}x{} {:?} does not match the backing \
                     texture {}x{} {:?}",
                    texture.name,
                    tv.descriptor.width,
                    tv.descriptor.height,
                    tv.format,
                    entry.width,
                    entry.height,
                    entry.format
                )));
            }
            views.push((texture.binding, entry.view));
        }
        Ok(views)
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

        let encoded = encode_uniforms(&self.module, &self.table, uniforms)?;
        let texture_views = self.resolve_texture_views(uniforms)?;
        let backing = gpu_texture_mut(target)?;

        // Rebuilt per render when texture bindings are present (the bound
        // textures may change between frames); otherwise the prebuilt
        // static group.
        let mut per_render_bind_group = None;
        if let Some(bindings) = &self.bindings {
            if let Some(shader_uniforms) = &bindings.uniforms {
                for ((_, bytes), &offset) in encoded.iter().zip(&shader_uniforms.offsets) {
                    self.shared
                        .queue
                        .write_buffer(&shader_uniforms.buffer, offset, bytes);
                }
            }
            if bindings.static_bind_group.is_none() {
                per_render_bind_group = Some(build_bind_group(
                    &self.shared.device,
                    &bindings.layout,
                    &self.table,
                    bindings.uniforms.as_ref(),
                    &texture_views,
                ));
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
            if let Some(bindings) = &self.bindings {
                let bind_group = per_render_bind_group
                    .as_ref()
                    .or(bindings.static_bind_group.as_ref())
                    .expect("bindings imply a static or per-render bind group");
                pass.set_bind_group(0, bind_group, &[]);
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

        // Uniform writes + bind group exactly as the render path builds
        // them (shared layout; textures resolve per call).
        let encoded = encode_uniforms(&self.module, &self.table, uniforms)?;
        let texture_views = self.resolve_texture_views(uniforms)?;
        let mut per_call_bind_group = None;
        if let Some(bindings) = &self.bindings {
            if let Some(shader_uniforms) = &bindings.uniforms {
                for ((_, bytes), &offset) in encoded.iter().zip(&shader_uniforms.offsets) {
                    self.shared
                        .queue
                        .write_buffer(&shader_uniforms.buffer, offset, bytes);
                }
            }
            if bindings.static_bind_group.is_none() {
                per_call_bind_group = Some(build_bind_group(
                    &self.shared.device,
                    &bindings.layout,
                    &self.table,
                    bindings.uniforms.as_ref(),
                    &texture_views,
                ));
            }
        }

        let point_coords = &sample_points(points)?.0;
        let out_channels = &mut sample_out_mut(out)?.0;
        let Self {
            shared,
            bindings,
            sample_pass,
            ..
        } = self;
        let bind_group = per_call_bind_group.as_ref().or_else(|| {
            bindings
                .as_ref()
                .and_then(|bindings| bindings.static_bind_group.as_ref())
        });
        sample_pass
            .as_mut()
            .expect("sample pass was ensured above")
            .run(shared, point_coords, bind_group, out_channels)
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

/// Assemble the `@group(0)` bind group from the uniform buffer slices plus
/// resolved texture views.
fn build_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    table: &UniformTable,
    uniforms: Option<&ShaderUniforms>,
    texture_views: &[(u32, wgpu::TextureView)],
) -> wgpu::BindGroup {
    let mut entries: Vec<wgpu::BindGroupEntry> = Vec::new();
    if let Some(shader_uniforms) = uniforms {
        entries.extend(table.globals.iter().zip(&shader_uniforms.offsets).map(
            |(global, &offset)| wgpu::BindGroupEntry {
                binding: global.binding,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &shader_uniforms.buffer,
                    offset,
                    size: wgpu::BufferSize::new(u64::from(global.size)),
                }),
            },
        ));
    }
    entries.extend(
        texture_views
            .iter()
            .map(|(binding, view)| wgpu::BindGroupEntry {
                binding: *binding,
                resource: wgpu::BindingResource::TextureView(view),
            }),
    );
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("lp-gfx-wgpu shader bindings"),
        layout,
        entries: &entries,
    })
}

/// Walk a dotted uniform path (`params.gradient`) through nested
/// `LpsValueF32::Struct` fields, mirroring the CPU tier's path convention.
fn lookup_uniform_path<'a>(root: &'a LpsValueF32, path: &str) -> Result<&'a LpsValueF32, GfxError> {
    let mut current = root;
    for segment in path.split('.') {
        let LpsValueF32::Struct { fields, .. } = current else {
            return Err(GfxError::Render(format!(
                "missing texture uniform `{path}` (expected struct fields along the path)"
            )));
        };
        current = fields
            .iter()
            .find(|(name, _)| name == segment)
            .map(|(_, value)| value)
            .ok_or_else(|| GfxError::Render(format!("missing texture uniform `{path}`")))?;
    }
    Ok(current)
}
