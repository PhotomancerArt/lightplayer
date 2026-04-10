#![cfg_attr(not(feature = "std"), no_std)]

//! Light Player builtins library.
//!
//! This crate provides low-level builtin functions for the Light Player compiler.
//! Functions are exported with `#[no_mangle] pub extern "C"` for linking.

// mem module provides memcpy/memset/memcmp for no_std environments
pub mod builtin_refs;
pub mod builtins;
pub mod host;
pub mod jit_builtin_ptr;
pub mod mem;
pub mod util;

pub use builtin_refs::ensure_builtins_referenced;
pub use jit_builtin_ptr::jit_builtin_code_ptr;

// Panic handler must be provided by the executable that uses this library
// This crate is only used as a dependency, never built standalone
