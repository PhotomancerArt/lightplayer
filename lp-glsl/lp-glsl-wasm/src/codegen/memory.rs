//! Layout of compiler-reserved bytes in imported `env.memory` (shared with builtins).

/// Base byte offset for LPFX scratch (result pointers, `out` vectors).
pub const LPFX_OUT_PARAM_BASE: u32 = 0;

/// Reserved span large enough for a vec4 result plus out-vectors used in a single call.
pub const LPFX_SCRATCH_BYTES: u32 = 64;
