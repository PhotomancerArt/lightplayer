//! LightPlayer CPU graphics backend (`no_std + alloc`).
//!
//! One generic [`LpvmGraphics<B>`] implements [`lp_gfx::LpGraphics`] over any
//! [`lpvm::LpvmEngine`], replacing the previous per-target copies. The
//! concrete engine is target-selected (no Cargo feature):
//!
//! | Target                         | Engine                                | Backend name             |
//! |--------------------------------|---------------------------------------|--------------------------|
//! | `cfg(target_arch = "riscv32")` | `lpvm_native::NativeJitEngine`        | `lpvm-native::rt_jit`    |
//! | `cfg(target_arch = "wasm32")`  | `lpvm_wasm::rt_browser`               | `lpvm-wasm::rt_browser`  |
//! | catchall (host)                | `lpvm_wasm::rt_wasmtime`              | `lpvm-wasm::rt_wasmtime` |
//!
//! Construct the per-target backend with [`TargetLpvmGraphics::new`] (or name
//! the type via [`TargetLpvmEngine`] / [`TargetLpvmGraphics`]), passing the
//! host's explicit [`lp_shader::ShaderFrontend`] product decision.
//!
//! This is the **guaranteed** CPU backend of the lp-gfx doctrine: always
//! present on every target, always able to compile Q32 shaders. It refuses
//! non-Q32 [`lp_gfx::ShaderSemantics`] explicitly rather than approximating.

#![no_std]

extern crate alloc;

pub mod lpvm_compute_shader;
pub mod lpvm_graphics;
pub mod lpvm_shader;
pub mod target_backend;

pub use lpvm_graphics::LpvmGraphics;
pub use target_backend::{TargetLpvmEngine, TargetLpvmGraphics};
