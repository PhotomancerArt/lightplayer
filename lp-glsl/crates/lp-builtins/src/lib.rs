#![cfg_attr(not(feature = "std"), no_std)]

//! Light Player builtins library.
//!
//! This crate provides low-level builtin functions for the Light Player compiler.
//! Functions are exported with `#[no_mangle] pub extern "C"` for linking.

// mem module provides memcpy/memset/memcmp for no_std environments
pub mod builtins;
pub mod glsl;
pub mod host;
pub mod mem;
pub mod util;
// Panic handler must be provided by the executable that uses this library
// This crate is only used as a dependency, never built standalone
