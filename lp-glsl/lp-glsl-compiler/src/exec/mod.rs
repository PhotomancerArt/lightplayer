//! Execution backends for GLSL shaders.
//!
//! This module provides trait-based APIs and implementations for executing
//! compiled GLSL shaders, abstracting away JIT vs Emulator implementations.

#[cfg(feature = "emulator")]
pub(crate) mod emu;
pub(crate) mod executable;
#[cfg(feature = "std")]
pub mod execute_fn;
pub(crate) mod glsl_value;
pub(crate) mod jit;

#[cfg(feature = "emulator")]
pub use emu::GlslEmulatorModule;
pub use executable::{DecimalFormat, GlslExecutable, GlslOptions, RunMode};
pub use glsl_value::GlslValue;
pub use jit::GlslJitModule;
