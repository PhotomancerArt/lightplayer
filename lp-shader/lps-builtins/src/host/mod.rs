//! Host functions for cross-context communication.
//!
//! This module provides functions like `debug!` and `println!` that work
//! differently depending on execution context:
//! - Emulator: Functions defined in `lps-builtins-emu-app` (syscall-based)
//! - Tests: Functions defined here using `std` (gated by feature flag)
//! - JIT: Functions registered by `GlJitModule` (delegates to `lpvm-cranelift` host hooks)

mod logger;
mod macros;
mod registry;

pub use logger::init as init_logger;
pub use registry::HostFn;

// Macros are exported at crate root via #[macro_export]
// Users should use: lps_builtins::host_debug!
// Note: host_println! has been removed - use log::info! instead

// Function declarations are provided by:
// - Emulator: `lps-builtins-emu-app` (syscall-based)
// - Tests: `test` module (gated by feature flag)
// - JIT: `lpvm-cranelift` (registers host syscalls / logging)
//
// No default implementations here to avoid symbol conflicts when linking.

#[cfg(not(feature = "std"))]
mod no_std_format;

#[cfg(not(feature = "std"))]
pub use no_std_format::_debug_format;

#[cfg(feature = "test")]
mod test;

#[cfg(feature = "test")]
pub use test::__host_log;

#[cfg(test)]
#[cfg(feature = "test")]
mod tests;
