//! Generic CPU implementation of [`LpGraphics`] over any [`LpvmEngine`].

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use lp_gfx::{
    GfxError, HandleAllocator, HandleBacking, LpComputeShader, LpGraphics, LpShader,
    SampleOutHandle, SamplePointsHandle, ShaderCompileOptions, ShaderSemantics, TextureData,
    TextureHandle,
};
use lp_shader::{
    CompilePxDesc, LpsEngine, LpsSamplePointBuf, LpsSampleRgba16Buf, LpsTextureBuf, TextureBuffer,
};
use lps_shared::TextureStorageFormat;
use lpvm::LpvmEngine;

use crate::lpvm_compute_shader::LpvmComputeShader;
use crate::lpvm_shader::LpvmShader;

/// CPU shader graphics backed by an [`LpsEngine`] over any LPVM engine `B`.
///
/// The engine lives behind an `Arc` shared with every handle this backend
/// allocates: handles free their buffers through it on drop (RAII) and keep
/// the engine's shared memory alive while they exist.
pub struct LpvmGraphics<B: LpvmEngine> {
    shared: Arc<SharedEngine<B>>,
    backend_name: &'static str,
}

impl<B> LpvmGraphics<B>
where
    B: LpvmEngine + Send + Sync + 'static,
    B::Module: 'static,
{
    /// Wrap an LPVM engine. `backend_name` is the log label
    /// (e.g. `"lpvm-wasm::rt_wasmtime"`).
    pub fn from_engine(backend: B, backend_name: &'static str) -> Self {
        Self {
            shared: Arc::new(SharedEngine {
                engine: LpsEngine::new(backend),
            }),
            backend_name,
        }
    }

    fn allocator(&self) -> Arc<dyn HandleAllocator> {
        self.shared.clone()
    }

    fn texture_handle(&self, buffer: LpsTextureBuf) -> TextureHandle {
        TextureHandle::from_backend_parts(
            buffer.width(),
            buffer.height(),
            buffer.format(),
            Box::new(buffer),
            self.allocator(),
        )
    }
}

