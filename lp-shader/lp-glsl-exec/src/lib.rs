//! Trait for calling compiled GLSL user functions (`GlslExecutable`).
//!
//! Backends (`lp-glsl-wasm`, `lpir-cranelift` adapters) implement this trait for filetests
//! and tooling.

#![no_std]

extern crate alloc;

mod executable;

pub use executable::GlslExecutable;
