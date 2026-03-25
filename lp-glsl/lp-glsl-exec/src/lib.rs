//! Trait for calling compiled GLSL user functions (`GlslExecutable`).
//!
//! Backends (`lp-glsl-wasm`, `lpir-cranelift` adapters, legacy `lp-glsl-cranelift`)
//! should implement this trait. JIT-only helpers such as `get_direct_call_info` stay
//! in `lp-glsl-cranelift` / `lp-glsl-jit-util` until those APIs are redesigned.

#![no_std]

extern crate alloc;

mod executable;

pub use executable::GlslExecutable;
