//! Graphics abstraction (`LpGraphics` / `LpShader`): boundary between the engine and shader backends.

pub mod lp_gfx;
pub mod lp_shader;
pub(crate) mod uniforms;

#[cfg(feature = "cranelift")]
pub mod cranelift;

#[cfg(all(target_arch = "riscv32", feature = "native-jit"))]
pub mod native_jit;

pub use lp_gfx::LpGraphics;
pub use lp_shader::{LpShader, ShaderCompileOptions};

#[cfg(feature = "cranelift")]
pub use cranelift::CraneliftGraphics;

#[cfg(all(target_arch = "riscv32", feature = "native-jit"))]
pub use native_jit::NativeJitGraphics;
