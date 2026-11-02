//! The real CPU graphics tier behind a thin recorder: every program the
//! engine compiles, every uniform block it binds and every coordinate
//! window it samples at, in order. Adapted from `lpc-engine`'s
//! `tests/pattern_space.rs`.

use std::sync::{Arc, Mutex};

use lp_gfx::{
    GfxError, LpComputeShader, LpGraphics, LpShader, SampleOutHandle, SamplePointsHandle,
    ShaderCompileOptions, ShaderEntrySpace, ShaderSemantics, TextureData, TextureHandle,
};
use lps_shared::{LpsValueF32, TextureStorageFormat};

/// One program: its GLSL and the space it was compiled for.
pub struct Program {
    pub source: String,
    pub space: ShaderEntrySpace,
}

/// What a program was handed, in order.
pub enum Event {
    /// A uniform block bound for the next sample stream (or a texture
    /// render).
    Bind {
        program: usize,
        uniforms: LpsValueF32,
    },
    /// One sampled batch: the coordinate words the program saw.
    Batch { program: usize, words: Vec<i32> },
    /// A whole-texture render (texture-area sampling, previews).
    Render { program: usize },
}

#[derive(Default)]
pub struct Log {
    pub programs: Vec<Program>,
    pub events: Vec<Event>,
    /// When set, the program compiled from `swap.glsl` is handed other
    /// coordinates and uniforms, and what it answers is kept per lamp.
    pub swap: Option<Swap>,
}

/// A dev-only seam for the look pictures: where today's engine hands the
/// program one fixture's coordinates (`today`, found batch by batch), hand
/// it `words` instead, with `uniforms` laid over the bound block, and keep
/// the colour it answers for each lamp. The program, its compile and every
/// other uniform (time, phasors, palettes) are the engine's own.
pub struct Swap {
    pub glsl: String,
    pub lanes: usize,
    pub today: Vec<i32>,
    pub words: Vec<i32>,
    pub uniforms: Vec<(String, LpsValueF32)>,
    /// Each lamp's answer, RGBA16, from the last batch that sampled it.
    pub colors: Vec<Option<[u16; 4]>>,
}

impl Swap {
    /// Where `batch` sits in today's stream, in words.
    fn offset_of(&self, batch: &[i32]) -> Option<usize> {
        (0..self.today.len().saturating_sub(batch.len()) + 1)
            .step_by(self.lanes)
            .find(|&at| self.today[at..at + batch.len()] == *batch)
    }
}

pub struct RecordingGraphics {
    pub inner: Arc<lp_gfx_lpvm::TargetLpvmGraphics>,
    pub log: Arc<Mutex<Log>>,
}

struct RecordingShader {
    inner: Box<dyn LpShader>,
    graphics: Arc<lp_gfx_lpvm::TargetLpvmGraphics>,
    program: usize,
    space: ShaderEntrySpace,
    /// Compiled from the swap's GLSL.
    swapped: bool,
    log: Arc<Mutex<Log>>,
}

impl LpShader for RecordingShader {
    fn render(
        &mut self,
        target: &mut TextureHandle,
        uniforms: &LpsValueF32,
    ) -> Result<(), GfxError> {
        let mut log = self.log.lock().unwrap();
        log.events.push(Event::Bind {
            program: self.program,
            uniforms: uniforms.clone(),
        });
        log.events.push(Event::Render {
            program: self.program,
        });
        drop(log);
        self.inner.render(target, uniforms)
    }

    fn bind_uniforms(&mut self, uniforms: &LpsValueF32) -> Result<(), GfxError> {
        let mut log = self.log.lock().unwrap();
        log.events.push(Event::Bind {
            program: self.program,
            uniforms: uniforms.clone(),
        });
        let mut block = uniforms.clone();
        if self.swapped
            && let Some(swap) = &log.swap
            && let LpsValueF32::Struct { fields, .. } = &mut block
        {
            for (name, value) in &swap.uniforms {
                match fields.iter_mut().find(|(n, _)| n == name) {
                    Some(field) => field.1 = value.clone(),
                    None => fields.insert(1, (name.clone(), value.clone())),
                }
            }
        }
        drop(log);
        self.inner.bind_uniforms(&block)
    }

