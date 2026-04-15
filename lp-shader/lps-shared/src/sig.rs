//! Function signature shapes (no registry / overload resolution).

use alloc::{string::String, vec::Vec};

use crate::LpsType;

/// Signature for LightPlayer Shader functions
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LpsFnSig {
    pub name: String,
    pub return_type: LpsType,
    pub parameters: Vec<FnParam>,
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

/// Full module metadata from GLSL lowering (one entry per user function).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LpsModuleSig {
    pub functions: Vec<LpsFnSig>,
    /// Struct type describing the uniforms region layout (std430).
    /// Members correspond to `uniform` declarations. `None` if no uniforms.
    pub uniforms_type: Option<LpsType>,
    /// Struct type describing the globals region layout (std430).
    /// Members correspond to private global declarations. `None` if no globals.
    pub globals_type: Option<LpsType>,
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
            }],
            uniforms_type: None,
            globals_type: None,
        };
        assert_eq!(m.functions[0].name, "add");
        assert_eq!(m.functions[0].parameters.len(), 2);
        assert_eq!(m.functions[0].return_type, LpsType::Float);
    }

    #[test]
    fn layout_rules_std430_implemented() {
        assert!(LayoutRules::Std430.is_implemented());
        assert!(!LayoutRules::Std140.is_implemented());
    }
}
