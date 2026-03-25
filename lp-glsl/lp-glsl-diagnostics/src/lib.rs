//! Structured GLSL errors and source locations.
//!
//! This crate is the **new** shared diagnostics layer. `lp-glsl-frontend` and
//! `lp-glsl-cranelift` retain their own copies until those crates are removed or
//! rewired to depend on this crate.
//!
//! Stack: `lp-glsl-diagnostics` → `lp-glsl-core` → `lp-glsl-values` → `lp-glsl-exec`.

#![no_std]

extern crate alloc;

mod error;
mod source_loc;

pub use error::{ErrorCode, GlslDiagnostics, GlslError};
pub use source_loc::{GlFileId, GlSourceLoc};

/// Default maximum errors collected in one compilation (matches legacy frontend default).
pub const DEFAULT_MAX_ERRORS: usize = 20;
