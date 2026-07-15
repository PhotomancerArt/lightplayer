//! [`GpuGraphics`]: the wgpu implementation of `lp_gfx::LpGraphics`.

use std::sync::{Arc, OnceLock};

use lp_gfx::{
    GfxError, HandleAllocator, HandleBacking, LpComputeShader, LpGraphics, LpShader,
    SampleOutHandle, SamplePointsHandle, ShaderCompileOptions, ShaderSemantics, TextureData,
    TextureHandle,
};
use lps_shared::{LpsTexture2DDescriptor, LpsTexture2DValue, LpsValueF32, TextureStorageFormat};

use crate::blend::{BlendPipeline, blend_textures_gpu};
use crate::read_back::read_back_texture;
use crate::render::GpuShader;
use crate::sample_backing::{
    CpuSampleOut, CpuSamplePoints, sample_out, sample_out_mut, sample_points, sample_points_mut,
};
use crate::texture_backing::{GpuTexture, gpu_channels, gpu_texture, gpu_texture_mut};
use crate::texture_registry::{RegisteredTexture, TextureRegistry};

/// GPU shader graphics on a wgpu device at f32 semantics.
///
/// Construction is sans-IO: the host performs its own async adapter/device
/// request at its platform edge and passes the resulting `Device`/`Queue`
/// here (browser: the fw-browser worker; native: the server host; tests: an
/// adapter-gated pollster helper). Compute shaders are delegated to
/// `compute_delegate` (in practice `lp_gfx_lpvm::TargetLpvmGraphics`) — the
/// compute tier is CPU permanently.
pub struct GpuGraphics {
    shared: Arc<GpuShared>,
    compute_delegate: Box<dyn LpGraphics>,
}

impl GpuGraphics {
    /// Wrap an already-created wgpu device/queue pair plus the CPU backend
    /// that services compute shaders.
    pub fn new(
        device: wgpu::Device,
        queue: wgpu::Queue,
        compute_delegate: Box<dyn LpGraphics>,
    ) -> Self {
        Self {
            shared: Arc::new(GpuShared {
                device,
                queue,
                blend_pipeline: OnceLock::new(),
                surface_blit_pipeline: OnceLock::new(),
                textures: TextureRegistry::new(),
            }),
            compute_delegate,
        }
    }

    /// The uniform-tree value for a texture input: the engine currency
    /// (`LpsValueF32::Texture2D`) with this backend's registry id in the
    /// descriptor's `ptr` lane. The CPU tier's counterpart is
    /// `LpsTextureBuf::to_texture2d_value()` (a real guest pointer); on the
    /// GPU tier the descriptor leg resolves to a bind-group entry at render
    /// time instead.
    pub fn texture_uniform_value(&self, texture: &TextureHandle) -> Result<LpsValueF32, GfxError> {
        let backing = gpu_texture(texture)?;
        let (width, height, format) = (texture.width(), texture.height(), texture.format());
        let bytes_per_pixel = format.bytes_per_pixel() as u32;
        let row_stride = width * bytes_per_pixel;
        Ok(LpsValueF32::Texture2D(LpsTexture2DValue {
            descriptor: LpsTexture2DDescriptor {
                ptr: backing.id,
                width,
                height,
                row_stride,
            },
            format,
            byte_len: row_stride as usize * height as usize,
        }))
    }

