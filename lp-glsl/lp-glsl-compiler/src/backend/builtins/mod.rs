//! Builtin function registry for linking external functions.
//!
//! This module provides a centralized registry for builtin functions that can be
//! linked into both JIT and emulator executables.

pub mod registry;

pub use registry::{BuiltinId, declare_builtins, declare_for_emulator, declare_for_jit};
