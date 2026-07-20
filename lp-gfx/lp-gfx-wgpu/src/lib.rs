//! LightPlayer GPU graphics backend (`std` edge crate).
//!
//! [`GpuGraphics`] implements `lp_gfx::LpGraphics` on wgpu: authored GLSL is
//! assembled with the canonical lpfn prelude, translated through
//! naga `glsl-in` → IR passes → `wgsl-out`, and rendered as a fragment shader
//! on a fullscreen triangle at **IEEE f32 semantics**
//! (`ShaderSemantics::F32Gpu`). Compute shaders stay on the CPU tier
//! permanently and are delegated to an inner `LpGraphics`.
//!
//! # Semantics contract
//!
//! Per `docs/adr/2026-07-09-preview-fidelity-tiers.md` the requested
//! [`lp_gfx::ShaderSemantics`] tier is honored exactly or compilation fails:
//! this backend implements `F32Gpu` only and rejects `Q32` with
//! [`lp_gfx::GfxError::Backend`] — never a silent substitution.
//!
//! # Sans-IO edges
//!
//! The crate exposes no async: hosts create the wgpu `Device`/`Queue`
//! themselves (each platform does its own async adapter/device request at
//! its edge) and hand them to [`GpuGraphics::new`].
//!
//! # GPU residency
//!
//! Render products stay GPU-resident: transforms run behind trait ops
//! (`blend_textures` — a small fixed pipeline); `read_back` is for sinks
//! that inherently need bytes and is native-only (explicit error on the
//! browser tier). See the crate README for the full policy.

pub mod assembly;
pub mod blend;
pub mod gpu_graphics;
pub mod read_back;
pub mod render;
pub mod sample_backing;
#[cfg(not(target_arch = "wasm32"))]
pub mod sample_pass;
pub mod surface_blit;
pub mod tanh_pass;
pub mod texture_backing;
pub mod texture_lowering;
pub(crate) mod texture_registry;
pub mod uniform_layout;
pub mod uniform_writer;
pub mod wgsl_compile;

#[cfg(test)]
pub(crate) mod test_gpu;

pub use gpu_graphics::GpuGraphics;