    /// Raw pre-quantization backing floats behind a texture handle
    /// (`gpu_channels(format)` lanes per pixel, row-major).
    ///
    /// Conformance/probe affordance: `LpGraphics::read_back` quantizes to
    /// the unorm16 product grid, which would mask non-finite lanes (they
    /// quantize to 0). Native only, like `read_back` itself.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn read_back_f32(&self, texture: &TextureHandle) -> Result<Vec<f32>, GfxError> {
        crate::read_back::read_back_f32(
            &self.shared.device,
            &self.shared.queue,
            gpu_texture(texture)?,
            texture.width(),
            texture.height(),
            texture.format(),
        )
    }

    /// Present a render product to a wgpu surface (zero readback).
    ///
    /// The GPU-tier card path: the product texture is blitted to the
    /// surface's current frame through the fixed present pipeline
    /// (unorm16-grid quantization + sRGB encode matching the CPU tier's
    /// byte conversion) and the frame is presented. The surface must be
    /// configured with a **non-sRGB** color format; see
    /// [`crate::surface_blit`] for the single-encode invariant.
    pub fn present_to_surface(
        &self,
        texture: &TextureHandle,
        surface: &wgpu::Surface<'_>,
    ) -> Result<(), GfxError> {
        let frame = match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            status @ (wgpu::CurrentSurfaceTexture::Timeout
            | wgpu::CurrentSurfaceTexture::Occluded
            | wgpu::CurrentSurfaceTexture::Outdated
            | wgpu::CurrentSurfaceTexture::Lost
            | wgpu::CurrentSurfaceTexture::Validation) => {
                return Err(GfxError::Render(format!(
                    "surface frame acquire: {status:?}"
                )));
            }
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        crate::surface_blit::present_to_view(
            &self.shared,
            gpu_texture(texture)?,
            &view,
            frame.texture.format(),
        )?;
        frame.present();
        Ok(())
    }

    fn allocator(&self) -> Arc<dyn HandleAllocator> {
        self.shared.clone()
    }

    #[cfg(test)]
    pub(crate) fn shared_for_tests(&self) -> &GpuShared {
        &self.shared
    }

    fn texture_handle(
        &self,
        width: u32,
        height: u32,
        format: TextureStorageFormat,
        mut backing: GpuTexture,
    ) -> TextureHandle {
        backing.id = self.shared.textures.register(RegisteredTexture {
            view: backing.view.clone(),
            width,
            height,
            format,
        });
        TextureHandle::from_backend_parts(
            width,
            height,
            format,
            Box::new(backing),
            self.allocator(),
        )
    }
}

impl LpGraphics for GpuGraphics {
    /// Compile authored GLSL for GPU execution.
    ///
    /// Honor-or-fail semantics contract
    /// (`docs/adr/2026-07-09-preview-fidelity-tiers.md`): this backend
    /// implements [`ShaderSemantics::F32Gpu`] only. An explicit
    /// [`ShaderSemantics::Q32`] request is rejected with
    /// [`GfxError::Backend`] — Q32 options are never silently dropped onto
    /// float GPU arithmetic. (`options.frontend` selects a GLSL → LPIR
    /// frontend and does not apply here: the GPU tier forks at the GLSL
    /// source and always translates through naga glsl-in.)
    fn compile_shader(
        &self,
        source: &str,
        options: &ShaderCompileOptions,
    ) -> Result<Box<dyn LpShader>, GfxError> {
        if options.semantics != ShaderSemantics::F32Gpu {
            return Err(GfxError::Backend(format!(
                "wgpu GPU backend only compiles F32Gpu semantics; explicit {:?} tier requested \
                 (see docs/adr/2026-07-09-preview-fidelity-tiers.md)",
                options.semantics
            )));
        }
        Ok(Box::new(GpuShader::new(
            self.shared.clone(),
            source,
            &options.textures,
        )?))
    }

    fn compile_compute_shader(
        &self,
        desc: lp_shader::CompileComputeDesc<'_>,
    ) -> Result<Box<dyn LpComputeShader>, GfxError> {
        self.compute_delegate.compile_compute_shader(desc)
    }

