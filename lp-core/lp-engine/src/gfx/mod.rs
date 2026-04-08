//! Graphics abstraction (`LpGraphics` / `LpShader`): boundary between the engine and shader backends.

pub mod lp_gfx;
pub mod lp_shader;

#[cfg(feature = "cranelift")]
pub mod cranelift;

pub use lp_gfx::LpGraphics;
pub use lp_shader::{LpShader, ShaderCompileOptions};

#[cfg(feature = "cranelift")]
pub use cranelift::CraneliftGraphics;