impl<B> LpGraphics for LpvmGraphics<B>
where
    B: LpvmEngine + Send + Sync + 'static,
    B::Module: 'static,
{
    fn compile_shader(
        &self,
        source: &str,
        options: &ShaderCompileOptions,
    ) -> Result<Box<dyn LpShader>, GfxError> {
        if options.semantics != ShaderSemantics::Q32 {
            return Err(GfxError::Backend(format!(
                "lpvm CPU backend only compiles Q32 semantics; explicit {:?} tier requested",
                options.semantics
            )));
        }
        let cfg = options.to_compiler_config();
        let px = self
            .shared
            .engine
            .compile_px_desc(
                CompilePxDesc::new(source, lps_shared::TextureStorageFormat::Rgba16Unorm, cfg)
                    .with_frontend(options.frontend),
            )
            .map_err(|e| GfxError::Compile(format!("{e}")))?;

        let _ = options.max_errors; // TODO: thread max_errors when front-end accepts it

        Ok(Box::new(LpvmShader::new(px)))
    }

    fn compile_compute_shader(
        &self,
        desc: lp_shader::CompileComputeDesc<'_>,
    ) -> Result<Box<dyn LpComputeShader>, GfxError> {
        let shader = self
            .shared
            .engine
            .compile_compute_desc(desc)
            .map_err(|e| GfxError::Compile(format!("{e}")))?;
        Ok(Box::new(LpvmComputeShader::new(shader)))
    }

    fn backend_name(&self) -> &'static str {
        self.backend_name
    }

    fn create_render_target(&self, width: u32, height: u32) -> Result<TextureHandle, GfxError> {
        let buffer = self
            .shared
            .engine
            .alloc_texture(width, height, lps_shared::TextureStorageFormat::Rgba16Unorm)
            .map_err(|e| GfxError::Alloc(format!("alloc texture: {e:?}")))?;
        Ok(self.texture_handle(buffer))
    }

    fn create_texture(
        &self,
        width: u32,
        height: u32,
        format: TextureStorageFormat,
        texels: &[u8],
    ) -> Result<TextureHandle, GfxError> {
        let mut buffer = self
            .shared
            .engine
            .alloc_texture(width, height, format)
            .map_err(|e| GfxError::Alloc(format!("alloc texture: {e:?}")))?;
        let dst = buffer.data_mut();
        if dst.len() != texels.len() {
            let mismatch = len_mismatch("texture texels", dst.len(), texels.len());
            self.shared.engine.free_texture(buffer);
            return Err(mismatch);
        }
        dst.copy_from_slice(texels);
        Ok(self.texture_handle(buffer))
    }

    fn write_texture(&self, texture: &mut TextureHandle, texels: &[u8]) -> Result<(), GfxError> {
        let buffer = texture_buf_mut(texture)?;
        let dst = buffer.data_mut();
        if dst.len() != texels.len() {
            return Err(len_mismatch("texture texels", dst.len(), texels.len()));
        }
        dst.copy_from_slice(texels);
        Ok(())
    }

    fn clear_texture(&self, texture: &mut TextureHandle) -> Result<(), GfxError> {
        texture_buf_mut(texture)?.data_mut().fill(0);
        Ok(())
    }

    /// CPU blend over the backing byte buffers (byte-identical to the
    /// playlist crossfade this replaced: `mix` in f32 over raw u16 channel
    /// values, `+0.5` rounding, saturating to `[0, 65535]`).
    fn blend_textures(
        &self,
        previous: &TextureHandle,
        active: &TextureHandle,
        alpha: f32,
        target: &mut TextureHandle,
    ) -> Result<(), GfxError> {
        let previous = texture_buf(previous)?.data();
        let active = texture_buf(active)?.data();
        let target = texture_buf_mut(target)?.data_mut();
        if previous.len() != active.len() || previous.len() != target.len() {
            return Err(GfxError::Backend(String::from(
                "blend_textures: texture length mismatch",
            )));
        }
        let alpha = clamp01(alpha);
        for ((prev, next), out) in previous
            .chunks_exact(2)
            .zip(active.chunks_exact(2))
            .zip(target.chunks_exact_mut(2))
        {
            let a = u16::from_le_bytes([prev[0], prev[1]]) as f32;
            let b = u16::from_le_bytes([next[0], next[1]]) as f32;
            let mixed = mix_u16(a, b, alpha);
            out.copy_from_slice(&mixed.to_le_bytes());
        }
        Ok(())
    }

    fn read_back(&self, texture: &TextureHandle) -> Result<TextureData, GfxError> {
        let buffer = texture_buf(texture)?;
        Ok(TextureData::new(
            buffer.width(),
            buffer.height(),
            buffer.format(),
            buffer.data().to_vec(),
        ))
    }

    fn create_sample_points(&self, count: u32) -> Result<SamplePointsHandle, GfxError> {
        let buffer = self
            .shared
            .engine
            .alloc_sample_points(count)
            .map_err(|e| GfxError::Alloc(format!("alloc sample points: {e:?}")))?;
        Ok(SamplePointsHandle::from_backend_parts(
            count,
            Box::new(buffer),
            self.allocator(),
        ))
    }

    fn write_sample_points(
        &self,
        points: &mut SamplePointsHandle,
        xy_q16: &[i32],
    ) -> Result<(), GfxError> {
        let buffer = sample_points_buf_mut(points)?;
        let dst = buffer.data_mut();
        if dst.len() != xy_q16.len() {
            return Err(len_mismatch(
                "sample point coordinates",
                dst.len(),
                xy_q16.len(),
            ));
        }
        dst.copy_from_slice(xy_q16);
        Ok(())
    }

    fn read_sample_points(&self, points: &SamplePointsHandle) -> Result<Vec<i32>, GfxError> {
        Ok(sample_points_buf(points)?.data().to_vec())
    }

    fn create_sample_out(&self, count: u32) -> Result<SampleOutHandle, GfxError> {
        let buffer = self
            .shared
            .engine
            .alloc_sample_rgba16(count)
            .map_err(|e| GfxError::Alloc(format!("alloc sample rgba16: {e:?}")))?;
        Ok(SampleOutHandle::from_backend_parts(
            count,
            Box::new(buffer),
            self.allocator(),
        ))
    }

    fn write_sample_out(&self, out: &mut SampleOutHandle, rgba16: &[u16]) -> Result<(), GfxError> {
        let buffer = sample_out_buf_mut(out)?;
        let dst = buffer.data_mut();
        if dst.len() != rgba16.len() {
            return Err(len_mismatch("sample out channels", dst.len(), rgba16.len()));
        }
        dst.copy_from_slice(rgba16);
        Ok(())
    }

    fn read_sample_out(&self, out: &SampleOutHandle) -> Result<Vec<u16>, GfxError> {
        Ok(sample_out_buf(out)?.data().to_vec())
    }

    fn clear_sample_out(&self, out: &mut SampleOutHandle) -> Result<(), GfxError> {
        sample_out_buf_mut(out)?.data_mut().fill(0);
        Ok(())
    }
}

