//! Host function implementations and registry.

#[cfg(feature = "std")]
mod impls;
mod registry;

pub use registry::{HostId, declare_host_functions, get_host_function_pointer};

#[cfg(feature = "std")]
pub use impls::{__host_debug, __host_println};

// Extern function declarations for no_std mode.
// Users must provide implementations for these functions.
#[cfg(not(feature = "std"))]
unsafe extern "C" {
    // User-provided debug function (must be defined by user in no_std mode)
    // Signature: `fn(ptr: *const u8, len: usize)`
    pub fn lp_jit_host_debug(ptr: *const u8, len: usize);

    // User-provided print function (must be defined by user in no_std mode)
    // Signature: `fn(ptr: *const u8, len: usize)`
    pub fn lp_jit_host_println(ptr: *const u8, len: usize);
}
