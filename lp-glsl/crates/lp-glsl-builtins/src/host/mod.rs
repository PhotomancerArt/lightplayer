//! Host functions for cross-context communication.
//!
//! This module provides functions like `debug!` and `println!` that work
//! differently depending on execution context:
//! - Emulator: Functions defined in `lp-glsl-builtins-emu-app` (syscall-based)
//! - Tests: Functions defined here using `std` (gated by feature flag)
//! - JIT: Functions registered by `GlJitModule` (delegate to `lp-glsl-compiler` macros)

mod macros;
mod registry;

pub use registry::HostFn;

// Macros are exported at crate root via #[macro_export]
// Users should use: lp_glsl_builtins::host_debug! and lp_glsl_builtins::host_println!
// Or we can re-export them here if needed

// Function declarations are provided by:
// - Emulator: `lp-glsl-builtins-emu-app` (syscall-based)
// - Tests: `test` module (gated by feature flag)
// - JIT: `lp-glsl-compiler` (delegates to `lp-glsl-compiler` macros)
//
// No default implementations here to avoid symbol conflicts when linking.

#[cfg(not(feature = "std"))]
mod no_std_format;

#[cfg(not(feature = "std"))]
pub use no_std_format::{_debug_format, _println_format};

#[cfg(feature = "test")]
mod test;

#[cfg(feature = "test")]
pub use test::{__host_debug, __host_println};

#[cfg(test)]
#[cfg(feature = "test")]
mod tests;
