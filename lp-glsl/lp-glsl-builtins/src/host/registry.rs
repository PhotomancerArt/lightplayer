//! Host function registry implementation.
//!
//! Provides enum-based registry for host functions with support for both
//! JIT (function pointer) and emulator (ELF symbol) linking.

/// Enum identifying host functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HostFn {
    Log,
}

impl HostFn {
    /// Get the symbol name for this host function.
    pub fn name(&self) -> &'static str {
        match self {
            HostFn::Log => "__host_log",
        }
    }

    /// Get all host IDs.
    pub fn all() -> &'static [HostFn] {
        &[HostFn::Log]
    }
}
