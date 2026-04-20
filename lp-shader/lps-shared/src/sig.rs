//! Function signature shapes (no registry / overload resolution).

use alloc::{string::String, vec::Vec};

use crate::{LayoutRules, LpsType, VMCTX_HEADER_SIZE};

/// Whether a function in `LpsModuleSig.functions` is user-authored
/// or synthesised by the toolchain.
///
/// Today the toolchain emits two synthetic families: `__shader_init` (constant
/// global initialisation from the `lps-frontend` lower pass) and
/// `__render_texture_<format>` (pixel loop + Q32→unorm16 stores from
/// `lp-shader`'s synthesis — see `lp-shader/lp-shader/src/synth/render_texture.rs`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LpsFnKind {
    /// Lowered from user GLSL.
    #[default]
    UserDefined,
    /// Synthesised by the toolchain — e.g. `__shader_init` (frontend lower) or
    /// `__render_texture_<format>` (`lp-shader`). Convention: name begins with `__`.
    Synthetic,
}

/// Signature for LightPlayer Shader functions
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LpsFnSig {
    pub name: String,
    pub return_type: LpsType,
    pub parameters: Vec<FnParam>,
    pub kind: LpsFnKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FnParam {
    pub name: String,
    pub ty: LpsType,
    pub qualifier: ParamQualifier,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamQualifier {
    In,
    Out,
    InOut,
}

/// Full module metadata from GLSL lowering through compile.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LpsModuleSig {
    /// One entry per function in the module — user-authored *and*
    /// toolchain-synthesised. Filter via [`LpsFnSig::kind`] to distinguish;
    /// e.g. `f.kind == LpsFnKind::UserDefined` for user code only.
    pub functions: Vec<LpsFnSig>,
    /// Struct type describing the uniforms region layout (std430).
    /// Members correspond to `uniform` declarations. `None` if no uniforms.
    pub uniforms_type: Option<LpsType>,
    /// Struct type describing the globals region layout (std430).
    /// Members correspond to private global declarations. `None` if no globals.
    pub globals_type: Option<LpsType>,
}

impl LpsModuleSig {
    /// Compute the size of the uniforms region in bytes (std430 layout).
    pub fn uniforms_size(&self) -> usize {
        self.uniforms_type
            .as_ref()
            .map(|t| crate::layout::type_size(t, LayoutRules::Std430))
            .unwrap_or(0)
    }

    /// Compute the size of the globals region in bytes (std430 layout).
    pub fn globals_size(&self) -> usize {
        self.globals_type
            .as_ref()
            .map(|t| crate::layout::type_size(t, LayoutRules::Std430))
            .unwrap_or(0)
    }

    /// Compute the total VMContext buffer size:
    /// header + uniforms + globals + snapshot
    pub fn vmctx_buffer_size(&self) -> usize {
        let uniforms_size = self.uniforms_size();
        let globals_size = self.globals_size();
        VMCTX_HEADER_SIZE + uniforms_size + 2 * globals_size
    }

    /// Offset to the uniforms region (after header).
    pub fn uniforms_offset(&self) -> usize {
        VMCTX_HEADER_SIZE
    }

    /// Offset to the globals region (after header + uniforms).
    pub fn globals_offset(&self) -> usize {
        VMCTX_HEADER_SIZE + self.uniforms_size()
    }

    /// Offset to the globals snapshot region (after globals).
    pub fn snapshot_offset(&self) -> usize {
        VMCTX_HEADER_SIZE + self.uniforms_size() + self.globals_size()
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;
    use crate::LayoutRules;

    #[test]
    fn minimal_module_meta_fields() {
        let m = LpsModuleSig {
            functions: vec![LpsFnSig {
                name: String::from("add"),
                parameters: vec![
                    FnParam {
                        name: String::from("a"),
                        ty: LpsType::Float,
                        qualifier: ParamQualifier::In,
                    },
                    FnParam {
                        name: String::from("b"),
                        ty: LpsType::Float,
                        qualifier: ParamQualifier::In,
                    },
                ],
                return_type: LpsType::Float,
                kind: LpsFnKind::UserDefined,
            }],
            uniforms_type: None,
            globals_type: None,
        };
        assert_eq!(m.functions[0].name, "add");
        assert_eq!(m.functions[0].parameters.len(), 2);
        assert_eq!(m.functions[0].return_type, LpsType::Float);
        assert_eq!(m.functions[0].kind, LpsFnKind::UserDefined);
    }

    #[test]
    fn fn_kind_default_is_user_defined() {
        assert_eq!(LpsFnKind::default(), LpsFnKind::UserDefined);
    }

    #[test]
    fn layout_rules_std430_implemented() {
        assert!(LayoutRules::Std430.is_implemented());
        assert!(!LayoutRules::Std140.is_implemented());
    }
}
