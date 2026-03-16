pub mod builtins;
pub mod codegen;
pub mod host; // Available in both std and no_std (impls submodule is std-only)
#[cfg(not(feature = "std"))]
pub mod memory; // Alloc-based memory provider for no_std
pub mod module;
pub mod q32;
pub mod target;
#[cfg(feature = "std")]
pub mod util;
