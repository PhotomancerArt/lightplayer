//! Instrumented `LpGraphics` wrapper: delegates everything to the selected
//! backend and times the shader calls the render loop cares about
//! (`render`, `sample_rgba16`) from inside the engine — the reported sample
//! latency is the real in-server call, not a side benchmark.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use lp_gfx::{
    GfxError, LpComputeShader, LpGraphics, LpShader, SampleOutHandle, SamplePointsHandle,
    ShaderCompileOptions, ShaderCompileStats, ShaderSemantics, TextureData, TextureHandle,
};
use lps_shared::{LpsValueF32, TextureStorageFormat};

/// Durations recorded by every shader the wrapped backend compiles.
#[derive(Default)]
pub struct ShaderTimings {
    /// One entry per `LpShader::render` call.
    pub renders: Mutex<Vec<Duration>>,
    /// One `(point_count, duration)` entry per `LpShader::sample_rgba16`.
    pub samples: Mutex<Vec<(u32, Duration)>>,
}

/// [`LpGraphics`] decorator around the selected engine.
pub struct TimingGraphics {
    inner: Box<dyn LpGraphics>,
    timings: Arc<ShaderTimings>,
}

impl TimingGraphics {
    pub fn new(inner: Box<dyn LpGraphics>, timings: Arc<ShaderTimings>) -> Self {
        Self { inner, timings }
    }
}

impl LpGraphics for TimingGraphics {
    fn compile_shader(
        &self,
        source: &str,
        options: &ShaderCompileOptions,
    ) -> Result<Box<dyn LpShader>, GfxError> {
        let inner = self.inner.compile_shader(source, options)?;
        Ok(Box::new(TimingShader {
            inner,
            timings: self.timings.clone(),
        }))
    }

    fn compile_compute_shader(
        &self,
        desc: lp_shader::CompileComputeDesc<'_>,
    ) -> Result<Box<dyn LpComputeShader>, GfxError> {
        self.inner.compile_compute_shader(desc)
    }

    fn backend_name(&self) -> &'static str {
        self.inner.backend_name()
    }

    fn native_semantics(&self) -> ShaderSemantics {
        self.inner.native_semantics()
    }

    fn create_render_target(&self, width: u32, height: u32) -> Result<TextureHandle, GfxError> {
        self.inner.create_render_target(width, height)
    }

    fn create_texture(
        &self,
        width: u32,
        height: u32,
        format: TextureStorageFormat,
        texels: &[u8],
    ) -> Result<TextureHandle, GfxError> {
        self.inner.create_texture(width, height, format, texels)
    }

    fn write_texture(&self, texture: &mut TextureHandle, texels: &[u8]) -> Result<(), GfxError> {
        self.inner.write_texture(texture, texels)
    }

    fn clear_texture(&self, texture: &mut TextureHandle) -> Result<(), GfxError> {
        self.inner.clear_texture(texture)
    }

    fn blend_textures(
        &self,
        previous: &TextureHandle,
        active: &TextureHandle,
        alpha: f32,
        target: &mut TextureHandle,
    ) -> Result<(), GfxError> {
        self.inner.blend_textures(previous, active, alpha, target)
    }

    fn read_back(&self, texture: &TextureHandle) -> Result<TextureData, GfxError> {
        self.inner.read_back(texture)
    }

    fn supports_read_back(&self) -> bool {
        self.inner.supports_read_back()
    }

    fn create_sample_points(&self, count: u32) -> Result<SamplePointsHandle, GfxError> {
        self.inner.create_sample_points(count)
    }

    fn write_sample_points(
        &self,
        points: &mut SamplePointsHandle,
        xy_q16: &[i32],
    ) -> Result<(), GfxError> {
        self.inner.write_sample_points(points, xy_q16)
    }

    fn read_sample_points(&self, points: &SamplePointsHandle) -> Result<Vec<i32>, GfxError> {
        self.inner.read_sample_points(points)
    }

    fn create_sample_out(&self, count: u32) -> Result<SampleOutHandle, GfxError> {
        self.inner.create_sample_out(count)
    }

    fn write_sample_out(&self, out: &mut SampleOutHandle, rgba16: &[u16]) -> Result<(), GfxError> {
        self.inner.write_sample_out(out, rgba16)
    }

    fn read_sample_out(&self, out: &SampleOutHandle) -> Result<Vec<u16>, GfxError> {
        self.inner.read_sample_out(out)
    }

    fn clear_sample_out(&self, out: &mut SampleOutHandle) -> Result<(), GfxError> {
        self.inner.clear_sample_out(out)
    }
}

/// Shader decorator recording call durations.
struct TimingShader {
    inner: Box<dyn LpShader>,
    timings: Arc<ShaderTimings>,
}

impl LpShader for TimingShader {
    fn render(
        &mut self,
        target: &mut TextureHandle,
        uniforms: &LpsValueF32,
    ) -> Result<(), GfxError> {
        let start = Instant::now();
        let result = self.inner.render(target, uniforms);
        self.timings
            .renders
            .lock()
            .expect("timings lock")
            .push(start.elapsed());
        result
    }

    fn sample_rgba16(
        &mut self,
        points: &mut SamplePointsHandle,
        out: &mut SampleOutHandle,
        uniforms: &LpsValueF32,
    ) -> Result<(), GfxError> {
        let count = points.count();
        let start = Instant::now();
        let result = self.inner.sample_rgba16(points, out, uniforms);
        self.timings
            .samples
            .lock()
            .expect("timings lock")
            .push((count, start.elapsed()));
        result
    }

    fn compile_stats(&self) -> Option<ShaderCompileStats> {
        self.inner.compile_stats()
    }
}
