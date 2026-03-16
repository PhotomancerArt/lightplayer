//! Host function implementations and registry.

#[cfg(feature = "std")]
mod impls;
mod registry;

pub use registry::{HostId, declare_host_functions, get_host_function_pointer};

#[cfg(feature = "std")]
pub use impls::__host_log;

// Extern function declarations for no_std mode.
// Users must provide implementations for these functions.
#[cfg(not(feature = "std"))]
unsafe extern "C" {
    // User-provided log function (must be defined by user in no_std mode)
    // Signature: `fn(level: u8, module_path_ptr: *const u8, module_path_len: usize, msg_ptr: *const u8, msg_len: usize)`
    pub fn lp_jit_host_log(
        level: u8,
        module_path_ptr: *const u8,
        module_path_len: usize,
        msg_ptr: *const u8,
        msg_len: usize,
    );
}
