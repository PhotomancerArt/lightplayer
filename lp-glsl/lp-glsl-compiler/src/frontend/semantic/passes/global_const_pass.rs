//! Pass for collecting global const declarations.
//!
//! Walks ExternalDeclaration::Declaration(InitDeclaratorList), evaluates const
//! initializers, and populates the global_constants map.

use super::SemanticPass;
use crate::error::{GlslDiagnostics, GlslError, extract_span_from_expr, source_span_to_location};
use crate::frontend::semantic::const_eval::{self, ConstEnv, ConstValue};
use hashbrown::HashMap;

use alloc::string::String;

/// Check if a FullySpecifiedType has the const qualifier.
fn has_const_qualifier(ty: &glsl::syntax::FullySpecifiedType) -> bool {
    use glsl::syntax::{StorageQualifier, TypeQualifierSpec};

    let Some(ref type_qual) = ty.qualifier else {
        return false;
    };
    for spec in &type_qual.qualifiers.0 {
        if let TypeQualifierSpec::Storage(StorageQualifier::Const) = spec {
            return true;
        }
    }
    false
}

/// Result of the global const pass.
pub struct GlobalConstPassResult {
    pub global_constants: HashMap<String, ConstValue>,
}

pub struct GlobalConstPass {
    global_constants: HashMap<String, ConstValue>,
}

impl GlobalConstPass {
    pub fn new() -> Self {
        Self {
            global_constants: HashMap::new(),
        }
    }

    pub fn into_result(self) -> GlobalConstPassResult {
        GlobalConstPassResult {
            global_constants: self.global_constants,
        }
    }
}

impl Default for GlobalConstPass {
    fn default() -> Self {
        Self::new()
    }
}

impl SemanticPass for GlobalConstPass {
    fn run(
        &mut self,
        shader: &glsl::syntax::TranslationUnit,
        _source: &str,
        diagnostics: &mut GlslDiagnostics,
    ) {
        for decl in &shader.0 {
            if diagnostics.at_limit() {
                break;
            }
            let glsl::syntax::ExternalDeclaration::Declaration(inner) = decl else {
                continue;
            };
            let glsl::syntax::Declaration::InitDeclaratorList(list) = inner else {
                continue;
            };
            if !has_const_qualifier(&list.head.ty) {
                continue;
            }
            // Const declaration - process head only (single const per line typical)
            let name = match &list.head.name {
                Some(ident) => ident.name.clone(),
                None => continue,
            };
            let init = match &list.head.initializer {
                Some(i) => i,
                None => {
                    let span = list.head.name.as_ref().unwrap().span.clone();
                    let _ = diagnostics.push(
                        GlslError::new(
                            crate::error::ErrorCode::E0400,
                            format!("const `{name}` must be initialized"),
                        )
                        .with_location(source_span_to_location(&span)),
                    );
                    continue;
                }
            };
            let expr = match init {
                glsl::syntax::Initializer::Simple(e) => e,
                glsl::syntax::Initializer::List(_) => {
                    let _ = diagnostics.push(GlslError::new(
                        crate::error::ErrorCode::E0400,
                        format!("const `{name}` initializer list not yet supported"),
                    ));
                    continue;
                }
            };
            let const_env: ConstEnv = self.global_constants.clone();
            let span = extract_span_from_expr(expr);
            match const_eval::eval_constant_expr(expr, &const_env, Some(&span)) {
                Ok(val) => {
                    self.global_constants.insert(name, val);
                }
                Err(e) => {
                    let _ = diagnostics.push(e);
                    // Don't insert - const is invalid
                }
            }
        }
    }

    fn name(&self) -> &str {
        "global_const"
    }
}
