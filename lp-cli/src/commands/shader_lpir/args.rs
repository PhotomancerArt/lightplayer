//! Arguments for `shader-lpir`.

use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct ShaderLpirArgs {
    pub path: PathBuf,
    /// Print per-function op count and vreg count to stderr after LPIR text.
    pub stats: bool,
    /// Print LPIR even when validation fails (for debugging mismatches).
    pub skip_validate: bool,
}
