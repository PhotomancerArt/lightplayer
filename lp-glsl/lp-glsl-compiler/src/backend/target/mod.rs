//! Target architecture and codegen options

pub mod builder;
pub mod target;

pub use target::Target;
#[cfg(not(feature = "std"))]
pub use target::default_riscv32_embedded_jit_flags;