    fn sample_rgba16_bound(
        &mut self,
        points: &mut SamplePointsHandle,
        out: &mut SampleOutHandle,
        count: u32,
    ) -> Result<(), GfxError> {
        let lanes = match self.space {
            ShaderEntrySpace::TwoD => 2,
            ShaderEntrySpace::OneD => 1,
        };
        let words = self.graphics.read_sample_points(points)?;
        let batch = words[..count as usize * lanes].to_vec();
        let mut log = self.log.lock().unwrap();
        log.events.push(Event::Batch {
            program: self.program,
            words: batch.clone(),
        });
        let at = if self.swapped {
            log.swap.as_ref().and_then(|swap| swap.offset_of(&batch))
        } else {
            None
        };
        if let (Some(at), Some(swap)) = (at, &log.swap) {
            let data = self.graphics.sample_points_data_mut(points)?;
            data[..batch.len()].copy_from_slice(&swap.words[at..at + batch.len()]);
        }
        drop(log);
        self.inner.sample_rgba16_bound(points, out, count)?;
        if let Some(at) = at {
            let samples = self.graphics.read_sample_out(out)?;
            let mut log = self.log.lock().unwrap();
            let swap = log.swap.as_mut().expect("swap");
            let first = at / lanes;
            for (k, rgba) in samples[..count as usize * 4].chunks_exact(4).enumerate() {
                swap.colors[first + k] = Some([rgba[0], rgba[1], rgba[2], rgba[3]]);
            }
        }
        Ok(())
    }

    fn compile_stats(&self) -> Option<lp_gfx::ShaderCompileStats> {
        self.inner.compile_stats()
    }
}

impl LpGraphics for RecordingGraphics {
    fn compile_shader(
        &self,
        source: &str,
        options: &ShaderCompileOptions,
    ) -> Result<Box<dyn LpShader>, GfxError> {
        let inner = self.inner.compile_shader(source, options)?;
        let mut log = self.log.lock().unwrap();
        let program = log.programs.len();
        log.programs.push(Program {
            source: source.to_string(),
            space: options.space,
        });
        let swapped = log.swap.as_ref().is_some_and(|swap| swap.glsl == source);
        Ok(Box::new(RecordingShader {
            inner,
            graphics: Arc::clone(&self.inner),
            program,
            space: options.space,
            swapped,
            log: Arc::clone(&self.log),
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

    fn float_semantics(&self) -> ShaderSemantics {
        self.inner.float_semantics()
    }

    fn glsl_frontend(&self) -> lp_shader::ShaderFrontend {
        self.inner.glsl_frontend()
    }

    fn create_render_target(&self, width: u32, height: u32) -> Result<TextureHandle, GfxError> {
        self.inner.create_render_target(width, height)
    }

    fn texture_uniform_value(&self, texture: &TextureHandle) -> Result<LpsValueF32, GfxError> {
        self.inner.texture_uniform_value(texture)
    }

    fn supports_read_back(&self) -> bool {
        self.inner.supports_read_back()
    }

    fn write_sample_points_1d(
        &self,
        points: &mut SamplePointsHandle,
        t_q16: &[i32],
    ) -> Result<(), GfxError> {
        self.inner.write_sample_points_1d(points, t_q16)
    }

    fn read_sample_out(&self, out: &SampleOutHandle) -> Result<Vec<u16>, GfxError> {
        self.inner.read_sample_out(out)
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

    fn read_back_into(&self, texture: &TextureHandle, out: &mut [u8]) -> Result<(), GfxError> {
        self.inner.read_back_into(texture, out)
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

    fn read_sample_out_into(&self, out: &SampleOutHandle, dst: &mut [u16]) -> Result<(), GfxError> {
        self.inner.read_sample_out_into(out, dst)
    }

    fn sample_out_data<'a>(&self, out: &'a SampleOutHandle) -> Result<&'a [u16], GfxError> {
        self.inner.sample_out_data(out)
    }

    fn clear_sample_out(&self, out: &mut SampleOutHandle) -> Result<(), GfxError> {
        self.inner.clear_sample_out(out)
    }

    fn sample_points_data_mut<'a>(
        &self,
        points: &'a mut SamplePointsHandle,
    ) -> Result<&'a mut [i32], GfxError> {
        self.inner.sample_points_data_mut(points)
    }

    fn sample_batch_capacity(&self) -> u32 {
        self.inner.sample_batch_capacity()
    }
}
