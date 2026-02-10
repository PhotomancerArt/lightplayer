//! GLSL compilation options (per-shader-node)

use serde::{Deserialize, Serialize};

/// GLSL compilation options (per-shader-node)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlslOpts {
    /// Use inline iadd/isub for add/sub (wrapping) instead of saturating builtins
    #[serde(default)]
    pub fast_math: bool,
}

impl Default for GlslOpts {
    fn default() -> Self {
        Self { fast_math: false }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glsl_opts_default() {
        let opts = GlslOpts::default();
        assert!(!opts.fast_math);
    }
}
