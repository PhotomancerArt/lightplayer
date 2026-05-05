use crate::legacy::glsl_opts::GlslOpts;
use crate::node::NodeKind;
use crate::node::node_def::NodeDef;
use lpc_model::NodeLoc;
use lpc_model::{AsLpPathBuf, LpPathBuf};
use serde::{Deserialize, Serialize};

/// Authored shader node definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShaderDef {
    /// Path to the GLSL source, relative to this artifact file.
    pub glsl_path: LpPathBuf,
    /// Texture node locator to render into.
    pub texture_loc: NodeLoc,
    /// Render order - lower numbers render first (default 0)
    pub render_order: i32,
    /// GLSL compilation options
    #[serde(default)]
    pub glsl_opts: GlslOpts,
}

impl Default for ShaderDef {
    fn default() -> Self {
        Self {
            glsl_path: "main.glsl".as_path_buf(),
            texture_loc: NodeLoc::from(""),
            render_order: 0,
            glsl_opts: GlslOpts::default(),
        }
    }
}

impl NodeDef for ShaderDef {
    fn kind(&self) -> NodeKind {
        NodeKind::Shader
    }

    fn as_any(&self) -> &dyn core::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::legacy::glsl_opts::{AddSubMode, DivMode, MulMode};

    #[test]
    fn test_shader_def_kind() {
        let def = ShaderDef {
            glsl_path: "main.glsl".as_path_buf(),
            texture_loc: NodeLoc::from("..tex_texture"),
            render_order: 0,
            glsl_opts: GlslOpts::default(),
        };
        assert_eq!(def.kind(), NodeKind::Shader);
    }

    #[test]
    fn test_shader_def_default() {
        let def = ShaderDef::default();
        assert_eq!(def.glsl_path.as_str(), "main.glsl");
        assert_eq!(def.render_order, 0);
        assert_eq!(def.glsl_opts.add_sub, AddSubMode::Saturating);
        assert_eq!(def.glsl_opts.mul, MulMode::Saturating);
        assert_eq!(def.glsl_opts.div, DivMode::Saturating);
    }
}
