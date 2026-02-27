//! Pass for extracting function bodies from the AST

use super::SemanticPass;
use super::function_signature;
use crate::error::{GlslDiagnostics, GlslError};
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

impl SemanticPass for FunctionExtractionPass {
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
            if let glsl::syntax::ExternalDeclaration::FunctionDefinition(func) = decl {
                match extract_function_body(func) {
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

    fn name(&self) -> &str {
        "function_extraction"
    }
}

fn extract_function_body(
    func: &glsl::syntax::FunctionDefinition,
) -> Result<TypedFunction, GlslError> {
    let sig = function_signature::extract_function_signature(&func.prototype)?;
    let body = func.statement.statement_list.clone();

    Ok(TypedFunction {
        name: sig.name,
        return_type: sig.return_type,
        parameters: sig.parameters,
        body,
    })
}
