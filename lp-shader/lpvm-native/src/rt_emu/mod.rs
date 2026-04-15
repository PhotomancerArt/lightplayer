//! [`LpvmEngine`] implementation for native RV32 → linked → emulated execution.
//!
//! Requires crate feature `emu` (enables std + linking + emulation dependencies).

pub mod engine;
pub mod instance;
pub mod module;

pub use engine::NativeEmuEngine;
pub use instance::NativeEmuInstance;
pub use module::NativeEmuModule;
