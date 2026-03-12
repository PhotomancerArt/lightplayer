//! Builtin function registry for linking external functions.
//!
//! This module provides a centralized registry for builtin functions that can be
//! linked into both JIT and emulator executables.

pub mod mapping;
pub mod registry;

pub use mapping::map_testcase_to_builtin;
pub use registry::{BuiltinId, declare_builtins, declare_for_emulator, declare_for_jit};