    fn backend_name(&self) -> &'static str {
        "wgpu"
    }

    fn native_semantics(&self) -> ShaderSemantics {
        ShaderSemantics::F32Gpu
    }

    fn create_render_target(&self, width: u32, height: u32) -> Result<TextureHandle, GfxError> {
        let format = TextureStorageFormat::Rgba16Unorm;
        let backing = GpuTexture::new(
            &self.shared.device,
            width,
            height,
            format,
            "lp-gfx-wgpu render target",
        );
        Ok(self.texture_handle(width, height, format, backing))
    }

    fn create_texture(
        &self,
        width: u32,
        height: u32,
        format: TextureStorageFormat,
        texels: &[u8],
    ) -> Result<TextureHandle, GfxError> {
        let backing = GpuTexture::new(
            &self.shared.device,
            width,
            height,
            format,
            "lp-gfx-wgpu texture",
        );
        backing.upload(&self.shared.queue, width, height, format, texels)?;
        Ok(self.texture_handle(width, height, format, backing))
    }

    fn write_texture(&self, texture: &mut TextureHandle, texels: &[u8]) -> Result<(), GfxError> {
        let (width, height, format) = (texture.width(), texture.height(), texture.format());
        gpu_texture_mut(texture)?.upload(&self.shared.queue, width, height, format, texels)
    }

    fn clear_texture(&self, texture: &mut TextureHandle) -> Result<(), GfxError> {
        let (width, height, format) = (texture.width(), texture.height(), texture.format());
        let zeros = vec![0.0f32; width as usize * height as usize * gpu_channels(format)];
        gpu_texture_mut(texture)?.upload_f32(&self.shared.queue, width, height, format, &zeros);
        Ok(())
    }

    fn blend_textures(
        &self,
        previous: &TextureHandle,
        active: &TextureHandle,
        alpha: f32,
        target: &mut TextureHandle,
    ) -> Result<(), GfxError> {
        let same_shape = |t: &TextureHandle| {
            t.width() == target.width()
                && t.height() == target.height()
                && t.format() == target.format()
        };
        if !same_shape(previous) || !same_shape(active) {
            return Err(GfxError::Backend(String::from(
                "blend_textures: texture shape mismatch",
            )));
        }
        let previous = gpu_texture(previous)?;
        let active = gpu_texture(active)?;
        let target = gpu_texture(target)?;
        blend_textures_gpu(&self.shared, previous, active, alpha, target)
    }

    fn read_back(&self, texture: &TextureHandle) -> Result<TextureData, GfxError> {
        read_back_texture(
            &self.shared.device,
            &self.shared.queue,
            gpu_texture(texture)?,
            texture.width(),
            texture.height(),
            texture.format(),
        )
    }

    /// Native wgpu can block on a buffer map; the browser tier cannot, so
    /// render products stay GPU-resident there (fidelity-tiers ADR — the
    /// probe edge surfaces the residency instead of an error string).
    fn supports_read_back(&self) -> bool {
        cfg!(not(target_arch = "wasm32"))
    }

    fn create_sample_points(&self, count: u32) -> Result<SamplePointsHandle, GfxError> {
        Ok(SamplePointsHandle::from_backend_parts(
            count,
            Box::new(CpuSamplePoints(vec![0; count as usize * 2])),
            self.allocator(),
        ))
    }

    fn write_sample_points(
        &self,
        points: &mut SamplePointsHandle,
        xy_q16: &[i32],
    ) -> Result<(), GfxError> {
        let buffer = &mut sample_points_mut(points)?.0;
        if buffer.len() != xy_q16.len() {
            return Err(len_mismatch(
                "sample point coordinates",
                buffer.len(),
                xy_q16.len(),
            ));
        }
        buffer.copy_from_slice(xy_q16);
        Ok(())
    }

    fn read_sample_points(&self, points: &SamplePointsHandle) -> Result<Vec<i32>, GfxError> {
        Ok(sample_points(points)?.0.clone())
    }

    fn create_sample_out(&self, count: u32) -> Result<SampleOutHandle, GfxError> {
        Ok(SampleOutHandle::from_backend_parts(
            count,
            Box::new(CpuSampleOut(vec![0; count as usize * 4])),
            self.allocator(),
        ))
    }

    fn write_sample_out(&self, out: &mut SampleOutHandle, rgba16: &[u16]) -> Result<(), GfxError> {
        let buffer = &mut sample_out_mut(out)?.0;
        if buffer.len() != rgba16.len() {
            return Err(len_mismatch(
                "sample out channels",
                buffer.len(),
                rgba16.len(),
            ));
        }
        buffer.copy_from_slice(rgba16);
        Ok(())
    }

    fn read_sample_out(&self, out: &SampleOutHandle) -> Result<Vec<u16>, GfxError> {
        Ok(sample_out(out)?.0.clone())
    }

    fn clear_sample_out(&self, out: &mut SampleOutHandle) -> Result<(), GfxError> {
        sample_out_mut(out)?.0.fill(0);
        Ok(())
    }
}

