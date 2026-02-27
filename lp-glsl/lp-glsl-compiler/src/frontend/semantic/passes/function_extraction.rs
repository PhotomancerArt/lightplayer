//! Pass for extracting function bodies from the AST

use super::SemanticPass;
use super::function_signature;
use crate::error::{GlslDiagnostics, GlslError};
use crate::frontend::semantic::const_eval::ConstEnv;
use crate::frontend::semantic::{MAIN_FUNCTION_NAME, TypedFunction};

use alloc::vec::Vec;

pub struct FunctionExtractionPass {
    main_func: Option<TypedFunction>,
    user_functions: Vec<TypedFunction>,
}

impl FunctionExtractionPass {
    pub fn new() -> Self {
        Self {
            main_func: None,
            user_functions: Vec::new(),
        }
    }

    pub fn into_results(self) -> (Option<TypedFunction>, Vec<TypedFunction>) {
        (self.main_func, self.user_functions)
    }
}

impl FunctionExtractionPass {
    /// Run with const environment for parameter array dimensions (e.g. float arr[N]).
    pub fn run_with_const_env(
        &mut self,
        shader: &glsl::syntax::TranslationUnit,
        _source: &str,
        diagnostics: &mut GlslDiagnostics,
        const_env: Option<&ConstEnv>,
    ) {
        for decl in &shader.0 {
            if diagnostics.at_limit() {
                break;
            }
            if let glsl::syntax::ExternalDeclaration::FunctionDefinition(func) = decl {
                match extract_function_body(func, const_env) {
                    Ok(typed_func) => {
                        if func.prototype.name.name == MAIN_FUNCTION_NAME {
                            self.main_func = Some(typed_func);
                        } else {
                            self.user_functions.push(typed_func);
                        }
                    }
                    Err(e) => {
                        if !diagnostics.push(e) {
                            break;
                        }
                    }
                }
            }
        }
    }
}

impl SemanticPass for FunctionExtractionPass {
    fn run(
        &mut self,
        shader: &glsl::syntax::TranslationUnit,
        source: &str,
        diagnostics: &mut GlslDiagnostics,
    ) {
        self.run_with_const_env(shader, source, diagnostics, None);
    }

    fn name(&self) -> &str {
        "function_extraction"
    }
}

fn extract_function_body(
    func: &glsl::syntax::FunctionDefinition,
    const_env: Option<&ConstEnv>,
) -> Result<TypedFunction, GlslError> {
    let sig = function_signature::extract_function_signature(&func.prototype, const_env)?;
    let body = func.statement.statement_list.clone();

    Ok(TypedFunction {
        name: sig.name,
        return_type: sig.return_type,
        parameters: sig.parameters,
        body,
    })
}
