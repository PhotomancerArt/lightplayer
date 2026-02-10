//! GLSL compilation options (per-shader-node)

use serde::{Deserialize, Serialize};

/// Mode for Q32 add/sub: saturating (builtin) or wrapping (inline iadd/isub)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AddSubMode {
    /// __lp_q32_add/sub: saturates on overflow
    #[default]
    Saturating,
    /// Inline iadd/isub: wraps on overflow, faster
    Wrapping,
}

/// Mode for Q32 mul: saturating (builtin) or wrapping (inline imul+smulhi)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MulMode {
    /// __lp_q32_mul: saturates on overflow
    #[default]
    Saturating,
    /// Inline imul+smulhi: wraps on overflow, faster
    Wrapping,
}

/// Mode for Q32 div: saturating (builtin) or reciprocal (inline approximate)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DivMode {
    /// __lp_q32_div: exact, saturates on div-by-zero
    #[default]
    Saturating,
    /// Reciprocal multiplication: ~0.01% typical error, faster
    Reciprocal,
}

/// GLSL compilation options (per-shader-node)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlslOpts {
    #[serde(default)]
    pub add_sub: AddSubMode,
    #[serde(default)]
    pub mul: MulMode,
    #[serde(default)]
    pub div: DivMode,
}

impl Default for GlslOpts {
    fn default() -> Self {
        Self {
            add_sub: AddSubMode::default(),
            mul: MulMode::default(),
            div: DivMode::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glsl_opts_default() {
        let opts = GlslOpts::default();
        assert_eq!(opts.add_sub, AddSubMode::Saturating);
        assert_eq!(opts.mul, MulMode::Saturating);
        assert_eq!(opts.div, DivMode::Saturating);
    }
}