/// Device/queue shared between the backend facade, every compiled shader,
/// and every live handle (the allocator `Arc` keeps the device alive while
/// they exist).
pub(crate) struct GpuShared {
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
    /// Fixed blend pipeline, built on first use (not a shader cache).
    pub(crate) blend_pipeline: OnceLock<BlendPipeline>,
    /// Fixed surface-present pipeline, built on first use for the surface
    /// format (one surface format per device).
    pub(crate) surface_blit_pipeline: OnceLock<crate::surface_blit::SurfaceBlitPipeline>,
    /// Live texture id → view map for texture uniform resolution
    /// (see [`crate::texture_registry`]).
    pub(crate) textures: TextureRegistry,
}

impl HandleAllocator for GpuShared {
    fn free_texture(&self, backing: HandleBacking) {
        // wgpu resources release on drop; the allocator drops the registry
        // entry (ending uniform-value resolution for the id) and keeps the
        // device alive while handles exist.
        if let Some(texture) = backing.downcast_ref::<GpuTexture>() {
            self.textures.unregister(texture.id);
        }
        drop(backing);
    }

    fn free_sample_points(&self, backing: HandleBacking) {
        drop(backing);
    }

    fn free_sample_out(&self, backing: HandleBacking) {
        drop(backing);
    }
}

fn len_mismatch(what: &'static str, expected: usize, got: usize) -> GfxError {
    GfxError::Backend(format!(
        "{what} write length mismatch: expected {expected}, got {got}"
    ))
}

#[cfg(test)]
mod tests {
    use lp_gfx_lpvm::TargetLpvmGraphics;
    use lps_shared::LpsValueF32;

    use super::*;
    use crate::test_gpu::test_gpu;

    fn test_graphics() -> Option<GpuGraphics> {
        let (device, queue) = test_gpu()?;
        Some(GpuGraphics::new(
            device,
            queue,
            Box::new(TargetLpvmGraphics::new()),
        ))
    }

    fn rgba16_bytes(channels: &[u16]) -> Vec<u8> {
        channels.iter().flat_map(|v| v.to_le_bytes()).collect()
    }

    #[test]
    fn backend_name_is_wgpu() {
        let Some(graphics) = test_graphics() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        assert_eq!(graphics.backend_name(), "wgpu");
    }

    #[test]
    fn q32_semantics_are_rejected_explicitly() {
        let Some(graphics) = test_graphics() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let options = ShaderCompileOptions {
            semantics: ShaderSemantics::Q32,
            ..Default::default()
        };
        match graphics.compile_shader("vec4 render(vec2 pos) { return vec4(0.0); }", &options) {
            Err(GfxError::Backend(message)) => {
                assert!(message.contains("Q32"), "message names the tier: {message}");
            }
            Err(other) => panic!("expected GfxError::Backend, got {other:?}"),
            Ok(_) => panic!("Q32 must be rejected"),
        }
    }

    #[test]
    fn compute_shaders_delegate_to_the_cpu_tier() {
        let Some(graphics) = test_graphics() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let desc = lp_shader::CompileComputeDesc::new(
            r#"
layout(binding = 0) uniform float x;
float y;
void tick() {
    y = x + 1.0;
}
"#,
            lpir::CompilerConfig::default(),
        )
        .with_consumed("x", lps_shared::LpsType::Float)
        .with_produced("y", lps_shared::LpsType::Float);

        let mut shader = graphics
            .compile_compute_shader(desc)
            .expect("compute compiles via the inner CPU tier");
        shader.tick(&[("x", LpsValueF32::F32(2.0))]).expect("tick");
        assert!(
            shader
                .get_output("y")
                .expect("output")
                .approx_eq_default(&LpsValueF32::F32(3.0))
        );
    }

    #[test]
    fn texture_upload_read_back_round_trips_bytes() {
        let Some(graphics) = test_graphics() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let texels = rgba16_bytes(&[0, 1, 32767, 65535, 4660, 22136, 43981, 65534]);
        let texture = graphics
            .create_texture(2, 1, TextureStorageFormat::Rgba16Unorm, &texels)
            .expect("create");
        let data = graphics.read_back(&texture).expect("read back");
        assert_eq!(data.bytes(), &texels[..], "byte-exact round trip");
    }

