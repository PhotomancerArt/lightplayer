//! Pass for collecting function signatures from the AST

use super::SemanticPass;
use super::function_signature;
use crate::error::GlslDiagnostics;
use crate::frontend::semantic::const_eval::ConstEnv;
use crate::frontend::semantic::functions::FunctionRegistry;

pub struct FunctionRegistryPass {
    registry: FunctionRegistry,
}

impl FunctionRegistryPass {
    pub fn new() -> Self {
        Self {
            registry: FunctionRegistry::new(),
        }
    }

    pub fn into_registry(self) -> FunctionRegistry {
        self.registry
    }

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
                match function_signature::extract_function_signature(&func.prototype, const_env) {
                    Ok(sig) => {
                        let _ = self.registry.register_function(sig);
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

impl SemanticPass for FunctionRegistryPass {
    fn run(
        &mut self,
        shader: &glsl::syntax::TranslationUnit,
        source: &str,
        diagnostics: &mut GlslDiagnostics,
    ) {
        self.run_with_const_env(shader, source, diagnostics, None);
    }

    fn name(&self) -> &str {
        "function_registry"
    }
}
