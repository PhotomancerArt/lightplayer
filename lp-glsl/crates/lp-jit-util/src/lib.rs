//! Utilities for calling and wrapping JIT-compiled functions.
//!
//! This crate provides abstractions for:
//! - Calling StructReturn functions with correct calling conventions
//! - Wrapping StructReturn functions into Rust-friendly closures
//! - Handling platform-specific ABI requirements

#![no_std]

extern crate alloc;

pub mod call;
pub mod error;
pub mod wrapper;

pub use call::{call_structreturn, call_structreturn_with_args};
pub use error::JitCallError;
pub use wrapper::{StructReturnWrapper, wrap_structreturn_function};