/// Engine shared between the backend facade and every live handle.
struct SharedEngine<B: LpvmEngine> {
    engine: LpsEngine<B>,
}

impl<B> HandleAllocator for SharedEngine<B>
where
    B: LpvmEngine + Send + Sync,
{
    fn free_texture(&self, backing: HandleBacking) {
        match backing.downcast::<LpsTextureBuf>() {
            Ok(buffer) => self.engine.free_texture(*buffer),
            Err(_) => debug_assert!(false, "texture handle backing was not an LpsTextureBuf"),
        }
    }

    fn free_sample_points(&self, backing: HandleBacking) {
        match backing.downcast::<LpsSamplePointBuf>() {
            Ok(buffer) => self.engine.free_sample_points(*buffer),
            Err(_) => debug_assert!(
                false,
                "sample point handle backing was not an LpsSamplePointBuf"
            ),
        }
    }

    fn free_sample_out(&self, backing: HandleBacking) {
        match backing.downcast::<LpsSampleRgba16Buf>() {
            Ok(buffer) => self.engine.free_sample_rgba16(*buffer),
            Err(_) => debug_assert!(
                false,
                "sample out handle backing was not an LpsSampleRgba16Buf"
            ),
        }
    }
}

/// Blend one u16 channel (moved verbatim from the playlist node's crossfade
/// so the CPU tier stays byte-identical).
fn mix_u16(a: f32, b: f32, alpha: f32) -> u16 {
    let mixed = a * (1.0 - alpha) + b * alpha + 0.5;
    if mixed <= 0.0 {
        0
    } else if mixed >= u16::MAX as f32 {
        u16::MAX
    } else {
        mixed as u16
    }
}

/// Saturating `[0, 1]` clamp (moved verbatim from the playlist node).
fn clamp01(value: f32) -> f32 {
    if value <= 0.0 {
        0.0
    } else if value >= 1.0 {
        1.0
    } else {
        value
    }
}

fn foreign_handle() -> GfxError {
    GfxError::Backend(String::from(
        "handle does not belong to this lpvm graphics backend",
    ))
}

/// Shared write-length mismatch error (one `format!` site for all writes).
fn len_mismatch(what: &'static str, expected: usize, got: usize) -> GfxError {
    GfxError::Backend(format!(
        "{what} write length mismatch: expected {expected}, got {got}"
    ))
}

/// Downcast a texture handle to its LPVM backing (backend-internal).
pub(crate) fn texture_buf(handle: &TextureHandle) -> Result<&LpsTextureBuf, GfxError> {
    handle
        .backing()
        .downcast_ref::<LpsTextureBuf>()
        .ok_or_else(foreign_handle)
}

/// Mutable variant of [`texture_buf`].
pub(crate) fn texture_buf_mut(handle: &mut TextureHandle) -> Result<&mut LpsTextureBuf, GfxError> {
    handle
        .backing_mut()
        .downcast_mut::<LpsTextureBuf>()
        .ok_or_else(foreign_handle)
}

pub(crate) fn sample_points_buf(
    handle: &SamplePointsHandle,
) -> Result<&LpsSamplePointBuf, GfxError> {
    handle
        .backing()
        .downcast_ref::<LpsSamplePointBuf>()
        .ok_or_else(foreign_handle)
}

pub(crate) fn sample_points_buf_mut(
    handle: &mut SamplePointsHandle,
) -> Result<&mut LpsSamplePointBuf, GfxError> {
    handle
        .backing_mut()
        .downcast_mut::<LpsSamplePointBuf>()
        .ok_or_else(foreign_handle)
}

pub(crate) fn sample_out_buf(handle: &SampleOutHandle) -> Result<&LpsSampleRgba16Buf, GfxError> {
    handle
        .backing()
        .downcast_ref::<LpsSampleRgba16Buf>()
        .ok_or_else(foreign_handle)
}

pub(crate) fn sample_out_buf_mut(
    handle: &mut SampleOutHandle,
) -> Result<&mut LpsSampleRgba16Buf, GfxError> {
    handle
        .backing_mut()
        .downcast_mut::<LpsSampleRgba16Buf>()
        .ok_or_else(foreign_handle)
}
