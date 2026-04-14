//! `LpvmModule` trait - compiled artifact with metadata.

use lps_shared::LpsModuleSig;

use crate::debug::ModuleDebugInfo;
use crate::instance::LpvmInstance;

/// A compiled shader module that can be instantiated for execution.
///
/// Modules are immutable after compilation. The `signatures()` method
/// provides access to function signatures for type checking and call
/// marshaling. Multiple instances can be created from one module,
/// each with independent execution state.
pub trait LpvmModule {
    /// Instance type produced by this module.
    type Instance: LpvmInstance;

    /// Error type for instantiation failures.
    type Error: core::fmt::Display;

    /// Get the function signatures for this module.
    fn signatures(&self) -> &LpsModuleSig;

    /// Create a new execution instance.
    ///
    /// The instance has independent VM state (fuel, globals, uniforms).
    /// Multiple instances can execute concurrently (subject to `Send` bounds).
    fn instantiate(&self) -> Result<Self::Instance, Self::Error>;

    /// Compilation debug info. Returns None if not available for this backend.
    fn debug_info(&self) -> Option<&ModuleDebugInfo> {
        None
    }
}
