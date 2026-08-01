//! [`LpvmEngine`] implementation for native → linked → emulated execution.
//!
//! Requires crate feature `emu` (enables std + linking + emulation dependencies)
//! for the rv32 path, and additionally `emu-xt` for the Xtensa path.
//!
//! ## One engine, two ISAs
//!
//! [`NativeEmuEngine`] is parameterized by [`crate::IsaTarget`] rather than
//! duplicated per ISA. Of `instance.rs`'s ~1,200 production lines only the
//! emulator construction and the call itself are ISA-specific; the vmctx,
//! uniform, global, snapshot, texture, fuel and Q32 plumbing is neutral, and all
//! host-side read-back goes through the shared arena rather than the emulator.
//! Keeping one engine also keeps `LpvmEngine`/`LpvmModule`/`LpvmInstance`
//! ISA-agnostic **types**, so no downstream consumer (`lps-filetests`,
//! `lp-shader`) ever grows per-ISA match arms.
//!
//! See `docs/adr/2026-07-30-isa-parameterized-host-emu-engine.md`.

pub mod engine;
pub mod image;
pub mod instance;
pub mod module;
#[cfg(feature = "emu-xt")]
pub mod xt_image;

pub use engine::NativeEmuEngine;
pub use image::{GuestImage, ImageRegion};
pub use instance::NativeEmuInstance;
pub use module::NativeEmuModule;