    #[test]
    fn render_produces_the_expected_pixels() {
        let Some(graphics) = test_graphics() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let options = ShaderCompileOptions {
            semantics: ShaderSemantics::F32Gpu,
            ..Default::default()
        };
        let mut shader = graphics
            .compile_shader(
                "layout(binding = 0) uniform vec2 outputSize;\n\
                 vec4 render(vec2 pos) { return vec4(pos / outputSize, 0.25, 1.0); }\n",
                &options,
            )
            .expect("compiles");
        let mut target = graphics.create_render_target(4, 4).expect("target");
        let uniforms = LpsValueF32::Struct {
            name: None,
            fields: vec![(String::from("outputSize"), LpsValueF32::Vec2([4.0, 4.0]))],
        };
        shader.render(&mut target, &uniforms).expect("renders");
        let data = graphics.read_back(&target).expect("read back");
        let channels: Vec<u16> = data
            .bytes()
            .chunks_exact(2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .collect();
        // Pixel (0,0): render(vec2(0,0)) → (0, 0, 0.25, 1.0).
        assert_eq!(channels[0], 0);
        assert_eq!(channels[1], 0);
        assert_eq!(channels[2], 16384, "0.25 → Q16.16 fraction");
        assert_eq!(channels[3], 65535, "1.0 saturates");
        // Pixel (2,1): render(vec2(2,1)) → (0.5, 0.25, …).
        let px = (4 + 2) * 4;
        assert_eq!(channels[px], 32768);
        assert_eq!(channels[px + 1], 16384);
    }

    #[test]
    fn missing_uniform_errors_at_render_time() {
        let Some(graphics) = test_graphics() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let options = ShaderCompileOptions {
            semantics: ShaderSemantics::F32Gpu,
            ..Default::default()
        };
        let mut shader = graphics
            .compile_shader(
                "layout(binding = 0) uniform float time;\n\
                 vec4 render(vec2 pos) { return vec4(time); }\n",
                &options,
            )
            .expect("compiles");
        let mut target = graphics.create_render_target(2, 2).expect("target");
        let uniforms = LpsValueF32::Struct {
            name: None,
            fields: vec![],
        };
        match shader.render(&mut target, &uniforms) {
            Err(GfxError::Render(message)) => {
                assert!(message.contains("time"), "{message}");
            }
            other => panic!("expected missing-uniform render error, got {other:?}"),
        }
    }

    #[test]
    fn gpu_blend_agrees_with_the_cpu_tier_within_one_lsb() {
        let Some(graphics) = test_graphics() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let cpu = TargetLpvmGraphics::new();

        // Deterministic pseudo-random u16 channels (2×2 RGBA16).
        let previous: Vec<u16> = (0..16u32)
            .map(|i| (i.wrapping_mul(40503) % 65536) as u16)
            .collect();
        let active: Vec<u16> = (0..16u32)
            .map(|i| (i.wrapping_mul(48271).wrapping_add(12345) % 65536) as u16)
            .collect();

        for alpha in [0.0f32, 0.25, 0.5, 0.75, 1.0, 0.37] {
            let blend_on = |graphics: &dyn LpGraphics| -> Vec<u16> {
                let previous_tex = graphics
                    .create_texture(
                        2,
                        2,
                        TextureStorageFormat::Rgba16Unorm,
                        &rgba16_bytes(&previous),
                    )
                    .expect("previous");
                let active_tex = graphics
                    .create_texture(
                        2,
                        2,
                        TextureStorageFormat::Rgba16Unorm,
                        &rgba16_bytes(&active),
                    )
                    .expect("active");
                let mut target = graphics.create_render_target(2, 2).expect("target");
                graphics
                    .blend_textures(&previous_tex, &active_tex, alpha, &mut target)
                    .expect("blend");
                graphics
                    .read_back(&target)
                    .expect("read back")
                    .bytes()
                    .chunks_exact(2)
                    .map(|b| u16::from_le_bytes([b[0], b[1]]))
                    .collect()
            };
            let gpu_result = blend_on(&graphics);
            let cpu_result = blend_on(&cpu);
            for (i, (g, c)) in gpu_result.iter().zip(&cpu_result).enumerate() {
                assert!(
                    g.abs_diff(*c) <= 1,
                    "alpha {alpha}, channel {i}: gpu {g} vs cpu {c}"
                );
            }
        }
    }

    #[test]
    fn texture_uniform_renders_fetched_texels_exactly() {
        let Some(graphics) = test_graphics() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let texels = rgba16_bytes(&[
            8192, 16384, 24576, 65535, // (0,0): 0.125, 0.25, 0.375, 1.0
            65535, 49151, 32768, 16384, // (1,0)
        ]);
        let texture = graphics
            .create_texture(2, 1, TextureStorageFormat::Rgba16Unorm, &texels)
            .expect("create");

        let mut options = ShaderCompileOptions {
            semantics: ShaderSemantics::F32Gpu,
            ..Default::default()
        };
        options.textures.insert(
            String::from("inputColor"),
            lp_shader::texture_binding::texture2d(
                TextureStorageFormat::Rgba16Unorm,
                lps_shared::TextureFilter::Nearest,
                lps_shared::TextureWrap::ClampToEdge,
                lps_shared::TextureWrap::ClampToEdge,
            ),
        );
        let mut shader = graphics
            .compile_shader(
                "uniform sampler2D inputColor;\n\
                 vec4 render(vec2 pos) { return texelFetch(inputColor, ivec2(int(pos.x), 0), 0); }\n",
                &options,
            )
            .expect("compiles");

        let mut target = graphics.create_render_target(2, 1).expect("target");
        let uniforms = LpsValueF32::Struct {
            name: None,
            fields: vec![(
                String::from("inputColor"),
                graphics
                    .texture_uniform_value(&texture)
                    .expect("uniform value"),
            )],
        };
        shader.render(&mut target, &uniforms).expect("renders");
        let data = graphics.read_back(&target).expect("read back");
        assert_eq!(data.bytes(), &texels[..], "texelFetch is bit-exact");
    }

    #[test]
    fn texture_render_validation_errors_match_the_contract() {
        let Some(graphics) = test_graphics() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let texels = rgba16_bytes(&[0, 0, 0, 0]);
        let texture = graphics
            .create_texture(1, 1, TextureStorageFormat::Rgba16Unorm, &texels)
            .expect("create");

        let mut options = ShaderCompileOptions {
            semantics: ShaderSemantics::F32Gpu,
            ..Default::default()
        };
        options.textures.insert(
            String::from("t"),
            lp_shader::texture_binding::height_one(
                TextureStorageFormat::Rgba16Unorm,
                lps_shared::TextureFilter::Nearest,
                lps_shared::TextureWrap::ClampToEdge,
            ),
        );
        let source = "uniform sampler2D t;\n\
                      vec4 render(vec2 pos) { return texture(t, pos); }\n";
        let mut shader = graphics.compile_shader(source, &options).expect("compiles");
        let mut target = graphics.create_render_target(1, 1).expect("target");

        // Missing texture uniform field.
        let missing = LpsValueF32::Struct {
            name: None,
            fields: vec![],
        };
        match shader.render(&mut target, &missing) {
            Err(GfxError::Render(message)) => assert!(message.contains("t"), "{message}"),
            other => panic!("expected missing-texture render error, got {other:?}"),
        }

        // Format mismatch between value and compile-time spec.
        let mut wrong_format = graphics.texture_uniform_value(&texture).expect("value");
        if let LpsValueF32::Texture2D(tv) = &mut wrong_format {
            tv.format = TextureStorageFormat::R16Unorm;
        }
        let uniforms = LpsValueF32::Struct {
            name: None,
            fields: vec![(String::from("t"), wrong_format)],
        };
        match shader.render(&mut target, &uniforms) {
            Err(GfxError::Render(message)) => {
                assert!(message.contains("format"), "{message}");
            }
            other => panic!("expected format-mismatch render error, got {other:?}"),
        }

        // HeightOne promised but the runtime texture is 2 rows tall.
        let tall = graphics
            .create_texture(
                1,
                2,
                TextureStorageFormat::Rgba16Unorm,
                &rgba16_bytes(&[0; 8]),
            )
            .expect("create");
        let uniforms = LpsValueF32::Struct {
            name: None,
            fields: vec![(
                String::from("t"),
                graphics.texture_uniform_value(&tall).expect("value"),
            )],
        };
        match shader.render(&mut target, &uniforms) {
            Err(GfxError::Render(message)) => {
                assert!(message.contains("HeightOne"), "{message}");
            }
            other => panic!("expected height-one render error, got {other:?}"),
        }

        // A dropped texture no longer resolves (registry unregistered).
        let stale = graphics.texture_uniform_value(&texture).expect("value");
        drop(texture);
        let uniforms = LpsValueF32::Struct {
            name: None,
            fields: vec![(String::from("t"), stale)],
        };
        match shader.render(&mut target, &uniforms) {
            Err(GfxError::Render(message)) => {
                assert!(message.contains("live texture"), "{message}");
            }
            other => panic!("expected stale-texture render error, got {other:?}"),
        }
    }

    #[test]
    fn texture_spec_mismatches_fail_compilation() {
        let Some(graphics) = test_graphics() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        // Declared sampler without a spec.
        let options = ShaderCompileOptions {
            semantics: ShaderSemantics::F32Gpu,
            ..Default::default()
        };
        match graphics.compile_shader(
            "uniform sampler2D mystery;\n\
             vec4 render(vec2 pos) { return vec4(0.0); }\n",
            &options,
        ) {
            Err(GfxError::Compile(message)) => {
                assert!(message.contains("mystery"), "{message}");
            }
            Err(other) => panic!("expected missing-spec compile error, got {other:?}"),
            Ok(_) => panic!("missing spec must fail compilation"),
        }

        // Spec naming a sampler the shader does not declare.
        let mut options = ShaderCompileOptions {
            semantics: ShaderSemantics::F32Gpu,
            ..Default::default()
        };
        options.textures.insert(
            String::from("ghost"),
            lp_shader::texture_binding::texture2d(
                TextureStorageFormat::Rgba16Unorm,
                lps_shared::TextureFilter::Nearest,
                lps_shared::TextureWrap::ClampToEdge,
                lps_shared::TextureWrap::ClampToEdge,
            ),
        );
        match graphics.compile_shader("vec4 render(vec2 pos) { return vec4(0.0); }", &options) {
            Err(GfxError::Compile(message)) => {
                assert!(message.contains("ghost"), "{message}");
            }
            Err(other) => panic!("expected extra-spec compile error, got {other:?}"),
            Ok(_) => panic!("extra spec must fail compilation"),
        }
    }

    #[test]
    fn clear_texture_zeroes_every_texel() {
        let Some(graphics) = test_graphics() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let texels = rgba16_bytes(&[1000, 2000, 3000, 4000]);
        let mut texture = graphics
            .create_texture(1, 1, TextureStorageFormat::Rgba16Unorm, &texels)
            .expect("create");
        graphics.clear_texture(&mut texture).expect("clear");
        let data = graphics.read_back(&texture).expect("read back");
        assert!(data.bytes().iter().all(|&b| b == 0));
    }

    #[test]
    fn sample_buffers_round_trip_on_the_cpu_side() {
        let Some(graphics) = test_graphics() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let mut points = graphics.create_sample_points(2).expect("points");
        graphics
            .write_sample_points(&mut points, &[1, 2, 3, 4])
            .expect("write");
        assert_eq!(
            graphics.read_sample_points(&points).expect("read"),
            vec![1, 2, 3, 4]
        );
        let mut out = graphics.create_sample_out(1).expect("out");
        graphics
            .write_sample_out(&mut out, &[5, 6, 7, 8])
            .expect("write");
        assert_eq!(
            graphics.read_sample_out(&out).expect("read"),
            vec![5, 6, 7, 8]
        );
        graphics.clear_sample_out(&mut out).expect("clear");
        assert_eq!(
            graphics.read_sample_out(&out).expect("read"),
            vec![0, 0, 0, 0]
        );
    }
}
