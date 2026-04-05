//! Structured GLSL errors and source locations.
//!
//! Shared diagnostics for the GLSL / LPIR test and exec stack.
//!
//! Stack: `lp-glsl-diagnostics` → `lps-types` → `lpvm` → `lp-glsl-exec`.

#![no_std]

extern crate alloc;

mod error;
mod source_loc;

pub use error::{ErrorCode, GlslDiagnostics, GlslError};
pub use source_loc::{GlFileId, GlSourceLoc};

/// Default maximum errors collected in one compilation (matches legacy frontend default).
pub const DEFAULT_MAX_ERRORS: usize = 20;
